//! Pressing paste in other apps on a Mac, which macOS allows only to apps the user has turned on
//! under System Settings › Privacy & Security › Accessibility. macOS asks once with its own
//! dialog; after that only System Settings changes it, and Onda cannot turn itself off there.
use crate::PlatformError;
use core_foundation::{
    base::TCFType,
    boolean::CFBoolean,
    dictionary::CFDictionary,
    string::{CFString, CFStringRef},
};
use core_foundation_sys::dictionary::CFDictionaryRef;
use serde::Serialize;

/// The Accessibility list in System Settings (System Preferences before macOS 13).
const ACCESSIBILITY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TypingAccess {
    NotAllowed,
    Allowed,
}

/// Whether Onda is on in the Accessibility list right now; the user can change it at any time.
pub(crate) fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

pub fn typing_access() -> TypingAccess {
    if trusted() {
        TypingAccess::Allowed
    } else {
        TypingAccess::NotAllowed
    }
}

/// Adds Onda to the Accessibility list with macOS's dialog and opens the list, where the user
/// turns Onda on. `Cancelled` means it is not on yet: the switch is in System Settings.
pub fn allow_typing() -> Result<(), PlatformError> {
    if trusted() {
        return Ok(());
    }
    let prompt = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(prompt, CFBoolean::true_value())]);
    if unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) } != 0 {
        return Ok(());
    }
    open_accessibility_settings();
    Err(PlatformError::Cancelled)
}

/// Opens the Accessibility list, the only place the user can turn typing off.
pub fn stop_typing() {
    open_accessibility_settings();
}

fn open_accessibility_settings() {
    // `open` hands the address to Launch Services and returns at once.
    if let Err(error) = std::process::Command::new("/usr/bin/open")
        .arg(ACCESSIBILITY_SETTINGS)
        .status()
    {
        tracing::warn!(subsystem = "typing", error = %error, "System Settings did not open");
    }
}
