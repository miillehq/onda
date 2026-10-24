use crate::{HelperConfig, InputError, Modifiers, ShortcutAction, ShortcutBinding};

pub(crate) fn is_modifier(key: u16) -> bool {
    matches!(key, 0x10..=0x12 | 0x5b..=0x5c | 0xa0..=0xa5)
}

/// Fixed key-state array prevents allocations and hash lookups inside the keyboard hook.
pub struct ShortcutMatcher {
    bindings: Vec<ShortcutBinding>,
    pressed_keys: [bool; 256],
    active: Option<(u16, usize)>,
    cancel_latch: Option<(u16, usize)>,
    operation_active: bool,
    desktop_active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchResult {
    pub suppress: bool,
    pub event: Option<(ShortcutAction, bool)>,
    pub binding_index: Option<usize>,
}

impl ShortcutMatcher {
    pub fn new(config: HelperConfig) -> Result<Self, InputError> {
        config.validate()?;
        Ok(Self {
            bindings: config.bindings,
            pressed_keys: [false; 256],
            active: None,
            cancel_latch: None,
            operation_active: false,
            desktop_active: true,
        })
    }

    /// Initialize modifier state when a hook is installed while keys are held.
    pub fn seed_key(&mut self, key: u16, pressed: bool) {
        if let Some(state) = self.pressed_keys.get_mut(key as usize) {
            *state = pressed;
        }
    }

    pub fn active_key(&self) -> Option<u16> {
        self.active.map(|(key, _)| key)
    }

    /// Forget stale state after desktop/session changes; caller emits one Reset event.
    pub fn reset(&mut self) {
        self.pressed_keys.fill(false);
        self.active = None;
        self.cancel_latch = None;
        self.operation_active = false;
    }

    /// Rearm only after both the operation and every consumed trigger have ended.
    pub fn is_idle_for_rearm(&self) -> bool {
        self.desktop_active
            && !self.operation_active
            && self.active.is_none()
            && self.cancel_latch.is_none()
    }

    /// Whether this helper's desktop currently accepts input, independent of key state.
    pub fn desktop_active(&self) -> bool {
        self.desktop_active
    }

    /// Disarm new shortcuts on desktop loss, retaining consumed keys until genuine key-up.
    /// Returns true once per transition away so the caller can cancel the parent's operation.
    pub fn set_desktop_active(&mut self, active: bool) -> bool {
        let lost = self.desktop_active && !active;
        self.desktop_active = active;
        if lost {
            self.operation_active = false;
            let trigger = self.active.map(|(key, _)| key);
            let cancel = self.cancel_latch.map(|(key, _)| key);
            self.pressed_keys.fill(false);
            for key in trigger.into_iter().chain(cancel) {
                self.pressed_keys[key as usize] = true;
            }
        }
        lost
    }

    /// Arm cancellation only while the parent has an operation that can be cancelled.
    pub fn set_operation_active(&mut self, active: bool) {
        self.operation_active = active && self.desktop_active;
    }
    pub fn operation_active(&self) -> bool {
        self.operation_active
    }

    // Time O(b), b ≤ 32 configured bindings; space O(1), no callback allocations.
    pub fn handle(&mut self, key: u16, down: bool, injected: bool) -> MatchResult {
        let pass = MatchResult {
            suppress: false,
            event: None,
            binding_index: None,
        };
        if injected || key > 255 {
            return pass;
        }
        let repeated = self.pressed_keys[key as usize] == down;
        self.pressed_keys[key as usize] = down;
        if let Some((cancel_key, binding_index)) = self.cancel_latch {
            if key == cancel_key {
                if !down {
                    self.cancel_latch = None;
                }
                return MatchResult {
                    suppress: true,
                    event: if down {
                        None
                    } else {
                        Some((ShortcutAction::Cancel, false))
                    },
                    binding_index: if down { None } else { Some(binding_index) },
                };
            }
        }
        if self.operation_active && down && !repeated {
            let modifiers = self.modifiers();
            if let Some((binding_index, _)) =
                self.bindings.iter().enumerate().find(|(_, binding)| {
                    binding.action == ShortcutAction::Cancel
                        && binding.virtual_key == key
                        && (binding.modifiers == Modifiers::default()
                            || binding.modifiers == modifiers)
                })
            {
                self.cancel_latch = Some((key, binding_index));
                return MatchResult {
                    suppress: true,
                    event: Some((ShortcutAction::Cancel, true)),
                    binding_index: Some(binding_index),
                };
            }
        }
        if let Some((active_key, binding_index)) = self.active {
            let action = self.bindings[binding_index].action;
            if key != active_key {
                return pass;
            }
            if down {
                return MatchResult {
                    suppress: true,
                    event: None,
                    binding_index: None,
                };
            }
            self.active = None;
            return MatchResult {
                suppress: true,
                event: Some((action, false)),
                binding_index: Some(binding_index),
            };
        }
        if !self.desktop_active || !down || repeated {
            return pass;
        }
        let modifiers = self.modifiers();
        let Some((binding_index, binding)) =
            self.bindings.iter().enumerate().find(|(_, binding)| {
                binding.action != ShortcutAction::Cancel
                    && binding.virtual_key == key
                    && binding.modifiers == modifiers
            })
        else {
            return pass;
        };
        let action = binding.action;
        self.active = Some((key, binding_index));
        MatchResult {
            suppress: true,
            event: Some((action, true)),
            binding_index: Some(binding_index),
        }
    }

    fn modifiers(&self) -> Modifiers {
        let any = |keys: &[usize]| keys.iter().any(|&key| self.pressed_keys[key]);
        Modifiers {
            control: any(&[0x11, 0xa2, 0xa3]),
            shift: any(&[0x10, 0xa0, 0xa1]),
            alt: any(&[0x12, 0xa4, 0xa5]),
            windows: any(&[0x5b, 0x5c]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeats_do_not_restart_and_modifier_release_does_not_lose_key_up() {
        let mut matcher = ShortcutMatcher::new(HelperConfig::default()).unwrap();
        matcher.handle(0xa2, true, false);
        matcher.handle(0x5b, true, false);
        assert_eq!(
            matcher.handle(0x20, true, false).event,
            Some((ShortcutAction::Dictation, true))
        );
        assert_eq!(matcher.handle(0x20, true, false).event, None);
        matcher.handle(0xa2, false, false);
        assert_eq!(
            matcher.handle(0x20, false, false).event,
            Some((ShortcutAction::Dictation, false))
        );
    }
    #[test]
    fn injected_keys_cannot_activate_and_reset_clears_latch() {
        let mut matcher = ShortcutMatcher::new(HelperConfig::default()).unwrap();
        matcher.handle(0xa2, true, true);
        matcher.handle(0x5b, true, true);
        assert!(!matcher.handle(0x20, true, true).suppress);
        assert!(matcher.active_key().is_none());
        matcher.seed_key(0xa2, true);
        matcher.seed_key(0x5b, true);
        matcher.handle(0x20, true, false);
        matcher.reset();
        assert!(matcher.active_key().is_none());
    }
    #[test]
    fn extra_modifier_and_duplicate_chords_are_rejected() {
        let mut config = HelperConfig::default();
        config.bindings.push(config.bindings[0].clone());
        assert!(config.validate().is_err());
        let mut matcher = ShortcutMatcher::new(HelperConfig::default()).unwrap();
        for key in [0xa2, 0x5b, 0xa0] {
            matcher.handle(key, true, false);
        }
        assert!(!matcher.handle(0x20, true, false).suppress);
    }
    #[test]
    fn custom_transforms_route_unique_binding_without_callback_allocation() {
        let config = HelperConfig {
            protocol_version: crate::PROTOCOL_VERSION,
            bindings: vec![
                ShortcutBinding {
                    action: ShortcutAction::Transform,
                    transform_id: Some("formal".into()),
                    virtual_key: 0x31,
                    modifiers: Modifiers::default(),
                },
                ShortcutBinding {
                    action: ShortcutAction::Transform,
                    transform_id: Some("concise".into()),
                    virtual_key: 0x32,
                    modifiers: Modifiers::default(),
                },
            ],
        };
        let mut matcher = ShortcutMatcher::new(config).unwrap();
        let matched = matcher.handle(0x32, true, false);
        assert_eq!(matched.binding_index, Some(1));
        assert_eq!(matched.event, Some((ShortcutAction::Transform, true)));
        assert_eq!(matcher.handle(0x32, false, false).binding_index, Some(1));
    }
    #[test]
    fn idle_escape_passes_but_active_cancel_works_while_trigger_is_held() {
        let mut config = HelperConfig::default();
        config.bindings.push(ShortcutBinding {
            action: ShortcutAction::Cancel,
            transform_id: None,
            virtual_key: 0x1b,
            modifiers: Modifiers::default(),
        });
        let mut matcher = ShortcutMatcher::new(config).unwrap();
        assert!(!matcher.handle(0x1b, true, false).suppress);
        matcher.handle(0x1b, false, false);
        matcher.set_operation_active(true);
        matcher.handle(0xa2, true, false);
        matcher.handle(0x5b, true, false);
        matcher.handle(0x20, true, false);
        assert_eq!(
            matcher.handle(0x1b, true, false).event,
            Some((ShortcutAction::Cancel, true))
        );
        matcher.set_operation_active(false);
        assert!(matcher.handle(0x1b, false, false).suppress);
        assert_eq!(
            matcher.handle(0x20, false, false).event,
            Some((ShortcutAction::Dictation, false))
        );
        matcher.reset();
        assert!(!matcher.operation_active());
    }

    #[test]
    fn held_trigger_survives_heartbeat_and_parent_state_changes_until_single_release() {
        let mut matcher = ShortcutMatcher::new(HelperConfig::default()).unwrap();
        matcher.handle(0xa2, true, false);
        matcher.handle(0x5b, true, false);
        assert_eq!(
            matcher.handle(0x20, true, false).event,
            Some((ShortcutAction::Dictation, true))
        );
        // A consumed Space can appear up to GetAsyncKeyState throughout this hold.
        // Heartbeats therefore use desktop/operation metadata and never reconcile that key.
        for operation_active in [true, false, true, false].into_iter().cycle().take(240) {
            matcher.set_operation_active(operation_active);
            assert!(!matcher.set_desktop_active(true));
            assert!(!matcher.is_idle_for_rearm());
            let repeated = matcher.handle(0x20, true, false);
            assert!(repeated.suppress);
            assert_eq!(repeated.event, None);
        }
        assert_eq!(
            matcher.handle(0x20, false, false).event,
            Some((ShortcutAction::Dictation, false))
        );
        assert_eq!(matcher.handle(0x20, false, false).event, None);
        assert!(matcher.is_idle_for_rearm());
    }

    #[test]
    fn released_trigger_does_not_allow_rearm_during_inference_or_a_held_cancel_key() {
        let mut config = HelperConfig::default();
        config.bindings.push(ShortcutBinding {
            action: ShortcutAction::Cancel,
            transform_id: None,
            virtual_key: 0x1b,
            modifiers: Modifiers::default(),
        });
        let mut matcher = ShortcutMatcher::new(config).unwrap();
        matcher.set_operation_active(true);
        matcher.seed_key(0xa2, true);
        matcher.seed_key(0x5b, true);
        matcher.handle(0x20, true, false);
        matcher.handle(0x20, false, false);
        assert!(!matcher.is_idle_for_rearm());
        assert_eq!(
            matcher.handle(0x1b, true, false).event,
            Some((ShortcutAction::Cancel, true))
        );
        matcher.set_operation_active(false);
        assert!(!matcher.is_idle_for_rearm());
        matcher.handle(0x1b, false, false);
        assert!(matcher.is_idle_for_rearm());
    }

    #[test]
    fn escape_cancellation_cannot_restart_a_still_held_push_to_talk_trigger() {
        let mut config = HelperConfig::default();
        config.bindings.push(ShortcutBinding {
            action: ShortcutAction::Cancel,
            transform_id: None,
            virtual_key: 0x1b,
            modifiers: Modifiers::default(),
        });
        let mut matcher = ShortcutMatcher::new(config).unwrap();
        matcher.seed_key(0xa2, true);
        matcher.seed_key(0x5b, true);
        matcher.handle(0x20, true, false);
        matcher.set_operation_active(true);
        matcher.handle(0x1b, true, false);
        matcher.set_operation_active(false);
        matcher.handle(0x1b, false, false);
        for _ in 0..16 {
            assert_eq!(matcher.handle(0x20, true, false).event, None);
            assert!(!matcher.is_idle_for_rearm());
        }
        assert_eq!(
            matcher.handle(0x20, false, false).event,
            Some((ShortcutAction::Dictation, false))
        );
        assert_eq!(
            matcher.handle(0x20, true, false).event,
            Some((ShortcutAction::Dictation, true))
        );
    }

    #[test]
    fn desktop_loss_cancels_once_and_quarantines_a_held_trigger_until_release() {
        let mut matcher = ShortcutMatcher::new(HelperConfig::default()).unwrap();
        matcher.seed_key(0xa2, true);
        matcher.seed_key(0x5b, true);
        matcher.handle(0x20, true, false);
        matcher.set_operation_active(true);
        assert!(matcher.set_desktop_active(false));
        assert!(!matcher.set_desktop_active(false));
        assert!(!matcher.operation_active());
        matcher.set_operation_active(true);
        assert!(!matcher.operation_active());
        assert_eq!(matcher.handle(0x20, true, false).event, None);
        assert!(!matcher.set_desktop_active(true));
        matcher.seed_key(0xa2, true);
        matcher.seed_key(0x5b, true);
        assert_eq!(matcher.handle(0x20, true, false).event, None);
        assert!(!matcher.is_idle_for_rearm());
        assert_eq!(
            matcher.handle(0x20, false, false).event,
            Some((ShortcutAction::Dictation, false))
        );
        assert!(matcher.is_idle_for_rearm());
        assert_eq!(
            matcher.handle(0x20, true, false).event,
            Some((ShortcutAction::Dictation, true))
        );
    }

    #[test]
    fn inactive_desktop_rejects_new_chords_and_retains_cancel_release() {
        let mut config = HelperConfig::default();
        config.bindings.push(ShortcutBinding {
            action: ShortcutAction::Cancel,
            transform_id: None,
            virtual_key: 0x1b,
            modifiers: Modifiers::default(),
        });
        let mut matcher = ShortcutMatcher::new(config).unwrap();
        matcher.set_operation_active(true);
        matcher.handle(0x1b, true, false);
        assert!(matcher.set_desktop_active(false));
        matcher.seed_key(0xa2, true);
        matcher.seed_key(0x5b, true);
        assert_eq!(matcher.handle(0x20, true, false).event, None);
        assert!(!matcher.is_idle_for_rearm());
        matcher.set_desktop_active(true);
        assert_eq!(matcher.handle(0x1b, true, false).event, None);
        let released = matcher.handle(0x1b, false, false);
        assert!(released.suppress);
        assert_eq!(released.event, Some((ShortcutAction::Cancel, false)));
    }
}
