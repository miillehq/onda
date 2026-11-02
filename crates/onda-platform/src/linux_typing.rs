//! Pressing the paste shortcut in other apps on Linux, through the remote desktop portal. The
//! user allows it once in the desktop's dialog; the portal's restore token then lets Onda start
//! the session again at launch without asking. While it runs, GNOME shows a remote-access icon
//! in the top bar, and stopping it there turns typing off until it is allowed again.
use crate::{
    portal::{session_handle, Options, Portal},
    PlatformError,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use zbus::{
    blocking::MessageIterator,
    message::Type as MessageType,
    zvariant::{ObjectPath, Value},
    MatchRule,
};

const REMOTE_DESKTOP: &str = "org.freedesktop.portal.RemoteDesktop";
const KEYBOARD: u32 = 1;
/// Kept until the user revokes it, so later launches start without a dialog.
const PERSIST_UNTIL_REVOKED: u32 = 2;
const MAX_TOKEN_BYTES: u64 = 4096;

pub const KEY_CONTROL: i32 = 0xffe3;
pub const KEY_SHIFT: i32 = 0xffe1;
pub const KEY_INSERT: i32 = 0xff63;
pub const KEY_V: i32 = 0x76;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TypingAccess {
    /// This desktop has no remote desktop portal.
    Unavailable,
    NotAllowed,
    Allowed,
}

struct Session {
    portal: Arc<Portal>,
    handle: String,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static TOKEN_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn typing_access() -> TypingAccess {
    if SESSION
        .lock()
        .map(|session| session.is_some())
        .unwrap_or(false)
    {
        return TypingAccess::Allowed;
    }
    // The desktop's portals do not change while Onda runs; ask once.
    static OFFERED: OnceLock<bool> = OnceLock::new();
    let offered = *OFFERED.get_or_init(|| {
        Portal::connect()
            .ok()
            .and_then(|portal| portal.version(REMOTE_DESKTOP))
            .is_some_and(|version| version >= 2)
    });
    if offered {
        TypingAccess::NotAllowed
    } else {
        TypingAccess::Unavailable
    }
}

/// Starts typing silently when an earlier launch was allowed; call once at startup, off the UI
/// thread. Without a saved permission this does nothing.
pub fn restore_typing(data_directory: &Path) {
    let file = data_directory.join("typing-permission");
    if let Ok(mut slot) = TOKEN_FILE.lock() {
        *slot = Some(file.clone());
    }
    let Some(token) = read_token(&file) else {
        return;
    };
    if let Err(error) = start(Some(&token)) {
        tracing::info!(subsystem = "typing", error = %error, "saved typing permission not restored");
    }
}

/// Shows the desktop's permission dialog and starts typing if the user allows it.
pub fn allow_typing() -> Result<(), PlatformError> {
    if typing_access() == TypingAccess::Allowed {
        return Ok(());
    }
    start(None)
}

/// Stops typing and forgets the saved permission.
pub fn stop_typing() {
    if let Ok(mut session) = SESSION.lock() {
        if let Some(session) = session.take() {
            close(&session);
        }
    }
    if let Some(file) = token_file() {
        let _ = std::fs::remove_file(file);
    }
}

/// Presses `keys` in order and releases them in reverse, like a person pressing a chord.
pub(crate) fn press_chord(keys: &[i32]) -> Result<(), PlatformError> {
    let session = SESSION.lock().map_err(|_| PlatformError::InjectionFailed)?;
    let session = session.as_ref().ok_or(PlatformError::UnsupportedPlatform)?;
    let handle = ObjectPath::try_from(session.handle.as_str())
        .map_err(|_| PlatformError::InjectionFailed)?;
    let mut pressed = Vec::with_capacity(keys.len());
    let mut result = Ok(());
    for key in keys {
        if let Err(error) = notify(session, &handle, *key, true) {
            result = Err(error);
            break;
        }
        pressed.push(*key);
    }
    // Release whatever went down, even after a failure, so no key stays held.
    for key in pressed.into_iter().rev() {
        let released = notify(session, &handle, key, false);
        if result.is_ok() {
            result = released;
        }
    }
    result
}

fn notify(
    session: &Session,
    handle: &ObjectPath<'_>,
    keysym: i32,
    pressed: bool,
) -> Result<(), PlatformError> {
    session
        .portal
        .call(
            REMOTE_DESKTOP,
            "NotifyKeyboardKeysym",
            &(handle, Options::new(), keysym, u32::from(pressed)),
        )
        .map(|_| ())
        .map_err(|_| PlatformError::InjectionFailed)
}

fn start(restore_token: Option<&str>) -> Result<(), PlatformError> {
    let portal = Arc::new(Portal::connect()?);
    if portal
        .version(REMOTE_DESKTOP)
        .is_none_or(|version| version < 2)
    {
        return Err(PlatformError::UnsupportedPlatform);
    }
    let token = portal.token();
    let session_token = portal.token();
    let created = portal.request(
        REMOTE_DESKTOP,
        "CreateSession",
        &(Options::from([
            ("handle_token", Value::from(token.as_str())),
            ("session_handle_token", Value::from(session_token.as_str())),
        ]),),
        &token,
    )?;
    let handle = session_handle(&created)?;
    let session = Session {
        portal: portal.clone(),
        handle,
    };
    let path = ObjectPath::try_from(session.handle.as_str())
        .map_err(|_| PlatformError::UnsupportedPlatform)?;
    let started = (|| {
        let token = portal.token();
        let mut options = Options::from([
            ("handle_token", Value::from(token.as_str())),
            ("types", Value::from(KEYBOARD)),
            ("persist_mode", Value::from(PERSIST_UNTIL_REVOKED)),
        ]);
        if let Some(restore) = restore_token {
            options.insert("restore_token", Value::from(restore));
        }
        portal.request(REMOTE_DESKTOP, "SelectDevices", &(&path, options), &token)?;
        let token = portal.token();
        portal.request(
            REMOTE_DESKTOP,
            "Start",
            &(
                &path,
                "",
                Options::from([("handle_token", Value::from(token.as_str()))]),
            ),
            &token,
        )
    })();
    let results = match started {
        Ok(results) => results,
        Err(error) => {
            close(&session);
            return Err(error);
        }
    };
    let devices = results
        .get("devices")
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    if devices & KEYBOARD == 0 {
        close(&session);
        return Err(PlatformError::Cancelled);
    }
    if let Some(token) = results
        .get("restore_token")
        .and_then(|value| <&str>::try_from(value).ok())
    {
        save_token(token);
    }
    watch_closed(portal, session.handle.clone());
    *SESSION.lock().map_err(|_| PlatformError::InjectionFailed)? = Some(session);
    tracing::info!(subsystem = "typing", "typing into apps started");
    Ok(())
}

/// The user can stop the session from the desktop's indicator; typing then needs permission
/// again, so the saved token goes too.
fn watch_closed(portal: Arc<Portal>, handle: String) {
    let _ = std::thread::Builder::new()
        .name("onda-typing-session".into())
        .spawn(move || {
            let rule = MatchRule::builder()
                .msg_type(MessageType::Signal)
                .interface("org.freedesktop.portal.Session")
                .and_then(|rule| rule.member("Closed"))
                .and_then(|rule| rule.path(handle.as_str()))
                .map(|rule| rule.build());
            let Ok(rule) = rule else { return };
            let Ok(mut closed) =
                MessageIterator::for_match_rule(rule, portal.connection(), Some(2))
            else {
                return;
            };
            if closed.next().is_some() {
                if let Ok(mut session) = SESSION.lock() {
                    if session
                        .as_ref()
                        .is_some_and(|current| current.handle == handle)
                    {
                        *session = None;
                    }
                }
                if let Some(file) = token_file() {
                    let _ = std::fs::remove_file(file);
                }
                tracing::info!(
                    subsystem = "typing",
                    "typing into apps stopped from the desktop"
                );
            }
        });
}

fn close(session: &Session) {
    let _ = session.portal.connection().call_method(
        Some(crate::portal::DESTINATION),
        session.handle.as_str(),
        Some("org.freedesktop.portal.Session"),
        "Close",
        &(),
    );
}

fn token_file() -> Option<PathBuf> {
    TOKEN_FILE.lock().ok().and_then(|file| file.clone())
}

fn read_token(file: &Path) -> Option<String> {
    let metadata = std::fs::metadata(file).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_TOKEN_BYTES {
        return None;
    }
    let token = std::fs::read_to_string(file).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

/// A token is good for one start; each start returns the next one.
fn save_token(token: &str) {
    let Some(file) = token_file() else { return };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut handle) = options.open(&file) {
        use std::io::Write;
        let _ = handle.write_all(token.as_bytes());
    }
}
