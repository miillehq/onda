//! Per-user at-rest protection for content Onda keeps on disk, such as history: DPAPI on
//! Windows, and on Linux a key held in the desktop keyring (Secret Service).
use crate::PlatformError;

/// Separates history blobs from other records protected for the same user.
const ENTROPY: &[u8] = b"onda-history-v1";

pub fn protect_user_data(bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
    protect_for(ENTROPY, bytes)
}

pub fn unprotect_user_data(bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
    unprotect_for(ENTROPY, bytes)
}

/// `purpose` separates each kind of record, so one cannot be opened as another.
pub fn protect_for(purpose: &[u8], bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
    crypt(bytes, false, purpose)
}

pub fn unprotect_for(purpose: &[u8], bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
    crypt(bytes, true, purpose)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn crypt(_bytes: &[u8], _decrypt: bool, _purpose: &[u8]) -> Result<Vec<u8>, PlatformError> {
    Err(PlatformError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn crypt(bytes: &[u8], decrypt: bool, purpose: &[u8]) -> Result<Vec<u8>, PlatformError> {
    if decrypt {
        keyring_seal::open(purpose, bytes)
    } else {
        keyring_seal::seal(purpose, bytes)
    }
}

/// ChaCha20-Poly1305 under one random key that the desktop keyring keeps for this user, the
/// counterpart of DPAPI's per-user key. Reading it can prompt to unlock the keyring.
#[cfg(target_os = "linux")]
mod keyring_seal {
    use crate::PlatformError;
    use ring::{
        aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305, NONCE_LEN},
        rand::{SecureRandom, SystemRandom},
    };
    use std::sync::Mutex;
    use zeroize::Zeroizing;

    /// The application identifier, as the provider keys use.
    const SERVICE: &str = "com.miille.onda";
    const ACCOUNT: &str = "Data protection key";
    const FORMAT: u8 = 1;
    const KEY_BYTES: usize = 32;

    static KEY: Mutex<Option<Zeroizing<[u8; KEY_BYTES]>>> = Mutex::new(None);

    fn key() -> Result<LessSafeKey, PlatformError> {
        let mut cached = KEY.lock().map_err(|_| PlatformError::Protection)?;
        if cached.is_none() {
            *cached = Some(load_or_create()?);
        }
        let bytes = cached.as_ref().ok_or(PlatformError::Protection)?;
        let key = UnboundKey::new(&CHACHA20_POLY1305, bytes.as_slice())
            .map_err(|_| PlatformError::Protection)?;
        Ok(LessSafeKey::new(key))
    }

    fn load_or_create() -> Result<Zeroizing<[u8; KEY_BYTES]>, PlatformError> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| PlatformError::Protection)?;
        let mut key = Zeroizing::new([0; KEY_BYTES]);
        match entry.get_secret() {
            Ok(stored) => {
                let stored = Zeroizing::new(stored);
                // Never replace a key of the wrong size: records sealed under it would be lost.
                if stored.len() != KEY_BYTES {
                    return Err(PlatformError::Protection);
                }
                key.copy_from_slice(&stored);
            }
            Err(keyring::Error::NoEntry) => {
                SystemRandom::new()
                    .fill(key.as_mut_slice())
                    .map_err(|_| PlatformError::Protection)?;
                entry
                    .set_secret(key.as_slice())
                    .map_err(|_| PlatformError::Protection)?;
            }
            Err(_) => return Err(PlatformError::Protection),
        }
        Ok(key)
    }

    /// Format byte, random nonce, then ciphertext and tag; `purpose` is authenticated data.
    pub(super) fn seal(purpose: &[u8], bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let key = key()?;
        let mut nonce = [0; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| PlatformError::Protection)?;
        let mut body = bytes.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(purpose),
            &mut body,
        )
        .map_err(|_| PlatformError::Protection)?;
        let mut sealed = Vec::with_capacity(1 + NONCE_LEN + body.len());
        sealed.push(FORMAT);
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&body);
        Ok(sealed)
    }

    pub(super) fn open(purpose: &[u8], sealed: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let (format, rest) = sealed.split_first().ok_or(PlatformError::Protection)?;
        if *format != FORMAT || rest.len() < NONCE_LEN {
            return Err(PlatformError::Protection);
        }
        let (nonce, body) = rest.split_at(NONCE_LEN);
        let nonce =
            Nonce::try_assume_unique_for_key(nonce).map_err(|_| PlatformError::Protection)?;
        let mut body = Zeroizing::new(body.to_vec());
        let plaintext = key()?
            .open_in_place(nonce, Aad::from(purpose), &mut body)
            .map_err(|_| PlatformError::Protection)?;
        Ok(plaintext.to_vec())
    }
}

#[cfg(windows)]
fn crypt(bytes: &[u8], decrypt: bool, purpose: &[u8]) -> Result<Vec<u8>, PlatformError> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    use zeroize::Zeroize;
    let length = u32::try_from(bytes.len()).map_err(|_| PlatformError::TextLimit)?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: length,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(purpose.len()).map_err(|_| PlatformError::Protection)?,
        pbData: purpose.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // Bound to the current Windows user; machine-wide protection is intentionally absent.
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
    if success == 0 || output.pbData.is_null() {
        return Err(PlatformError::Protection);
    }
    // DPAPI owns this allocation; copy, zero and release it.
    let result = unsafe {
        let owned = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copied = owned.to_vec();
        owned.zeroize();
        LocalFree(output.pbData.cast());
        copied
    };
    Ok(result)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn protected_bytes_round_trip_and_hide_plaintext() {
        let plaintext = b"meet me at 4pm on Thursday";
        let sealed = protect_user_data(plaintext).unwrap();
        assert!(!sealed
            .windows(plaintext.len())
            .any(|window| window == plaintext));
        assert_eq!(unprotect_user_data(&sealed).unwrap(), plaintext);
        assert!(unprotect_user_data(b"not a dpapi blob").is_err());
        let other = protect_for(b"another purpose", plaintext).unwrap();
        assert!(unprotect_user_data(&other).is_err());
    }
}

/// Needs a desktop Secret Service; run inside `dbus-run-session` with an unlocked gnome-keyring
/// (see docs/linux.md). Ignored by default so CI never touches a real keyring.
#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;

    #[test]
    #[ignore]
    fn keyring_sealed_bytes_round_trip_and_stay_apart_by_purpose() {
        let plaintext = b"meet me at 4pm on Thursday";
        let sealed = protect_user_data(plaintext).unwrap();
        assert!(!sealed
            .windows(plaintext.len())
            .any(|window| window == plaintext));
        assert_eq!(unprotect_user_data(&sealed).unwrap(), plaintext);
        assert_ne!(
            protect_user_data(plaintext).unwrap(),
            sealed,
            "nonces differ"
        );
        let other = protect_for(b"another purpose", plaintext).unwrap();
        assert!(unprotect_user_data(&other).is_err());
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(unprotect_user_data(&tampered).is_err());
        assert!(unprotect_user_data(b"").is_err());
    }
}
