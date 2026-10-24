//! Per-user at-rest protection (DPAPI) for content Onda keeps on disk, such as history.
use crate::PlatformError;

/// Separates history blobs from other DPAPI consumers running as the same Windows user.
const ENTROPY: &[u8] = b"onda-history-v1";

pub fn protect_user_data(bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
    crypt(bytes, false)
}

pub fn unprotect_user_data(bytes: &[u8]) -> Result<Vec<u8>, PlatformError> {
    crypt(bytes, true)
}

#[cfg(not(windows))]
fn crypt(_bytes: &[u8], _decrypt: bool) -> Result<Vec<u8>, PlatformError> {
    Err(PlatformError::UnsupportedPlatform)
}

#[cfg(windows)]
fn crypt(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>, PlatformError> {
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
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
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
    }
}
