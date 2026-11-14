//! The focused field on a Mac, read through the Accessibility API: which app, window and field it
//! is, its text, caret and selection, and pasting into it with ⌘V. All of it needs the
//! Accessibility permission (`macos_typing`); without it the snapshot is unidentified and
//! dictation stays on the clipboard. An app that shows no field (Electron and Chromium windows
//! often don't) is still named with its window, and a paste checks that both stayed in front.
use crate::{
    macos_typing,
    target::{ControlFingerprint, InsertionOutcome, PasteShortcut},
    PlatformError, TargetSnapshot,
};
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_foundation_sys::base::{CFHash, CFRange, CFTypeID};
use std::{
    ffi::{c_void, OsStr},
    os::unix::ffi::OsStrExt,
    path::Path,
    time::{Duration, Instant},
};

type AXUIElementRef = CFTypeRef;
type AXError = i32;
const AX_SUCCESS: AXError = 0;
const AX_VALUE_CGPOINT: u32 = 1;
const AX_VALUE_CGSIZE: u32 = 2;
const AX_VALUE_CFRANGE: u32 = 4;

/// `kCGEventSourceStateHIDSystemState`: the keyboard as the hardware reports it.
const HID_SYSTEM_STATE: i32 = 1;
/// `kCGHIDEventTap`: events enter where the keyboard's do, so the frontmost app receives them.
const HID_EVENT_TAP: u32 = 0;
/// `kVK_ANSI_V`, the V key's position on ANSI, ISO and JIS keyboards.
const KEY_V: u16 = 0x09;
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_OPTION: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const MODIFIER_FLAGS: u64 = FLAG_SHIFT | FLAG_CONTROL | FLAG_OPTION | FLAG_COMMAND;

/// A hung app must not hold up dictation; macOS would wait six seconds per call.
const MESSAGING_TIMEOUT_SECONDS: f32 = 1.5;
/// How long a dictation shortcut's keys may stay down before the paste gives up.
const MODIFIERS_WAIT: Duration = Duration::from_millis(1000);
/// How long a pasted text may take to show in the field before Onda stops waiting.
const PASTE_SETTLE: Duration = Duration::from_millis(1000);
/// How long the paste keeps the pasteboard before the earlier text comes back.
const CLIPBOARD_HOLD: Duration = Duration::from_millis(500);

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Size {
    width: f64,
    height: f64,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        result: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> AXError;
    fn AXValueGetTypeID() -> CFTypeID;
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, out: *mut c_void) -> u8;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> CFTypeRef;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    /// On while any app protects typed text, as password fields and Terminal's Secure Keyboard
    /// Entry do.
    fn IsSecureEventInputEnabled() -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceCreate(state: i32) -> CFTypeRef;
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
    fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    fn CGEventPost(tap: u32, event: CFTypeRef);
    fn CGEventSourceFlagsState(state: i32) -> u64;
}

/// An accessibility object, released when dropped.
struct Element(CFType);

impl Element {
    fn system_wide() -> Option<Self> {
        let reference = unsafe { AXUIElementCreateSystemWide() };
        if reference.is_null() {
            return None;
        }
        let element = Self(unsafe { CFType::wrap_under_create_rule(reference) });
        // Applies to every call made through this object.
        unsafe { AXUIElementSetMessagingTimeout(element.raw(), MESSAGING_TIMEOUT_SECONDS) };
        Some(element)
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef()
    }

    fn copy(&self, attribute: &'static str) -> Option<CFType> {
        let name = CFString::from_static_string(attribute);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(self.raw(), name.as_concrete_TypeRef(), &mut value)
        };
        (error == AX_SUCCESS && !value.is_null())
            .then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn element(&self, attribute: &'static str) -> Option<Element> {
        let value = self.copy(attribute)?;
        (value.type_of() == unsafe { AXUIElementGetTypeID() }).then_some(Element(value))
    }

    fn string(&self, attribute: &'static str) -> Option<String> {
        self.copy(attribute)?
            .downcast::<CFString>()
            .map(|text| text.to_string())
    }

    fn integer(&self, attribute: &'static str) -> Option<i64> {
        self.copy(attribute)?.downcast::<CFNumber>()?.to_i64()
    }

    fn range(&self, attribute: &'static str) -> Option<CFRange> {
        ax_value(
            &self.copy(attribute)?,
            AX_VALUE_CFRANGE,
            CFRange {
                location: 0,
                length: 0,
            },
        )
    }

    fn pid(&self) -> Option<i32> {
        let mut pid = 0;
        (unsafe { AXUIElementGetPid(self.raw(), &mut pid) } == AX_SUCCESS && pid > 0).then_some(pid)
    }

    /// Equal for two references to the same object, as long as it exists.
    fn identity(&self) -> u64 {
        unsafe { CFHash(self.raw()) as u64 }
    }

    /// The text in `range`, in UTF-16 units, as the field reports it.
    fn string_for_range(&self, range: CFRange) -> Option<String> {
        let parameter =
            unsafe { AXValueCreate(AX_VALUE_CFRANGE, &range as *const CFRange as *const c_void) };
        if parameter.is_null() {
            return None;
        }
        let parameter = unsafe { CFType::wrap_under_create_rule(parameter) };
        let name = CFString::from_static_string("AXStringForRange");
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyParameterizedAttributeValue(
                self.raw(),
                name.as_concrete_TypeRef(),
                parameter.as_CFTypeRef(),
                &mut value,
            )
        };
        if error != AX_SUCCESS || value.is_null() {
            return None;
        }
        unsafe { CFType::wrap_under_create_rule(value) }
            .downcast::<CFString>()
            .map(|text| text.to_string())
    }
}

fn ax_value<T: Copy>(value: &CFType, kind: u32, mut out: T) -> Option<T> {
    if value.type_of() != unsafe { AXValueGetTypeID() } {
        return None;
    }
    (unsafe {
        AXValueGetValue(
            value.as_CFTypeRef(),
            kind,
            &mut out as *mut T as *mut c_void,
        )
    } != 0)
        .then_some(out)
}

/// Password fields are `AXSecureTextField`, as their role or, more often, their subrole.
fn secure_role(role: &str, subrole: &str) -> bool {
    role == "AXSecureTextField" || subrole == "AXSecureTextField"
}

/// A role as a number, so the fingerprint can compare it like a Windows control type.
fn role_code(role: &str) -> i32 {
    role.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    }) as i32
}

/// The process and the object's hash: never empty, so a field always has an identity.
fn runtime_id(process_id: i32, identity: u64) -> Vec<i32> {
    vec![process_id, (identity >> 32) as i32, identity as i32]
}

fn bounds(field: &Element) -> [i32; 4] {
    let position = field
        .copy("AXPosition")
        .and_then(|value| ax_value(&value, AX_VALUE_CGPOINT, Point::default()));
    let size = field
        .copy("AXSize")
        .and_then(|value| ax_value(&value, AX_VALUE_CGSIZE, Size::default()));
    match (position, size) {
        (Some(position), Some(size)) => [
            position.x as i32,
            position.y as i32,
            (position.x + size.width) as i32,
            (position.y + size.height) as i32,
        ],
        _ => [0; 4],
    }
}

/// When the process started, in microseconds, so a reused process ID is not the same app.
fn process_started(process_id: i32) -> u64 {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            process_id,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut libc::proc_bsdinfo as *mut c_void,
            size,
        )
    };
    if read != size {
        return 0;
    }
    info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec
}

fn process_name(process_id: i32) -> String {
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(
            process_id,
            path.as_mut_ptr() as *mut c_void,
            path.len() as u32,
        )
    };
    if length <= 0 {
        return String::new();
    }
    Path::new(OsStr::from_bytes(&path[..length as usize]))
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The frontmost app and its focused field, if it shows one.
struct Focus {
    snapshot: TargetSnapshot,
    field: Option<Element>,
}

fn focus() -> Option<Focus> {
    if !macos_typing::trusted() {
        return None;
    }
    let application = Element::system_wide()?.element("AXFocusedApplication")?;
    let process_id = application.pid()?;
    let window = application.element("AXFocusedWindow");
    let field = application.element("AXFocusedUIElement");
    let mut snapshot = TargetSnapshot {
        window_handle: window.map(|window| window.identity() as isize).unwrap_or(0),
        process_id: process_id as u32,
        process_created_at: process_started(process_id),
        executable_name: process_name(process_id),
        focused_runtime_id: Vec::new(),
        is_password: unsafe { IsSecureEventInputEnabled() } != 0,
        integrity_level: 0,
        focused_control: ControlFingerprint {
            process_id: process_id as u32,
            ..ControlFingerprint::default()
        },
    };
    if let Some(field) = &field {
        let role = field.string("AXRole").unwrap_or_default();
        let subrole = field.string("AXSubrole").unwrap_or_default();
        snapshot.focused_runtime_id = runtime_id(process_id, field.identity());
        snapshot.is_password |= secure_role(&role, &subrole);
        snapshot.focused_control = ControlFingerprint {
            process_id: process_id as u32,
            control_type: role_code(&role),
            automation_id: field.string("AXIdentifier").unwrap_or_default(),
            framework_id: subrole,
            bounds: bounds(field),
        };
    }
    Some(Focus { snapshot, field })
}

/// Without the permission the snapshot names nothing, and typing is off.
pub(crate) fn snapshot() -> Result<TargetSnapshot, PlatformError> {
    Ok(focus()
        .map(|focus| focus.snapshot)
        .unwrap_or_else(TargetSnapshot::unidentified))
}

pub(crate) fn can_type(_target: &TargetSnapshot) -> bool {
    macos_typing::trusted()
}

/// The focused field, when it is still the one `target` describes.
fn field(target: &TargetSnapshot) -> Result<Element, PlatformError> {
    if !macos_typing::trusted() {
        return Err(PlatformError::UnsupportedPlatform);
    }
    let focus = focus().ok_or(PlatformError::TargetChanged)?;
    crate::target::validate_identity(target, &focus.snapshot)?;
    focus.field.ok_or(PlatformError::TargetChanged)
}

fn check_limit(text: &str, max_characters: usize) -> Result<(), PlatformError> {
    if text.encode_utf16().count() > max_characters {
        return Err(PlatformError::TextLimit);
    }
    Ok(())
}

pub(crate) fn read_text(
    target: &TargetSnapshot,
    max_characters: usize,
    selected: bool,
) -> Result<String, PlatformError> {
    let field = field(target)?;
    let text = if selected {
        field
            .string("AXSelectedText")
            .ok_or(PlatformError::SelectionUnavailable)?
    } else {
        field
            .string("AXValue")
            .ok_or_else(|| PlatformError::TargetUnavailable("the field shows no text".into()))?
    };
    check_limit(&text, max_characters)?;
    if selected && text.is_empty() {
        return Err(PlatformError::EmptySelection);
    }
    Ok(text)
}

/// Up to `max_characters` just before the caret, or before the selection a paste replaces.
pub(crate) fn preceding_text(
    target: &TargetSnapshot,
    max_characters: usize,
) -> Result<String, PlatformError> {
    let field = field(target)?;
    let caret = field
        .range("AXSelectedTextRange")
        .ok_or_else(|| PlatformError::TargetUnavailable("the field shows no caret".into()))?
        .location;
    if caret <= 0 {
        return Ok(String::new());
    }
    let start = caret.saturating_sub(max_characters as isize).max(0);
    let text = field
        .string_for_range(CFRange {
            location: start,
            length: caret - start,
        })
        .ok_or_else(|| PlatformError::TargetUnavailable("the field shows no text".into()))?;
    let skip = text.chars().count().saturating_sub(max_characters);
    Ok(text.chars().skip(skip).collect())
}

/// Presses ⌘V into the frontmost app with the text on the pasteboard, then puts the earlier
/// pasteboard text back. A Mac pastes with ⌘V in terminals too, so an app profile's paste keys,
/// which are Windows and Linux keys, do not apply.
pub(crate) fn paste(
    target: &TargetSnapshot,
    text: &str,
    _shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    if !macos_typing::trusted() {
        return Err(PlatformError::UnsupportedPlatform);
    }
    wait_for_modifiers()?;
    // Checked again after the wait, right before the keys go down.
    let focus = focus().ok_or(PlatformError::TargetChanged)?;
    let field = if target.focused_runtime_id.is_empty() {
        crate::target::validate_frontmost_app(target, &focus.snapshot)?;
        None
    } else {
        crate::target::validate_identity(target, &focus.snapshot)?;
        Some(focus.field.ok_or(PlatformError::TargetChanged)?)
    };
    let before = field.as_ref().and_then(edit_state);
    let previous = crate::macos_clipboard::read_text();
    crate::macos_clipboard::copy_text(text)?;
    if should_cancel() {
        restore_clipboard(text, previous);
        return Err(PlatformError::Cancelled);
    }
    if let Err(error) = press_paste() {
        restore_clipboard(text, previous);
        return Err(error);
    }
    // A field that reports its length or caret shows whether the paste landed; others cannot
    // be checked.
    let mut observed = before.is_none();
    if let (Some(field), Some(before)) = (&field, before) {
        let deadline = Instant::now() + PASTE_SETTLE;
        while Instant::now() < deadline {
            if edit_state(field).is_some_and(|now| now != before) {
                observed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }
    std::thread::sleep(CLIPBOARD_HOLD);
    let restored = restore_clipboard(text, previous);
    Ok(InsertionOutcome {
        input_injected: true,
        clipboard_read_observed: observed,
        clipboard_restored: restored,
        clipboard_changed_by_other: false,
    })
}

/// The field's length and caret, whichever it reports; a paste changes at least one.
fn edit_state(field: &Element) -> Option<(Option<i64>, Option<isize>)> {
    let length = field.integer("AXNumberOfCharacters");
    let caret = field
        .range("AXSelectedTextRange")
        .map(|range| range.location);
    (length.is_some() || caret.is_some()).then_some((length, caret))
}

/// A shortcut's own keys can still be down when the text is ready; ⌘V with Control or Shift
/// held would be another shortcut.
fn wait_for_modifiers() -> Result<(), PlatformError> {
    let deadline = Instant::now() + MODIFIERS_WAIT;
    loop {
        if unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) } & MODIFIER_FLAGS == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(PlatformError::ModifiersHeld);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn press_paste() -> Result<(), PlatformError> {
    let source = unsafe { CGEventSourceCreate(HID_SYSTEM_STATE) };
    if source.is_null() {
        return Err(PlatformError::InjectionFailed);
    }
    let source = unsafe { CFType::wrap_under_create_rule(source) };
    // Both events exist before either is posted, so V is never left down.
    let event = |down| {
        let event = unsafe { CGEventCreateKeyboardEvent(source.as_CFTypeRef(), KEY_V, down) };
        (!event.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(event) })
    };
    let (Some(down), Some(up)) = (event(true), event(false)) else {
        return Err(PlatformError::InjectionFailed);
    };
    for event in [&down, &up] {
        unsafe {
            CGEventSetFlags(event.as_CFTypeRef(), FLAG_COMMAND);
            CGEventPost(HID_EVENT_TAP, event.as_CFTypeRef());
        }
    }
    Ok(())
}

/// Puts back the text the pasteboard held before, unless something else replaced the dictation
/// meanwhile. With nothing to put back it is emptied, so a later ⌘V cannot paste the dictation
/// again somewhere else.
fn restore_clipboard(dictated: &str, previous: Option<String>) -> bool {
    if crate::macos_clipboard::read_text().as_deref() != Some(dictated) {
        return false;
    }
    match previous {
        Some(previous) => crate::macos_clipboard::copy_text(&previous).is_ok(),
        None => crate::macos_clipboard::clear().is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_fields_are_known_by_role_or_subrole() {
        assert!(secure_role("AXTextField", "AXSecureTextField"));
        assert!(secure_role("AXSecureTextField", ""));
        assert!(!secure_role("AXTextField", ""));
        assert!(!secure_role("AXTextArea", "AXSearchField"));
    }

    #[test]
    fn a_field_identity_is_never_empty_and_keeps_its_process_and_hash() {
        let id = runtime_id(4242, 0x0123_4567_89ab_cdef);
        assert_eq!(id, vec![4242, 0x0123_4567, 0x89ab_cdef_u32 as i32]);
        assert_ne!(runtime_id(1, 0), Vec::<i32>::new());
        assert_ne!(role_code("AXTextField"), role_code("AXTextArea"));
        assert_eq!(role_code("AXTextArea"), role_code("AXTextArea"));
    }

    /// Runs on the macOS runners, which hold the Accessibility permission (they report typing as
    /// allowed): whatever the permission, a snapshot answers without hanging, typing follows it,
    /// and without it a snapshot names nothing.
    #[test]
    fn the_permission_decides_what_a_snapshot_names() {
        let snapshot = snapshot().expect("snapshot");
        let trusted = macos_typing::trusted();
        assert_eq!(
            can_type(&snapshot),
            trusted,
            "typing follows the permission"
        );
        assert_eq!(
            macos_typing::typing_access() == macos_typing::TypingAccess::Allowed,
            trusted,
            "typing access reports the permission"
        );
        if !trusted {
            assert_eq!(snapshot, TargetSnapshot::unidentified());
        }
    }
}
