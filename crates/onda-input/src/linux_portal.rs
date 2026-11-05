//! Linux shortcuts through the desktop's global shortcuts portal. The desktop owns the keys: it
//! asks the user once to approve Onda's shortcuts, may let them choose other keys, and then
//! reports each press and release of those shortcuts only. Onda never sees other keystrokes.
use onda_input::{
    HelperCommand, HelperConfig, HelperEvent, InputError, Modifiers, ShortcutAction,
    ShortcutBinding, HEARTBEAT_INTERVAL_MS, MAX_PROTOCOL_LINE_BYTES,
};
use onda_platform::{
    portal::{session_handle, Options, Portal, PATH},
    PlatformError,
};
use std::{
    io::{BufRead, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use zbus::{
    blocking::MessageIterator,
    message::Type as MessageType,
    zvariant::{ObjectPath, OwnedObjectPath, Value},
    MatchRule,
};

const GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
/// A release this soon after the press means the desktop reports the press only, or the keys
/// were tapped: either way, recording continues until the shortcut is pressed again.
const TAP: Duration = Duration::from_millis(200);
/// GNOME reports each keyboard repeat of held keys as another press: one comes after the
/// repeat delay (500 ms by default), then many a second. A press this soon after the last one
/// for the same shortcut, with no release between, is such a repeat.
const REPEAT: Duration = Duration::from_millis(700);
/// A hold-to-talk press counts as finished once Onda has had this long to start and nothing
/// is running: the recording was cancelled or failed, or ended some other way.
const SETTLE: Duration = Duration::from_millis(1500);

type Output = Arc<Mutex<std::io::Stdout>>;

fn emit(output: &Output, event: &HelperEvent) {
    let Ok(mut output) = output.lock() else {
        std::process::exit(1);
    };
    if serde_json::to_writer(&mut *output, event).is_err()
        || output.write_all(b"\n").is_err()
        || output.flush().is_err()
    {
        std::process::exit(1);
    }
}

pub fn run() -> Result<(), InputError> {
    let config = onda_input::read_config()?;
    let output: Output = Arc::new(Mutex::new(std::io::stdout()));
    // Heartbeats start at once: the approval dialog can stay open for as long as the user likes.
    let heartbeat = output.clone();
    let started = Instant::now();
    thread::Builder::new()
        .name("onda-portal-heartbeat".into())
        .spawn(move || loop {
            emit(
                &heartbeat,
                &HelperEvent::Heartbeat {
                    uptime_ms: started.elapsed().as_millis() as u64,
                },
            );
            thread::sleep(Duration::from_millis(u64::from(HEARTBEAT_INTERVAL_MS)));
        })?;
    let operation_active = Arc::new(AtomicBool::new(false));
    let active = operation_active.clone();
    thread::Builder::new()
        .name("onda-portal-commands".into())
        .spawn(move || read_commands(&active))?;

    let portal = Portal::connect().map_err(unsupported)?;
    if portal.version(GLOBAL_SHORTCUTS).is_none() {
        return Err(InputError::Helper("unsupported".into()));
    }
    let token = portal.token();
    let session_token = portal.token();
    let created = portal
        .request(
            GLOBAL_SHORTCUTS,
            "CreateSession",
            &(Options::from([
                ("handle_token", Value::from(token.as_str())),
                ("session_handle_token", Value::from(session_token.as_str())),
            ]),),
            &token,
        )
        .map_err(unsupported)?;
    let session = session_handle(&created).map_err(unsupported)?;
    let session_path =
        ObjectPath::try_from(session.as_str()).map_err(|_| InputError::Helper("failed".into()))?;

    // Listen before binding, so a press right after approval is not lost.
    let signals = |member: &str| -> Result<MessageIterator, InputError> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(GLOBAL_SHORTCUTS)
            .and_then(|rule| rule.member(member.to_owned()))
            .and_then(|rule| rule.path(PATH))
            .map_err(|_| InputError::Helper("failed".into()))?
            .build();
        MessageIterator::for_match_rule(rule, portal.connection(), Some(64))
            .map_err(|_| InputError::Helper("failed".into()))
    };
    let activated = signals("Activated")?;
    let deactivated = signals("Deactivated")?;

    let shortcuts = shortcuts(&config);
    let token = portal.token();
    portal
        .request(
            GLOBAL_SHORTCUTS,
            "BindShortcuts",
            &(
                &session_path,
                shortcuts,
                "",
                Options::from([("handle_token", Value::from(token.as_str()))]),
            ),
            &token,
        )
        .map_err(|error| match error {
            PlatformError::Cancelled => InputError::Helper("declined".into()),
            other => unsupported(other),
        })?;
    emit(
        &output,
        &HelperEvent::Ready {
            protocol_version: onda_input::PROTOCOL_VERSION,
        },
    );

    // Both signal kinds feed one ordered queue, so a press and its release stay in order.
    let (sender, receiver) = std::sync::mpsc::sync_channel(64);
    for (iterator, pressed) in [(activated, true), (deactivated, false)] {
        let sender = sender.clone();
        let session = session.clone();
        thread::Builder::new()
            .name("onda-portal-signals".into())
            .spawn(move || {
                for message in iterator.flatten() {
                    let Ok((handle, id, _, _)) = message.body().deserialize::<(
                        OwnedObjectPath,
                        String,
                        u64,
                        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                    )>() else {
                        continue;
                    };
                    if handle.as_str() == session && sender.send((id, pressed)).is_err() {
                        break;
                    }
                }
            })?;
    }
    drop(sender);
    let mut matcher = PortalMatcher::new(config.bindings);
    for (id, pressed) in receiver {
        let active = operation_active.load(Ordering::Acquire);
        for event in matcher.handle(&id, pressed, active, Instant::now()) {
            emit(&output, &event);
        }
    }
    Err(InputError::Helper("failed".into()))
}

/// A portal that is missing or refuses the session leaves the same way out: a desktop shortcut
/// that runs `onda --dictate`. Restarting the helper would only fail again.
fn unsupported(_error: PlatformError) -> InputError {
    InputError::Helper("unsupported".into())
}

fn read_commands(operation_active: &AtomicBool) {
    let input = std::io::stdin();
    let mut input = input.lock();
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            // The parent closed its end: Onda has quit, so the helper goes too.
            Ok(0) | Err(_) => std::process::exit(0),
            Ok(_) if line.len() > MAX_PROTOCOL_LINE_BYTES => std::process::exit(1),
            Ok(_) => match serde_json::from_str::<HelperCommand>(&line) {
                Ok(HelperCommand::SetOperationActive { active }) => {
                    operation_active.store(active, Ordering::Release)
                }
                Ok(HelperCommand::Shutdown) => std::process::exit(0),
                Err(_) => std::process::exit(1),
            },
        }
    }
}

fn binding_id(binding: &ShortcutBinding) -> String {
    match (&binding.action, &binding.transform_id) {
        (ShortcutAction::Transform, Some(id)) => format!("transform-{id}"),
        (action, _) => format!("{action:?}").to_lowercase(),
    }
}

/// The shortcuts as the portal takes them: an id, a description the desktop shows, and the
/// keys Onda suggests. The desktop may let the user choose different keys.
fn shortcuts(config: &HelperConfig) -> Vec<(String, Options<'static>)> {
    let dictation_modifiers = config
        .bindings
        .iter()
        .find(|binding| binding.action == ShortcutAction::Dictation)
        .map(|binding| binding.modifiers)
        .unwrap_or_default();
    let mut rewrites = 0;
    config
        .bindings
        .iter()
        .map(|binding| {
            let description = match binding.action {
                ShortcutAction::Dictation => "Hold to talk".to_owned(),
                ShortcutAction::Command => "Edit with your voice".to_owned(),
                ShortcutAction::Toggle => "Start or stop hands-free dictation".to_owned(),
                ShortcutAction::Cancel => "Cancel dictation".to_owned(),
                ShortcutAction::Transform if binding.transform_id.is_none() => {
                    "Rewrite selected text".to_owned()
                }
                ShortcutAction::Transform => {
                    rewrites += 1;
                    format!("Rewrite selected text, saved rewrite {rewrites}")
                }
            };
            // Esc alone would be taken from every app; with the dictation modifiers it can be
            // pressed while still holding them.
            let modifiers = if binding.action == ShortcutAction::Cancel {
                dictation_modifiers
            } else {
                binding.modifiers
            };
            let mut options = Options::from([("description", Value::from(description))]);
            if let Some(trigger) = trigger(binding.virtual_key, modifiers) {
                options.insert("preferred_trigger", Value::from(trigger));
            }
            (binding_id(binding), options)
        })
        .collect()
}

/// Windows virtual-key codes to the trigger syntax of the XDG shortcuts specification.
fn trigger(virtual_key: u16, modifiers: Modifiers) -> Option<String> {
    let key = match virtual_key {
        0x08 => "BackSpace".to_owned(),
        0x09 => "Tab".to_owned(),
        0x0D => "Return".to_owned(),
        0x13 => "Pause".to_owned(),
        0x1B => "Escape".to_owned(),
        0x20 => "space".to_owned(),
        0x21 => "Page_Up".to_owned(),
        0x22 => "Page_Down".to_owned(),
        0x23 => "End".to_owned(),
        0x24 => "Home".to_owned(),
        0x25 => "Left".to_owned(),
        0x26 => "Up".to_owned(),
        0x27 => "Right".to_owned(),
        0x28 => "Down".to_owned(),
        0x2C => "Print".to_owned(),
        0x2D => "Insert".to_owned(),
        0x2E => "Delete".to_owned(),
        0x30..=0x39 | 0x41..=0x5A => char::from(virtual_key as u8)
            .to_ascii_lowercase()
            .to_string(),
        0x60..=0x69 => format!("KP_{}", virtual_key - 0x60),
        0x6A => "KP_Multiply".to_owned(),
        0x6B => "KP_Add".to_owned(),
        0x6D => "KP_Subtract".to_owned(),
        0x6E => "KP_Decimal".to_owned(),
        0x6F => "KP_Divide".to_owned(),
        0x70..=0x87 => format!("F{}", virtual_key - 0x6F),
        0x91 => "Scroll_Lock".to_owned(),
        0xBA => "semicolon".to_owned(),
        0xBB => "equal".to_owned(),
        0xBC => "comma".to_owned(),
        0xBD => "minus".to_owned(),
        0xBE => "period".to_owned(),
        0xBF => "slash".to_owned(),
        0xC0 => "grave".to_owned(),
        0xDB => "bracketleft".to_owned(),
        0xDC => "backslash".to_owned(),
        0xDD => "bracketright".to_owned(),
        0xDE => "apostrophe".to_owned(),
        _ => return None,
    };
    let mut parts = Vec::new();
    if modifiers.control {
        parts.push("CTRL");
    }
    if modifiers.alt {
        parts.push("ALT");
    }
    if modifiers.shift {
        parts.push("SHIFT");
    }
    if modifiers.windows {
        parts.push("LOGO");
    }
    parts.push(&key);
    Some(parts.join("+"))
}

/// A hold-to-talk or voice-edit press that has not finished yet.
#[derive(Clone, Copy)]
struct Held {
    index: usize,
    since: Instant,
    /// Released right after the press: recording continues until the next press.
    latched: bool,
}

/// The Windows matcher's rules for portal events: nothing starts while a hold-to-talk shortcut
/// is held down, and cancel only while there is something to cancel. Only hold-to-talk and
/// voice edit are held; the other shortcuts act once per press, so a release the desktop never
/// reports cannot leave them in the way.
struct PortalMatcher {
    bindings: Vec<ShortcutBinding>,
    held: Option<Held>,
    /// When each shortcut was last reported pressed, until its release.
    pressed_at: Vec<Option<Instant>>,
}

impl PortalMatcher {
    fn new(bindings: Vec<ShortcutBinding>) -> Self {
        Self {
            pressed_at: vec![None; bindings.len()],
            bindings,
            held: None,
        }
    }

    fn event(&self, index: usize, pressed: bool) -> HelperEvent {
        let binding = &self.bindings[index];
        let (action, transform_id) = (binding.action, binding.transform_id.clone());
        if pressed {
            HelperEvent::Pressed {
                action,
                transform_id,
            }
        } else {
            HelperEvent::Released {
                action,
                transform_id,
            }
        }
    }

    fn handle(
        &mut self,
        id: &str,
        pressed: bool,
        operation_active: bool,
        now: Instant,
    ) -> Vec<HelperEvent> {
        let Some(index) = self
            .bindings
            .iter()
            .position(|binding| binding_id(binding) == id)
        else {
            return Vec::new();
        };
        if pressed {
            let repeat = self.pressed_at[index].is_some_and(|at| now.duration_since(at) < REPEAT);
            self.pressed_at[index] = Some(now);
            if repeat {
                return Vec::new();
            }
        } else {
            self.pressed_at[index] = None;
        }
        if self
            .held
            .is_some_and(|held| !operation_active && now.duration_since(held.since) > SETTLE)
        {
            self.held = None;
        }
        match self.bindings[index].action {
            ShortcutAction::Cancel => {
                if pressed && operation_active {
                    vec![self.event(index, true)]
                } else {
                    Vec::new()
                }
            }
            ShortcutAction::Dictation | ShortcutAction::Command => self.hold(index, pressed, now),
            ShortcutAction::Toggle | ShortcutAction::Transform => {
                let holding = self.held.is_some_and(|held| !held.latched);
                if pressed && !holding {
                    vec![self.event(index, true)]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn hold(&mut self, index: usize, pressed: bool, now: Instant) -> Vec<HelperEvent> {
        match (self.held, pressed) {
            (None, true) => {
                self.held = Some(Held {
                    index,
                    since: now,
                    latched: false,
                });
                vec![self.event(index, true)]
            }
            // Pressed again while latched, or the desktop never reported the release.
            (Some(held), true) if held.index == index => {
                self.held = None;
                vec![self.event(index, false)]
            }
            (Some(held), false) if held.index == index => {
                if !held.latched && now.duration_since(held.since) < TAP {
                    self.held = Some(Held {
                        latched: true,
                        ..held
                    });
                    return Vec::new();
                }
                self.held = None;
                vec![self.event(index, false)]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(action: ShortcutAction, virtual_key: u16) -> ShortcutBinding {
        ShortcutBinding {
            action,
            transform_id: None,
            virtual_key,
            modifiers: Modifiers {
                control: true,
                windows: true,
                ..Modifiers::default()
            },
        }
    }

    fn kinds(events: &[HelperEvent]) -> Vec<(bool, ShortcutAction)> {
        events
            .iter()
            .map(|event| match event {
                HelperEvent::Pressed { action, .. } => (true, *action),
                HelperEvent::Released { action, .. } => (false, *action),
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn triggers_follow_the_shortcuts_specification() {
        let modifiers = Modifiers {
            control: true,
            windows: true,
            ..Modifiers::default()
        };
        assert_eq!(trigger(0x20, modifiers).as_deref(), Some("CTRL+LOGO+space"));
        assert_eq!(
            trigger(
                0x52,
                Modifiers {
                    alt: true,
                    shift: true,
                    ..Modifiers::default()
                }
            )
            .as_deref(),
            Some("ALT+SHIFT+r")
        );
        assert_eq!(trigger(0x7B, Modifiers::default()).as_deref(), Some("F12"));
        assert_eq!(trigger(0xFF, modifiers), None);
    }

    #[test]
    fn cancel_suggests_escape_with_the_dictation_modifiers() {
        let config = HelperConfig {
            protocol_version: onda_input::PROTOCOL_VERSION,
            bindings: vec![
                binding(ShortcutAction::Dictation, 0x20),
                ShortcutBinding {
                    modifiers: Modifiers::default(),
                    ..binding(ShortcutAction::Cancel, 0x1B)
                },
            ],
        };
        let shortcuts = shortcuts(&config);
        assert_eq!(shortcuts[1].0, "cancel");
        assert_eq!(
            shortcuts[1].1.get("preferred_trigger"),
            Some(&Value::from("CTRL+LOGO+Escape"))
        );
    }

    fn ms(start: Instant, milliseconds: u64) -> Instant {
        start + Duration::from_millis(milliseconds)
    }

    /// Presses as GNOME reports keys held from `from` to `to`: the press, then a repeat after
    /// 500 ms and every 33 ms after that.
    fn held(
        matcher: &mut PortalMatcher,
        id: &str,
        start: Instant,
        from: u64,
        to: u64,
    ) -> Vec<(bool, ShortcutAction)> {
        let mut events = kinds(&matcher.handle(id, true, true, ms(start, from)));
        let mut at = from + 500;
        while at < to {
            events.extend(kinds(&matcher.handle(id, true, true, ms(start, at))));
            at += 33;
        }
        events
    }

    #[test]
    fn holding_with_key_repeat_records_until_the_release() {
        let mut matcher = PortalMatcher::new(vec![
            binding(ShortcutAction::Dictation, 0x20),
            binding(ShortcutAction::Transform, 0x52),
        ]);
        let start = Instant::now();
        assert_eq!(
            held(&mut matcher, "dictation", start, 0, 2000),
            [(true, ShortcutAction::Dictation)]
        );
        // Another shortcut while one is held does nothing.
        assert!(matcher
            .handle("transform", true, true, ms(start, 1500))
            .is_empty());
        assert_eq!(
            kinds(&matcher.handle("dictation", false, true, ms(start, 2000))),
            [(false, ShortcutAction::Dictation)]
        );
        // The next hold starts afresh.
        assert_eq!(
            held(&mut matcher, "dictation", start, 3000, 4000),
            [(true, ShortcutAction::Dictation)]
        );
        assert!(matcher
            .handle("unknown", true, true, ms(start, 4000))
            .is_empty());
    }

    #[test]
    fn a_tap_latches_until_the_next_press() {
        let mut matcher = PortalMatcher::new(vec![binding(ShortcutAction::Dictation, 0x20)]);
        let start = Instant::now();
        assert_eq!(matcher.handle("dictation", true, false, start).len(), 1);
        assert!(matcher
            .handle("dictation", false, true, ms(start, 50))
            .is_empty());
        // Pressed again, and held a little: the repeats don't start another recording.
        assert_eq!(
            held(&mut matcher, "dictation", start, 5000, 5800),
            [(false, ShortcutAction::Dictation)]
        );
        assert!(matcher
            .handle("dictation", false, true, ms(start, 5800))
            .is_empty());
    }

    #[test]
    fn a_release_the_desktop_never_reports_ends_at_the_next_press() {
        let mut matcher = PortalMatcher::new(vec![binding(ShortcutAction::Dictation, 0x20)]);
        let start = Instant::now();
        assert_eq!(
            held(&mut matcher, "dictation", start, 0, 2000),
            [(true, ShortcutAction::Dictation)]
        );
        assert_eq!(
            kinds(&matcher.handle("dictation", true, true, ms(start, 3500))),
            [(false, ShortcutAction::Dictation)]
        );
    }

    #[test]
    fn a_rewrite_or_hands_free_tap_never_blocks_hold_to_talk() {
        let mut matcher = PortalMatcher::new(vec![
            binding(ShortcutAction::Dictation, 0x20),
            binding(ShortcutAction::Toggle, 0x48),
            binding(ShortcutAction::Transform, 0x50),
        ]);
        let start = Instant::now();
        assert_eq!(
            kinds(&matcher.handle("transform", true, false, start)),
            [(true, ShortcutAction::Transform)]
        );
        assert!(matcher
            .handle("transform", false, true, ms(start, 80))
            .is_empty());
        assert_eq!(
            kinds(&matcher.handle("dictation", true, false, ms(start, 3000))),
            [(true, ShortcutAction::Dictation)]
        );
        assert_eq!(
            kinds(&matcher.handle("dictation", false, true, ms(start, 5000))),
            [(false, ShortcutAction::Dictation)]
        );
        // Hands-free held down toggles once, not with every repeat.
        assert_eq!(
            held(&mut matcher, "toggle", start, 6000, 7000),
            [(true, ShortcutAction::Toggle)]
        );
    }

    #[test]
    fn a_latched_recording_that_ended_elsewhere_lets_the_next_press_start() {
        let mut matcher = PortalMatcher::new(vec![binding(ShortcutAction::Dictation, 0x20)]);
        let start = Instant::now();
        assert_eq!(matcher.handle("dictation", true, false, start).len(), 1);
        assert!(matcher
            .handle("dictation", false, false, ms(start, 60))
            .is_empty());
        // Cancelled from the tray or failed: nothing is running a while later.
        assert_eq!(
            kinds(&matcher.handle("dictation", true, false, ms(start, 4000))),
            [(true, ShortcutAction::Dictation)]
        );
    }

    #[test]
    fn cancel_fires_only_during_an_operation_and_once_per_press() {
        let mut matcher = PortalMatcher::new(vec![
            binding(ShortcutAction::Dictation, 0x20),
            binding(ShortcutAction::Cancel, 0x1B),
        ]);
        let start = Instant::now();
        assert!(matcher.handle("cancel", true, false, start).is_empty());
        assert_eq!(
            held(&mut matcher, "cancel", start, 1000, 2000),
            [(true, ShortcutAction::Cancel)]
        );
        // Its release went unreported; the next press still cancels.
        assert_eq!(
            kinds(&matcher.handle("cancel", true, true, ms(start, 4000))),
            [(true, ShortcutAction::Cancel)]
        );
    }
}
