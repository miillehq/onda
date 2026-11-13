//! macOS registers Onda's shortcuts as system hotkeys from accelerator strings such as
//! `Control+Super+Space`. Settings store Windows virtual-key chords on every platform, so this
//! maps a binding to the accelerator the hotkey parser accepts. Pure, so it is tested everywhere.
use crate::ShortcutBinding;

/// The accelerator for `binding`, or `None` for a key macOS hotkeys cannot register.
/// The Windows key is the Command key on a Mac (`Super`), as it is in the recorder.
pub fn accelerator(binding: &ShortcutBinding) -> Option<String> {
    let key = key_name(binding.virtual_key)?;
    let modifiers = &binding.modifiers;
    let mut parts: Vec<&str> = Vec::with_capacity(5);
    if modifiers.control {
        parts.push("Control");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.windows {
        parts.push("Super");
    }
    parts.push(&key);
    Some(parts.join("+"))
}

fn key_name(virtual_key: u16) -> Option<String> {
    let named = match virtual_key {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x2C => "PrintScreen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x6A => "NumpadMultiply",
        0x6B => "NumpadAdd",
        0x6D => "NumpadSubtract",
        0x6E => "NumpadDecimal",
        0x6F => "NumpadDivide",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xBA => "Semicolon",
        0xBB => "Equal",
        0xBC => "Comma",
        0xBD => "Minus",
        0xBE => "Period",
        0xBF => "Slash",
        0xC0 => "Backquote",
        0xDB => "BracketLeft",
        0xDC => "Backslash",
        0xDD => "BracketRight",
        0xDE => "Quote",
        0x30..=0x39 => return Some(format!("Digit{}", virtual_key - 0x30)),
        0x41..=0x5A => return Some(format!("Key{}", char::from(virtual_key as u8))),
        0x60..=0x69 => return Some(format!("Numpad{}", virtual_key - 0x60)),
        // The hotkey parser knows F1 to F15.
        0x70..=0x7E => return Some(format!("F{}", virtual_key - 0x6F)),
        _ => return None,
    };
    Some(named.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Modifiers, ShortcutAction};

    fn binding(virtual_key: u16, modifiers: Modifiers) -> ShortcutBinding {
        ShortcutBinding {
            action: ShortcutAction::Dictation,
            transform_id: None,
            virtual_key,
            modifiers,
        }
    }

    #[test]
    fn default_chords_become_command_based_accelerators() {
        let control_windows = Modifiers {
            control: true,
            windows: true,
            ..Modifiers::default()
        };
        assert_eq!(
            accelerator(&binding(0x20, control_windows)).as_deref(),
            Some("Control+Super+Space")
        );
        let control_alt = Modifiers {
            control: true,
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(
            accelerator(&binding(0x50, control_alt)).as_deref(),
            Some("Control+Alt+KeyP")
        );
        assert_eq!(
            accelerator(&binding(0x1B, Modifiers::default())).as_deref(),
            Some("Escape")
        );
    }

    #[test]
    fn keys_map_by_their_windows_code_and_unknown_keys_are_refused() {
        let none = Modifiers::default();
        let names: Vec<_> = [0x37, 0x5A, 0x63, 0x70, 0x7E, 0xBF, 0xDE, 0x25]
            .into_iter()
            .map(|key| accelerator(&binding(key, none)))
            .collect();
        assert_eq!(
            names,
            [
                "Digit7",
                "KeyZ",
                "Numpad3",
                "F1",
                "F15",
                "Slash",
                "Quote",
                "ArrowLeft"
            ]
            .map(|name| Some(name.to_string()))
        );
        // F16 and beyond, modifiers alone and unassigned codes have no hotkey name.
        for key in [0x7F, 0x87, 0xA2, 0x5B, 0xFF] {
            assert_eq!(accelerator(&binding(key, none)), None, "{key:#x}");
        }
    }

    #[test]
    fn every_name_produced_is_accepted_by_the_hotkey_parser() {
        let all = Modifiers {
            control: true,
            shift: true,
            alt: true,
            windows: true,
        };
        let mut checked = 0;
        for key in 0..=u16::from(u8::MAX) {
            if let Some(text) = accelerator(&binding(key, all)) {
                let parsed: Result<global_hotkey::hotkey::HotKey, _> = text.parse();
                assert!(
                    parsed.is_ok(),
                    "{text} ({key:#x}) does not parse: {parsed:?}"
                );
                checked += 1;
            }
        }
        assert!(checked > 90, "only {checked} keys produced names");
    }
}
