//! Locate the one system Evergreen install used by the default WebView2 loader.
//! A policy, per-user install or unfamiliar updater layout makes the hint inapplicable.
use std::{
    ffi::OsString,
    mem::size_of_val,
    os::windows::ffi::OsStringExt,
    path::{Component, Path, PathBuf, Prefix},
    ptr,
};
use windows_sys::{
    core::PCWSTR,
    w,
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
            KEY_QUERY_VALUE, KEY_WOW64_32KEY, KEY_WOW64_64KEY, RRF_RT_ANY, RRF_RT_REG_SZ,
            RRF_SUBKEY_WOW6432KEY, RRF_SUBKEY_WOW6464KEY,
        },
    },
};

const EVERGREEN_CLIENT: PCWSTR =
    w!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}");
const EVERGREEN_STATE: PCWSTR =
    w!(r"SOFTWARE\Microsoft\EdgeUpdate\ClientState\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}");
const POLICY_KEYS: [PCWSTR; 2] = [
    w!(r"SOFTWARE\Policies\Microsoft\Edge\WebView2"),
    w!(r"SOFTWARE\Policies\Microsoft\EmbeddedBrowserWebView\LoaderOverride"),
];
const WEBVIEW_OVERRIDES: [&str; 6] = [
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "WEBVIEW2_CHANNEL_SEARCH_KIND",
    "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
    "WEBVIEW2_RELEASE_CHANNELS",
    "WEBVIEW2_USER_DATA_FOLDER",
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
];

pub(super) fn machine_loader() -> Option<PathBuf> {
    if WEBVIEW_OVERRIDES
        .iter()
        .any(|key| std::env::var_os(key).is_some())
        || !uses_default_registration(HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER)
    {
        return None;
    }
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    let program_files = machine_string(
        w!(r"SOFTWARE\Microsoft\Windows\CurrentVersion"),
        w!("ProgramFilesDir"),
    )?;
    let version = machine_string(EVERGREEN_CLIENT, w!("pv"))?;
    let active = machine_string(EVERGREEN_STATE, w!("EBWebView"))?;
    registered_loader(
        Path::new(&program_files),
        version.to_str()?,
        Path::new(&active),
        architecture,
    )
}

/// Looking only for absence also skips an unreadable policy. Reading configured override
/// values or guessing precedence could cause disk I/O for a runtime the application won't use.
fn uses_default_registration(machine: HKEY, user: HKEY) -> bool {
    for (view, value_view) in [
        (KEY_WOW64_32KEY, RRF_SUBKEY_WOW6432KEY),
        (KEY_WOW64_64KEY, RRF_SUBKEY_WOW6464KEY),
    ] {
        if !key_missing(user, EVERGREEN_CLIENT, view) {
            return false;
        }
        // Ordinary usage creates a per-user ClientState key even for a system install.
        let mut size = 0;
        let status = unsafe {
            RegGetValueW(
                user,
                EVERGREEN_STATE,
                w!("EBWebView"),
                RRF_RT_ANY | value_view,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size,
            )
        };
        if !matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return false;
        }
        if [machine, user].into_iter().any(|root| {
            POLICY_KEYS
                .into_iter()
                .any(|key| !key_missing(root, key, view))
        }) {
            return false;
        }
    }
    true
}

fn key_missing(root: HKEY, key: PCWSTR, view: u32) -> bool {
    let mut handle = ptr::null_mut();
    let status = unsafe { RegOpenKeyExW(root, key, 0, KEY_QUERY_VALUE | view, &mut handle) };
    if status == ERROR_SUCCESS {
        unsafe { RegCloseKey(handle) };
    }
    matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
}

/// EdgeUpdate registers Evergreen in the 32-bit view. A longer or non-string value is an
/// unfamiliar layout, so a fixed-size buffer suffices.
fn machine_string(key: PCWSTR, name: PCWSTR) -> Option<OsString> {
    const MAX_REGISTRY_CHARS: usize = 512;
    let mut buffer = [0u16; MAX_REGISTRY_CHARS];
    let mut bytes = size_of_val(&buffer) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key,
            name,
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6432KEY,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != ERROR_SUCCESS || !bytes.is_multiple_of(2) {
        return None;
    }
    let value = buffer.get(..bytes as usize / 2)?.strip_suffix(&[0])?;
    (!value.contains(&0)).then(|| OsString::from_wide(value))
}

/// The active updater location must agree with the installed version in the standard
/// system directory. This identifies one file even while EdgeUpdate keeps older releases.
fn registered_loader(
    program_files: &Path,
    version: &str,
    runtime: &Path,
    architecture: &str,
) -> Option<PathBuf> {
    let mut components = program_files.components();
    if !matches!(components.next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
        || components.next() != Some(Component::RootDir)
        || components.any(|component| !matches!(component, Component::Normal(_)))
        || !valid_version(version)
    {
        return None;
    }
    let expected = program_files
        .join(r"Microsoft\EdgeWebView\Application")
        .join(version);
    expected
        .as_os_str()
        .eq_ignore_ascii_case(runtime.as_os_str())
        .then(|| {
            expected
                .join("EBWebView")
                .join(architecture)
                .join("EmbeddedBrowserWebView.dll")
        })
}

/// Evergreen publishes four decimal 16-bit components.
fn valid_version(version: &str) -> bool {
    let mut components = version.split('.');
    for index in 0..4 {
        let Some(part) = components.next() else {
            return false;
        };
        if !part.bytes().all(|byte| byte.is_ascii_digit())
            || part.len() > 5
            || (part.len() > 1 && part.starts_with('0'))
        {
            return false;
        }
        let Ok(value) = part.parse::<u16>() else {
            return false;
        };
        if index == 0 && value == 0 {
            return false;
        }
    }
    components.next().is_none()
}

#[cfg(test)]
mod tests;
