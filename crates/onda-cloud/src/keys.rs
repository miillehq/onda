//! API keys for user-configured AI providers. Keys live only in the operating system's protected
//! storage: DPAPI files bound to the Windows user, or the desktop Secret Service on Linux. They
//! never enter settings, logs, prompts or the interface; the interface only learns whether one is
//! saved.

use crate::{CloudError, Result};
use std::path::PathBuf;
use zeroize::Zeroizing;

const MAX_KEY_BYTES: usize = 512;

/// One saved key per provider kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySlot {
    OpenaiCompatible,
    Anthropic,
    Local,
}

impl KeySlot {
    fn name(self) -> &'static str {
        match self {
            Self::OpenaiCompatible => "openai-compatible",
            Self::Anthropic => "anthropic",
            Self::Local => "local",
        }
    }
}

/// A key that is zeroed on drop and never printed.
pub struct ApiKey(Zeroizing<String>);

impl ApiKey {
    /// Accepts printable ASCII without spaces, as every provider issues.
    pub fn parse(raw: &str) -> Result<Self> {
        let key = raw.trim();
        if key.is_empty()
            || key.len() > MAX_KEY_BYTES
            || !key.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(CloudError::InvalidApiKey);
        }
        Ok(Self(Zeroizing::new(key.to_owned())))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ApiKey(…)")
    }
}

#[derive(Clone)]
pub struct KeyStore {
    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    root: PathBuf,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    service: String,
}

impl KeyStore {
    /// `service` names the Secret Service entries on Linux, e.g. the application identifier.
    pub fn new(data_directory: PathBuf, service: impl Into<String>) -> Self {
        Self {
            root: data_directory.join("provider-keys"),
            service: service.into(),
        }
    }

    pub fn save(&self, slot: KeySlot, key: &ApiKey) -> Result<()> {
        platform::save(self, slot, key.expose())
    }

    pub fn load(&self, slot: KeySlot) -> Result<Option<ApiKey>> {
        platform::load(self, slot)
    }

    pub fn remove(&self, slot: KeySlot) -> Result<()> {
        platform::remove(self, slot)
    }

    /// Cheap and prompt-free: never decrypts or unlocks anything, so the interface can ask
    /// often. A key removed outside Onda is only noticed when it is next used.
    pub fn has(&self, slot: KeySlot) -> bool {
        platform::has(self, slot)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::storage::{atomic_write, crypt_bytes};
    const FILE_PREFIX: &[u8] = b"ONDA-KEY-DPAPI-1\0";

    fn entropy(slot: KeySlot) -> Vec<u8> {
        format!("onda/provider-key/v1/{}", slot.name()).into_bytes()
    }

    fn path(store: &KeyStore, slot: KeySlot) -> PathBuf {
        store.root.join(format!("{}.bin", slot.name()))
    }

    pub fn save(store: &KeyStore, slot: KeySlot, key: &str) -> Result<()> {
        std::fs::create_dir_all(&store.root).map_err(|_| CloudError::Storage)?;
        let mut record = FILE_PREFIX.to_vec();
        record.extend(crypt_bytes(key.as_bytes(), false, &entropy(slot))?);
        atomic_write(&path(store, slot), &record)
    }

    pub fn load(store: &KeyStore, slot: KeySlot) -> Result<Option<ApiKey>> {
        let bytes = match std::fs::read(path(store, slot)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(CloudError::Storage),
        };
        let protected = bytes.strip_prefix(FILE_PREFIX).ok_or(CloudError::Storage)?;
        let plain = Zeroizing::new(crypt_bytes(protected, true, &entropy(slot))?);
        let key = std::str::from_utf8(&plain).map_err(|_| CloudError::Storage)?;
        ApiKey::parse(key)
            .map(Some)
            .map_err(|_| CloudError::Storage)
    }

    pub fn remove(store: &KeyStore, slot: KeySlot) -> Result<()> {
        match std::fs::remove_file(path(store, slot)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(CloudError::Storage),
        }
    }

    pub fn has(store: &KeyStore, slot: KeySlot) -> bool {
        path(store, slot).is_file()
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    fn entry(store: &KeyStore, slot: KeySlot) -> Result<keyring::Entry> {
        keyring::Entry::new(&store.service, &format!("{} API key", slot.name()))
            .map_err(|_| CloudError::Storage)
    }

    /// An empty file that only records that a key was saved: reading the Secret Service can
    /// prompt to unlock the keyring, which should happen only when the key is used.
    fn marker(store: &KeyStore, slot: KeySlot) -> PathBuf {
        store.root.join(format!("{}.saved", slot.name()))
    }

    pub fn save(store: &KeyStore, slot: KeySlot, key: &str) -> Result<()> {
        entry(store, slot)?
            .set_password(key)
            .map_err(|_| CloudError::Storage)?;
        std::fs::create_dir_all(&store.root).map_err(|_| CloudError::Storage)?;
        std::fs::write(marker(store, slot), b"").map_err(|_| CloudError::Storage)
    }

    pub fn has(store: &KeyStore, slot: KeySlot) -> bool {
        marker(store, slot).is_file()
    }

    pub fn load(store: &KeyStore, slot: KeySlot) -> Result<Option<ApiKey>> {
        match entry(store, slot)?.get_password() {
            Ok(key) => {
                let key = Zeroizing::new(key);
                ApiKey::parse(&key)
                    .map(Some)
                    .map_err(|_| CloudError::Storage)
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(CloudError::Storage),
        }
    }

    pub fn remove(store: &KeyStore, slot: KeySlot) -> Result<()> {
        match entry(store, slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err(CloudError::Storage),
        }
        match std::fs::remove_file(marker(store, slot)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(CloudError::Storage),
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub fn save(_: &KeyStore, _: KeySlot, _: &str) -> Result<()> {
        Err(CloudError::UnsupportedPlatform)
    }
    pub fn load(_: &KeyStore, _: KeySlot) -> Result<Option<ApiKey>> {
        Err(CloudError::UnsupportedPlatform)
    }
    pub fn remove(_: &KeyStore, _: KeySlot) -> Result<()> {
        Err(CloudError::UnsupportedPlatform)
    }
    pub fn has(_: &KeyStore, _: KeySlot) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_trimmed_printable_and_never_debug_printed() {
        let key = ApiKey::parse("  sk-test-123  ").unwrap();
        assert_eq!(key.expose(), "sk-test-123");
        assert_eq!(format!("{key:?}"), "ApiKey(…)");
        for invalid in ["", "   ", "sk test", "sk-\u{e9}", "sk-\n1"] {
            assert!(ApiKey::parse(invalid).is_err(), "{invalid:?}");
        }
        assert!(ApiKey::parse(&"k".repeat(MAX_KEY_BYTES + 1)).is_err());
    }

    /// Needs a desktop Secret Service; run inside `dbus-run-session` with an unlocked
    /// gnome-keyring (see docs/linux.md). Ignored by default so CI never touches a real keyring.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn secret_service_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let store = KeyStore::new(directory.path().into(), "com.miille.onda.test");
        store.remove(KeySlot::Anthropic).unwrap();
        assert!(store.load(KeySlot::Anthropic).unwrap().is_none());
        store
            .save(KeySlot::Anthropic, &ApiKey::parse("sk-ant-test").unwrap())
            .unwrap();
        assert_eq!(
            store.load(KeySlot::Anthropic).unwrap().unwrap().expose(),
            "sk-ant-test"
        );
        assert!(store.load(KeySlot::OpenaiCompatible).unwrap().is_none());
        assert!(store.has(KeySlot::Anthropic));
        // Only an empty marker is written to disk; the key itself is in the keyring.
        let marker = directory.path().join("provider-keys/anthropic.saved");
        assert_eq!(std::fs::read(&marker).unwrap(), b"");
        store.remove(KeySlot::Anthropic).unwrap();
        assert!(!store.has(KeySlot::Anthropic));
        assert!(!marker.exists());
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_round_trip_keeps_slots_apart() {
        let directory = tempfile::tempdir().unwrap();
        let store = KeyStore::new(directory.path().into(), "unused");
        store
            .save(KeySlot::OpenaiCompatible, &ApiKey::parse("sk-one").unwrap())
            .unwrap();
        let file = directory.path().join("provider-keys/openai-compatible.bin");
        let bytes = std::fs::read(&file).unwrap();
        assert!(!bytes.windows(6).any(|window| window == b"sk-one"));
        // A file copied into another slot does not decrypt there.
        std::fs::copy(&file, directory.path().join("provider-keys/anthropic.bin")).unwrap();
        assert!(store.load(KeySlot::Anthropic).is_err());
        assert_eq!(
            store
                .load(KeySlot::OpenaiCompatible)
                .unwrap()
                .unwrap()
                .expose(),
            "sk-one"
        );
        store.remove(KeySlot::OpenaiCompatible).unwrap();
        assert!(store.load(KeySlot::OpenaiCompatible).unwrap().is_none());
    }
}
