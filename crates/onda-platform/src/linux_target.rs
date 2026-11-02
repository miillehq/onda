//! The focused field on Linux, read through the accessibility bus (AT-SPI): which field it is,
//! its text, caret and selection, and editing it. GTK, Qt and LibreOffice expose their fields
//! by default; browsers and Electron apps only when the desktop's accessibility support is on.
//! Where no field is visible, the snapshot is unidentified and dictation can only paste blind.
use crate::{
    linux_typing,
    target::{ControlFingerprint, InsertionOutcome, PasteShortcut},
    PlatformError, TargetSnapshot,
};
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use zbus::{
    blocking::{Connection, MessageIterator},
    message::Type as MessageType,
    zvariant::{OwnedObjectPath, OwnedValue},
    MatchRule,
};

const ROLE_DIALOG: u32 = 16;
const ROLE_FRAME: u32 = 23;
const ROLE_PASSWORD_TEXT: u32 = 40;
const ROLE_TERMINAL: u32 = 60;
const ROLE_WINDOW: u32 = 69;
const ROLE_APPLICATION: u32 = 75;
const STATE_ACTIVE: u32 = 1;
const STATE_DEFUNCT: u32 = 6;
const STATE_EDITABLE: u32 = 7;
const STATE_FOCUSED: u32 = 12;

const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const TEXT: &str = "org.a11y.atspi.Text";
const EDITABLE_TEXT: &str = "org.a11y.atspi.EditableText";
const COMPONENT: &str = "org.a11y.atspi.Component";
const APPLICATION: &str = "org.a11y.atspi.Application";
const COORDINATES_IN_WINDOW: u32 = 1;
/// Bounds the search for a focused field in an app whose focus change Onda did not hear.
const MAX_SEARCHED_OBJECTS: usize = 3000;
const MAX_PARENT_DEPTH: usize = 64;
/// How long a pasted text may take to show in the field before Onda stops waiting.
const PASTE_SETTLE: Duration = Duration::from_millis(800);
const CALL_TIMEOUT: Duration = Duration::from_millis(1500);
/// How long the paste keeps the clipboard before the previous text comes back.
const CLIPBOARD_HOLD: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Object {
    bus: String,
    path: String,
}

static BUS: OnceLock<Option<Connection>> = OnceLock::new();
static FOCUS: Mutex<Option<Object>> = Mutex::new(None);
static TRACKING: OnceLock<()> = OnceLock::new();

fn bus() -> Result<&'static Connection, PlatformError> {
    BUS.get_or_init(|| {
        let session = Connection::session().ok()?;
        let reply = session
            .call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.a11y.Bus"),
                "GetAddress",
                &(),
            )
            .ok()?;
        let address: String = reply.body().deserialize().ok()?;
        // A hung app must not hold up dictation; the bus itself would wait 25 seconds.
        zbus::blocking::connection::Builder::address(address.as_str())
            .ok()?
            .method_timeout(CALL_TIMEOUT)
            .build()
            .ok()
    })
    .as_ref()
    .ok_or_else(|| PlatformError::TargetUnavailable("the accessibility bus is unavailable".into()))
}

/// Listens for focus changes so a snapshot need not search every app. Call once at startup.
pub fn start_focus_tracking() {
    TRACKING.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("onda-focus".into())
            .spawn(track_focus);
    });
}

fn track_focus() {
    let Ok(connection) = bus() else { return };
    // Apps send events only while someone is listening; newer registries take extra arguments.
    let event = "object:state-changed:focused";
    let registered = connection
        .call_method(
            Some("org.a11y.atspi.Registry"),
            "/org/a11y/atspi/registry",
            Some("org.a11y.atspi.Registry"),
            "RegisterEvent",
            &(event, Vec::<String>::new(), ""),
        )
        .or_else(|_| {
            connection.call_method(
                Some("org.a11y.atspi.Registry"),
                "/org/a11y/atspi/registry",
                Some("org.a11y.atspi.Registry"),
                "RegisterEvent",
                &(event,),
            )
        });
    if registered.is_err() {
        return;
    }
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface("org.a11y.atspi.Event.Object")
        .and_then(|rule| rule.member("StateChanged"))
        .map(|rule| rule.build());
    let Ok(rule) = rule else { return };
    let Ok(events) = MessageIterator::for_match_rule(rule, connection, Some(64)) else {
        return;
    };
    for message in events.flatten() {
        let header = message.header();
        let (Some(sender), Some(path)) = (header.sender(), header.path()) else {
            continue;
        };
        // The body carries more fields, whose types vary between versions; only these are read.
        let Ok((kind, gained, _)) = message.body().deserialize_unchecked::<(String, i32, i32)>()
        else {
            continue;
        };
        if kind != "focused" {
            continue;
        }
        let object = Object {
            bus: sender.to_string(),
            path: path.to_string(),
        };
        if let Ok(mut focus) = FOCUS.lock() {
            if gained == 1 {
                *focus = Some(object);
            } else if focus.as_ref() == Some(&object) {
                *focus = None;
            }
        }
    }
}

fn call<B, R>(object: &Object, interface: &str, method: &str, body: &B) -> Result<R, PlatformError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = bus()?
        .call_method(
            Some(object.bus.as_str()),
            object.path.as_str(),
            Some(interface),
            method,
            body,
        )
        .map_err(|error| match error {
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod"
                    || name.as_str() == "org.freedesktop.DBus.Error.UnknownInterface" =>
            {
                PlatformError::SelectionUnavailable
            }
            _ => PlatformError::TargetUnavailable("the focused app stopped answering".into()),
        })?;
    reply.body().deserialize().map_err(|_| {
        PlatformError::TargetUnavailable("the focused app sent an unexpected reply".into())
    })
}

fn property(object: &Object, interface: &str, name: &str) -> Result<OwnedValue, PlatformError> {
    call(
        object,
        "org.freedesktop.DBus.Properties",
        "Get",
        &(interface, name),
    )
}

fn integer(object: &Object, interface: &str, name: &str) -> Result<i32, PlatformError> {
    i32::try_from(property(object, interface, name)?)
        .map_err(|_| PlatformError::TargetUnavailable("unexpected accessibility value".into()))
}

fn states(object: &Object) -> Result<Vec<u32>, PlatformError> {
    call(object, ACCESSIBLE, "GetState", &())
}

fn has(states: &[u32], state: u32) -> bool {
    states
        .get((state / 32) as usize)
        .is_some_and(|word| word & (1 << (state % 32)) != 0)
}

fn role(object: &Object) -> Result<u32, PlatformError> {
    call(object, ACCESSIBLE, "GetRole", &())
}

fn reference((bus, path): (String, OwnedObjectPath)) -> Object {
    Object {
        bus,
        path: path.to_string(),
    }
}

fn parent(object: &Object) -> Result<Object, PlatformError> {
    let value = property(object, ACCESSIBLE, "Parent")?;
    let pair = <(String, OwnedObjectPath)>::try_from(value)
        .map_err(|_| PlatformError::TargetUnavailable("unexpected accessibility value".into()))?;
    Ok(reference(pair))
}

fn children(object: &Object) -> Result<Vec<Object>, PlatformError> {
    let children: Vec<(String, OwnedObjectPath)> = call(object, ACCESSIBLE, "GetChildren", &())?;
    Ok(children.into_iter().map(reference).collect())
}

/// The focused object, if an app reports one in the active window. The tracked focus is
/// checked live: an app that lost focus to one without accessibility still claims it.
fn focused() -> Result<Option<Object>, PlatformError> {
    bus()?;
    let tracked = FOCUS.lock().ok().and_then(|focus| focus.clone());
    if let Some(object) = tracked {
        if let Ok(current) = states(&object) {
            if has(&current, STATE_FOCUSED) && !has(&current, STATE_DEFUNCT) {
                return Ok(window_is_active(&object).then_some(object));
            }
        }
    }
    let found = search_focused();
    if let (Some(object), Ok(mut focus)) = (&found, FOCUS.lock()) {
        *focus = Some(object.clone());
    }
    Ok(found)
}

fn window_is_active(object: &Object) -> bool {
    window_of(object)
        .and_then(|window| states(&window).ok())
        .is_some_and(|states| has(&states, STATE_ACTIVE))
}

/// The top-level window holding `object`: the last ancestor below the application.
fn window_of(object: &Object) -> Option<Object> {
    let mut current = object.clone();
    for _ in 0..MAX_PARENT_DEPTH {
        let role = role(&current).ok()?;
        if matches!(role, ROLE_FRAME | ROLE_DIALOG | ROLE_WINDOW) {
            return Some(current);
        }
        let parent = parent(&current).ok()?;
        if parent.path == ROOT_PATH || role == ROLE_APPLICATION || parent == current {
            return Some(current);
        }
        current = parent;
    }
    None
}

/// Breadth-first through the active window of each app, bounded in size.
fn search_focused() -> Option<Object> {
    let registry = Object {
        bus: "org.a11y.atspi.Registry".into(),
        path: ROOT_PATH.into(),
    };
    let mut queue: VecDeque<Object> = VecDeque::new();
    for app in children(&registry).ok()? {
        for window in children(&app).unwrap_or_default() {
            if states(&window).is_ok_and(|states| has(&states, STATE_ACTIVE)) {
                queue.push_back(window);
            }
        }
    }
    let mut visited = 0;
    while let Some(object) = queue.pop_front() {
        visited += 1;
        if visited > MAX_SEARCHED_OBJECTS {
            break;
        }
        let Ok(current) = states(&object) else {
            continue;
        };
        if has(&current, STATE_FOCUSED) {
            return Some(object);
        }
        queue.extend(children(&object).unwrap_or_default());
    }
    None
}

/// Stable within one app connection; two numbers from a hash of the bus name and path.
fn identity(object: &Object) -> Vec<i32> {
    let hash = fnv(&[object.bus.as_bytes(), b"\0", object.path.as_bytes()]);
    vec![(hash >> 32) as i32, hash as i32]
}

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in parts.iter().flat_map(|part| part.iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn process_id(bus_name: &str) -> u32 {
    bus()
        .ok()
        .and_then(|connection| {
            connection
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetConnectionUnixProcessID",
                    &(bus_name,),
                )
                .ok()
        })
        .and_then(|reply| reply.body().deserialize::<u32>().ok())
        .unwrap_or(0)
}

/// Clock ticks since boot at which the process started, so a reused process ID differs.
fn process_started(process_id: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{process_id}/stat"))
        .ok()
        .and_then(|stat| {
            let after_name = stat.rsplit_once(')')?.1;
            after_name.split_whitespace().nth(19)?.parse().ok()
        })
        .unwrap_or(0)
}

/// The executable's file name; `comm` is cut to 15 characters ("gnome-text-edit").
fn process_name(process_id: u32) -> String {
    std::fs::read_link(format!("/proc/{process_id}/exe"))
        .ok()
        .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
        .or_else(|| {
            std::fs::read_to_string(format!("/proc/{process_id}/comm"))
                .ok()
                .map(|name| name.trim().to_owned())
        })
        .unwrap_or_default()
}

fn snapshot_of(object: &Object) -> Result<TargetSnapshot, PlatformError> {
    let role = role(object)?;
    let process_id = process_id(&object.bus);
    let window = window_of(object).unwrap_or_else(|| object.clone());
    let automation_id = property(object, ACCESSIBLE, "AccessibleId")
        .ok()
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default();
    let application = Object {
        bus: object.bus.clone(),
        path: ROOT_PATH.into(),
    };
    let framework_id = property(&application, APPLICATION, "ToolkitName")
        .ok()
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default();
    let bounds =
        call::<_, (i32, i32, i32, i32)>(object, COMPONENT, "GetExtents", &(COORDINATES_IN_WINDOW,))
            .map(|(x, y, width, height)| [x, y, x + width, y + height])
            .unwrap_or_default();
    Ok(TargetSnapshot {
        window_handle: fnv(&[window.bus.as_bytes(), b"\0", window.path.as_bytes()]) as isize,
        process_id,
        process_created_at: process_started(process_id),
        executable_name: process_name(process_id),
        focused_runtime_id: identity(object),
        is_password: role == ROLE_PASSWORD_TEXT,
        integrity_level: 0,
        focused_control: ControlFingerprint {
            process_id,
            control_type: role as i32,
            automation_id,
            framework_id,
            bounds,
        },
    })
}

pub(crate) fn snapshot() -> Result<TargetSnapshot, PlatformError> {
    match focused() {
        Ok(Some(object)) => snapshot_of(&object),
        // No accessibility bus, or no app reports a focused field.
        Ok(None) | Err(_) => Ok(TargetSnapshot::unidentified()),
    }
}

/// The focused object, when it is still the field `target` describes.
fn field(target: &TargetSnapshot) -> Result<Object, PlatformError> {
    let object = focused()?.ok_or(PlatformError::TargetChanged)?;
    crate::target::validate_identity(target, &snapshot_of(&object)?)?;
    Ok(object)
}

fn text_range(object: &Object, start: i32, end: i32) -> Result<String, PlatformError> {
    call(object, TEXT, "GetText", &(start, end))
}

/// The one selection as character offsets, or `None` when nothing is selected.
fn selection(object: &Object) -> Result<Option<(i32, i32)>, PlatformError> {
    let count: i32 = call(object, TEXT, "GetNSelections", &())?;
    match count {
        0 => Ok(None),
        1 => {
            let (start, end): (i32, i32) = call(object, TEXT, "GetSelection", &(0,))?;
            Ok((start != end).then_some((start.min(end), start.max(end))))
        }
        _ => Err(PlatformError::SelectionUnavailable),
    }
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
    let object = field(target)?;
    let text = if selected {
        match selection(&object)? {
            Some((start, end)) => text_range(&object, start, end)?,
            None => return Err(PlatformError::EmptySelection),
        }
    } else {
        let count = integer(&object, TEXT, "CharacterCount")?;
        if usize::try_from(count).unwrap_or(usize::MAX) > max_characters {
            return Err(PlatformError::TextLimit);
        }
        text_range(&object, 0, count)?
    };
    check_limit(&text, max_characters)?;
    field(target)?;
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
    let object = field(target)?;
    let caret = match selection(&object) {
        Ok(Some((start, _))) => start,
        _ => integer(&object, TEXT, "CaretOffset")?,
    };
    if caret <= 0 {
        return Ok(String::new());
    }
    let start = caret.saturating_sub(i32::try_from(max_characters).unwrap_or(i32::MAX));
    let text = text_range(&object, start.max(0), caret)?;
    let skip = text.chars().count().saturating_sub(max_characters);
    Ok(text.chars().skip(skip).collect())
}

pub(crate) fn select_unique_text(
    target: &TargetSnapshot,
    expected_document: &str,
    needle: &str,
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), PlatformError> {
    if needle.is_empty() || expected_document.match_indices(needle).count() != 1 {
        return Err(PlatformError::TargetChanged);
    }
    let object = field(target)?;
    let count = integer(&object, TEXT, "CharacterCount")?;
    if text_range(&object, 0, count)? != expected_document {
        return Err(PlatformError::TargetChanged);
    }
    let byte_start = expected_document
        .find(needle)
        .ok_or(PlatformError::TargetChanged)?;
    let start = expected_document[..byte_start].chars().count() as i32;
    let end = start + needle.chars().count() as i32;
    if should_cancel() {
        return Err(PlatformError::Cancelled);
    }
    let selected: bool = if selection(&object)?.is_some() {
        call(&object, TEXT, "SetSelection", &(0, start, end))?
    } else {
        call(&object, TEXT, "AddSelection", &(start, end))?
    };
    if !selected || selection(&object)? != Some((start, end)) {
        return Err(PlatformError::SelectionUnavailable);
    }
    Ok(())
}

/// Whether a dictation into `target` can be typed: with the paste permission anywhere, or
/// without it into a field that accepts edits through accessibility.
pub(crate) fn can_type(target: &TargetSnapshot) -> bool {
    if linux_typing::typing_access() == linux_typing::TypingAccess::Allowed {
        return true;
    }
    !target.focused_runtime_id.is_empty() && field(target).is_ok_and(|object| editable(&object))
}

fn editable(object: &Object) -> bool {
    states(object).is_ok_and(|states| has(&states, STATE_EDITABLE))
        && call::<_, Vec<String>>(object, ACCESSIBLE, "GetInterfaces", &())
            .is_ok_and(|interfaces| interfaces.iter().any(|name| name == EDITABLE_TEXT))
}

/// With the paste permission, the clipboard and the paste shortcut, as on Windows. Without it,
/// the text goes in through the field's own editing interface.
pub(crate) fn paste(
    target: &TargetSnapshot,
    text: &str,
    shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    // Unidentified means no app reports focus; there is nothing to check, so paste blind.
    let object = if target.focused_runtime_id.is_empty() {
        None
    } else {
        Some(field(target)?)
    };
    if linux_typing::typing_access() == linux_typing::TypingAccess::Allowed {
        return paste_with_shortcut(object.as_ref(), text, shortcut, should_cancel);
    }
    match object {
        Some(object) if editable(&object) => insert_directly(&object, text, should_cancel),
        _ => Err(PlatformError::UnsupportedPlatform),
    }
}

fn paste_with_shortcut(
    object: Option<&Object>,
    text: &str,
    shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    // Terminals paste with Ctrl+Shift+V; Ctrl+V would reach the shell as a control character.
    let shortcut = match object.map(role) {
        Some(Ok(ROLE_TERMINAL)) if shortcut == PasteShortcut::ControlV => {
            PasteShortcut::ControlShiftV
        }
        _ => shortcut,
    };
    let keys: &[i32] = match shortcut {
        PasteShortcut::ControlV => &[linux_typing::KEY_CONTROL, linux_typing::KEY_V],
        PasteShortcut::ControlShiftV => &[
            linux_typing::KEY_CONTROL,
            linux_typing::KEY_SHIFT,
            linux_typing::KEY_V,
        ],
        PasteShortcut::ShiftInsert => &[linux_typing::KEY_SHIFT, linux_typing::KEY_INSERT],
    };
    let before = object.and_then(|object| integer(object, TEXT, "CharacterCount").ok());
    let previous = crate::linux_clipboard::read_text();
    crate::linux_clipboard::copy_text(text)?;
    if should_cancel() {
        restore_clipboard(text, previous);
        return Err(PlatformError::Cancelled);
    }
    if let Err(error) = linux_typing::press_chord(keys) {
        restore_clipboard(text, previous);
        return Err(error);
    }
    // A field that reports its text shows whether the paste landed; others cannot be checked.
    let mut observed = before.is_none();
    if let (Some(object), Some(before)) = (object, before) {
        let deadline = Instant::now() + PASTE_SETTLE;
        while Instant::now() < deadline {
            if integer(object, TEXT, "CharacterCount").is_ok_and(|count| count != before) {
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

/// Puts back the text the clipboard held before, unless something else replaced the
/// dictation meanwhile. With nothing to put back it is emptied, so a later Ctrl+V cannot paste
/// the dictation again somewhere else.
fn restore_clipboard(dictated: &str, previous: Option<String>) -> bool {
    if crate::linux_clipboard::read_text().as_deref() != Some(dictated) {
        return false;
    }
    match previous {
        Some(previous) => crate::linux_clipboard::copy_text(&previous).is_ok(),
        None => crate::linux_clipboard::clear().is_ok(),
    }
}

fn insert_directly(
    object: &Object,
    text: &str,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    let position = match selection(object)? {
        Some((start, end)) => {
            if should_cancel() {
                return Err(PlatformError::Cancelled);
            }
            let deleted: bool = call(object, EDITABLE_TEXT, "DeleteText", &(start, end))?;
            if !deleted {
                return Err(PlatformError::InjectionFailed);
            }
            start
        }
        None => integer(object, TEXT, "CaretOffset")?,
    };
    if should_cancel() {
        return Err(PlatformError::Cancelled);
    }
    let length = i32::try_from(text.chars().count()).map_err(|_| PlatformError::TextLimit)?;
    let inserted: bool = call(
        object,
        EDITABLE_TEXT,
        "InsertText",
        &(position, text, length),
    )?;
    if !inserted {
        return Err(PlatformError::InjectionFailed);
    }
    // Leave the caret after the new text, where typing would have left it.
    let _: Result<bool, _> = call(object, TEXT, "SetCaretOffset", &(position + length,));
    Ok(InsertionOutcome {
        input_injected: true,
        clipboard_read_observed: true,
        clipboard_restored: true,
        clipboard_changed_by_other: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_are_bits_across_two_words() {
        let states = [(1 << STATE_FOCUSED) | (1 << STATE_ACTIVE), 1 << 3];
        assert!(has(&states, STATE_FOCUSED) && has(&states, STATE_ACTIVE));
        assert!(!has(&states, STATE_EDITABLE));
        assert!(has(&states, 35));
        assert!(!has(&states, 64), "beyond the reported words");
    }

    #[test]
    fn a_process_is_identified_by_its_start_time_and_name() {
        let id = std::process::id();
        assert!(process_started(id) > 0);
        assert!(!process_name(id).is_empty());
        assert_eq!(process_started(u32::MAX), 0);
    }

    #[test]
    fn object_identity_is_stable_and_distinct() {
        let field = Object {
            bus: ":1.42".into(),
            path: "/org/a11y/atspi/accessible/7".into(),
        };
        let other = Object {
            bus: ":1.43".into(),
            ..field.clone()
        };
        assert_eq!(identity(&field), identity(&field.clone()));
        assert_ne!(identity(&field), identity(&other));
        assert_eq!(identity(&field).len(), 2);
    }
}
