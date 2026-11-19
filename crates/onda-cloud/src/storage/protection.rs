use super::*;

#[cfg(windows)]
pub(super) fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    crypt_bytes(bytes, false, PROTECTION_ENTROPY)
}

#[cfg(windows)]
pub(super) fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    crypt_bytes(bytes, true, PROTECTION_ENTROPY)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    onda_platform::protect_for(PROTECTION_ENTROPY, bytes).map_err(|_| CloudError::Storage)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    onda_platform::unprotect_for(PROTECTION_ENTROPY, bytes).map_err(|_| CloudError::Storage)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(super) fn protect(_bytes: &[u8]) -> Result<Vec<u8>> {
    Err(CloudError::UnsupportedPlatform)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(super) fn unprotect(_bytes: &[u8]) -> Result<Vec<u8>> {
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
