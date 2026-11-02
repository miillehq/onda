//! Protected storage and cross-process locking for rotating account credentials: DPAPI on
//! Windows, a key in the desktop keyring on Linux.

use crate::{unix_time, AccountProfile, CloudError, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const MAX_STORED_BYTES: u64 = 1024 * 1024;
const STATE_FILE: &str = "host.json";
const PROFILE_DIRECTORY: &str = "accounts";
#[cfg(windows)]
const FILE_PREFIX: &[u8] = b"ONDA-DPAPI-1\0";
#[cfg(not(windows))]
const FILE_PREFIX: &[u8] = b"ONDA-KEYRING-1\0";
const PROTECTION_ENTROPY: &[u8] = b"onda/chatgpt/credential-record/v1";

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub(crate) struct SecretTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
    pub earliest_refresh_at: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ProfileRecord {
    pub id: String,
    pub client_id: String,
    pub issuer: String,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub preferred_model: Option<String>,
    pub tokens: Option<SecretTokens>,
    pub refresh_pending: bool,
}

impl ProfileRecord {
    pub fn public(&self, active: bool) -> AccountProfile {
        let identity = self
            .name
            .as_deref()
            .or(self.email.as_deref())
            .unwrap_or("ChatGPT account");
        AccountProfile {
            id: self.id.clone(),
            label: format!("{} · {}", identity, &self.id[..8]),
            email: self.email.clone(),
            signed_in: self.tokens.is_some(),
            plan_usage_enabled: self.tokens.as_ref().is_some_and(|tokens| {
                tokens
                    .scopes
                    .iter()
                    .any(|scope| scope == "chatgpt.tokens.use.direct")
            }),
            active,
            preferred_model: self.preferred_model.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct HostState {
    version: u32,
    ext_agent_host_id: String,
    active_profile: Option<String>,
}

/// Holding the handle holds an OS file lock; closing releases it after crashes.
pub(crate) struct StorageLock {
    _file: File,
}

#[derive(Clone)]
pub(crate) struct AccountStore {
    root: PathBuf,
}

impl AccountStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        if !cfg!(any(windows, target_os = "linux")) {
            return Err(CloudError::UnsupportedPlatform);
        }
        fs::create_dir_all(root.join(PROFILE_DIRECTORY)).map_err(|_| CloudError::Storage)?;
        Ok(Self { root })
    }

    /// Locks are acquired outside the async executor to avoid starving callbacks.
    pub async fn lock_profile(&self, profile_id: &str) -> Result<StorageLock> {
        validate_profile_id(profile_id)?;
        acquire_lock(
            self.root
                .join(PROFILE_DIRECTORY)
                .join(format!("{profile_id}.lock")),
        )
        .await
    }

    pub async fn lock_host(&self) -> Result<StorageLock> {
        acquire_lock(self.root.join("host.lock")).await
    }

    pub fn host_id(&self) -> Result<String> {
        Ok(self.load_host()?.ext_agent_host_id)
    }

    pub fn active_profile(&self) -> Result<Option<String>> {
        Ok(self.load_host()?.active_profile)
    }

    pub fn select(&self, profile_id: Option<String>) -> Result<()> {
        if let Some(id) = &profile_id {
            validate_profile_id(id)?;
        }
        let mut host = self.load_host()?;
        host.active_profile = profile_id;
        atomic_write(
            &self.root.join(STATE_FILE),
            &serde_json::to_vec(&host).map_err(|_| CloudError::Storage)?,
        )
    }

    pub fn load(&self, profile_id: &str) -> Result<ProfileRecord> {
        let path = self.profile_path(profile_id)?;
        let ciphertext = read_bounded(&path).map_err(|error| {
            if matches!(error, CloudError::UnknownProfile) {
                CloudError::UnknownProfile
            } else {
                CloudError::Storage
            }
        })?;
        if !ciphertext.starts_with(FILE_PREFIX) {
            return Err(CloudError::Storage);
        }
        let plaintext = Zeroizing::new(unprotect(&ciphertext[FILE_PREFIX.len()..])?);
        let record: ProfileRecord =
            serde_json::from_slice(&plaintext).map_err(|_| CloudError::Storage)?;
        if record.id != profile_id
            || record.client_id == "dynamic_agent_client"
            || record.client_id.is_empty()
        {
            return Err(CloudError::Storage);
        }
        Ok(record)
    }

    pub fn save(&self, record: &ProfileRecord) -> Result<()> {
        let plaintext =
            Zeroizing::new(serde_json::to_vec(record).map_err(|_| CloudError::Storage)?);
        if plaintext.len() as u64 > MAX_STORED_BYTES {
            return Err(CloudError::SizeLimit);
        }
        let mut ciphertext = FILE_PREFIX.to_vec();
        ciphertext.extend(protect(&plaintext)?);
        atomic_write(&self.profile_path(&record.id)?, &ciphertext)
    }

    /// Delete one registration; the caller holds its profile lock.
    pub fn remove(&self, profile_id: &str) -> Result<()> {
        match fs::remove_file(self.profile_path(profile_id)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(CloudError::UnknownProfile)
            }
            Err(_) => Err(CloudError::Storage),
        }
    }

    /// Best-effort cleanup of a removed registration's lock file.
    pub fn remove_lock_file(&self, profile_id: &str) {
        if validate_profile_id(profile_id).is_ok() {
            let _ = fs::remove_file(
                self.root
                    .join(PROFILE_DIRECTORY)
                    .join(format!("{profile_id}.lock")),
            );
        }
    }

    pub fn profile_ids(&self) -> Result<Vec<String>> {
        let directory =
            fs::read_dir(self.root.join(PROFILE_DIRECTORY)).map_err(|_| CloudError::Storage)?;
        let mut ids = Vec::new();
        for entry in directory {
            let path = entry.map_err(|_| CloudError::Storage)?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("bin") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or(CloudError::Storage)?;
            validate_profile_id(id)?;
            ids.push(id.to_owned());
        }
        ids.sort_unstable();
        Ok(ids)
    }

    fn profile_path(&self, id: &str) -> Result<PathBuf> {
        validate_profile_id(id)?;
        Ok(self.root.join(PROFILE_DIRECTORY).join(format!("{id}.bin")))
    }

    fn load_host(&self) -> Result<HostState> {
        let path = self.root.join(STATE_FILE);
        if !path.exists() {
            let state = HostState {
                version: 1,
                ext_agent_host_id: format!("urn:uuid:{}", Uuid::new_v4()),
                active_profile: None,
            };
            atomic_write(
                &path,
                &serde_json::to_vec(&state).map_err(|_| CloudError::Storage)?,
            )?;
            return Ok(state);
        }
        let state: HostState =
            serde_json::from_slice(&read_bounded(&path)?).map_err(|_| CloudError::Storage)?;
        let host_uuid = state
            .ext_agent_host_id
            .strip_prefix("urn:uuid:")
            .ok_or(CloudError::Storage)?;
        Uuid::parse_str(host_uuid).map_err(|_| CloudError::Storage)?;
        if state.version != 1 {
            return Err(CloudError::Storage);
        }
        if let Some(id) = &state.active_profile {
            validate_profile_id(id)?;
        }
        Ok(state)
    }
}

async fn acquire_lock(path: PathBuf) -> Result<StorageLock> {
    const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    const LOCK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(25);
    tokio::task::spawn_blocking(move || {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| CloudError::Storage)?;
        let deadline = std::time::Instant::now() + LOCK_TIMEOUT;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(33) =>
                {
                    if std::time::Instant::now() >= deadline {
                        return Err(CloudError::Storage);
                    }
                    std::thread::sleep(LOCK_RETRY_DELAY);
                }
                Err(_) => return Err(CloudError::Storage),
            }
        }
        Ok(StorageLock { _file: file })
    })
    .await
    .map_err(|_| CloudError::Storage)?
}

fn validate_profile_id(id: &str) -> Result<()> {
    if Uuid::parse_str(id).is_err() || id.len() != 36 {
        return Err(CloudError::UnknownProfile);
    }
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CloudError::UnknownProfile
        } else {
            CloudError::Storage
        }
    })?;
    if file.metadata().map_err(|_| CloudError::Storage)?.len() > MAX_STORED_BYTES {
        return Err(CloudError::SizeLimit);
    }
    let mut bytes = Vec::new();
    file.take(MAX_STORED_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CloudError::Storage)?;
    if bytes.len() as u64 > MAX_STORED_BYTES {
        return Err(CloudError::SizeLimit);
    }
    Ok(bytes)
}

/// Writes a complete durable record before an atomic, write-through replacement.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or(CloudError::Storage)?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|_| CloudError::Storage)?;
        file.write_all(bytes).map_err(|_| CloudError::Storage)?;
        file.sync_all().map_err(|_| CloudError::Storage)?;
        drop(file);
        replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both buffers are NUL-terminated and remain alive through the native call.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(CloudError::Storage);
    }
    Ok(())
}

/// `rename` replaces atomically; syncing the directory makes the new entry durable.
#[cfg(unix)]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).map_err(|_| CloudError::Storage)?;
    if let Some(parent) = destination.parent() {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| CloudError::Storage)?;
    }
    Ok(())
}

#[cfg(not(any(windows, unix)))]
fn replace_file(_source: &Path, _destination: &Path) -> Result<()> {
    Err(CloudError::UnsupportedPlatform)
}

#[cfg(windows)]
fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    crypt_bytes(bytes, false, PROTECTION_ENTROPY)
}
#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    crypt_bytes(bytes, true, PROTECTION_ENTROPY)
}
#[cfg(target_os = "linux")]
fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    onda_platform::protect_for(PROTECTION_ENTROPY, bytes).map_err(|_| CloudError::Storage)
}
#[cfg(target_os = "linux")]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    onda_platform::unprotect_for(PROTECTION_ENTROPY, bytes).map_err(|_| CloudError::Storage)
}
#[cfg(not(any(windows, target_os = "linux")))]
fn protect(_bytes: &[u8]) -> Result<Vec<u8>> {
    Err(CloudError::UnsupportedPlatform)
}
#[cfg(not(any(windows, target_os = "linux")))]
fn unprotect(_bytes: &[u8]) -> Result<Vec<u8>> {
    Err(CloudError::UnsupportedPlatform)
}

/// DPAPI for the current Windows user. `entropy` separates each kind of record, so one cannot be
/// swapped for another.
#[cfg(windows)]
pub(crate) fn crypt_bytes(bytes: &[u8], decrypt: bool, entropy: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    if bytes.len() > u32::MAX as usize {
        return Err(CloudError::SizeLimit);
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI is bound to the current Windows user; machine-wide protection is intentionally absent.
    let success = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err(CloudError::Storage);
    }
    // DPAPI owns this allocation; copy then zero and release it with LocalFree.
    let result = unsafe {
        let output_bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copied = output_bytes.to_vec();
        output_bytes.zeroize();
        LocalFree(output.pbData.cast());
        copied
    };
    Ok(result)
}

pub(crate) fn expiry_after(seconds: u64) -> Result<u64> {
    unix_time()?
        .checked_add(seconds)
        .ok_or(CloudError::MalformedResponse)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dpapi_round_trip_keeps_tokens_off_disk_and_preserves_registration() {
        let directory = tempfile::tempdir().unwrap();
        let store = AccountStore::new(directory.path().to_owned()).unwrap();
        let _host_lock = store.lock_host().await.unwrap();
        let host = store.host_id().unwrap();
        assert_eq!(host, store.host_id().unwrap());
        let mut record = ProfileRecord {
            id: Uuid::new_v4().to_string(),
            client_id: "oaiapp_test".into(),
            issuer: "https://auth.openai.com".into(),
            subject: Some("subject".into()),
            email: None,
            name: None,
            preferred_model: None,
            refresh_pending: false,
            tokens: Some(SecretTokens {
                access_token: "secret-access".into(),
                refresh_token: "secret-refresh".into(),
                id_token: "secret-identity".into(),
                scopes: vec!["chatgpt.tokens.use.direct".into()],
                expires_at: 123,
                earliest_refresh_at: None,
            }),
        };
        store.save(&record).unwrap();
        let disk = fs::read(store.profile_path(&record.id).unwrap()).unwrap();
        assert!(!disk
            .windows("secret-access".len())
            .any(|window| window == b"secret-access"));
        assert_eq!(
            store
                .load(&record.id)
                .unwrap()
                .tokens
                .as_ref()
                .unwrap()
                .refresh_token,
            "secret-refresh"
        );
        record.tokens = None;
        store.save(&record).unwrap();
        assert_eq!(store.load(&record.id).unwrap().client_id, "oaiapp_test");
        assert!(!store.load(&record.id).unwrap().public(false).signed_in);
    }
}
