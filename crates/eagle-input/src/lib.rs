//! Bounded shortcut protocol and restartable supervisor for the isolated hook process.

mod matcher;
mod supervisor;

pub use matcher::ShortcutMatcher;
use serde::{Deserialize, Serialize};
pub use supervisor::HelperSupervisor;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PROTOCOL_LINE_BYTES: usize = 16 * 1024;
pub const MAX_SHORTCUT_BINDINGS: usize = 32;
pub const EVENT_QUEUE_CAPACITY: usize = 128;
pub const HEARTBEAT_INTERVAL_MS: u32 = 500;

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("invalid shortcut configuration: {0}")]
    Configuration(String),
    #[error("shortcut helper I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("shortcut helper protocol failed: {0}")]
    Protocol(String),
    #[error("shortcut helper failed: {0}")]
    Helper(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
    Dictation,
    Transform,
    Command,
    Toggle,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub windows: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShortcutBinding {
    pub action: ShortcutAction,
    #[serde(default)]
    pub transform_id: Option<String>,
    /// Windows virtual key code; names are resolved by the application settings layer.
    pub virtual_key: u16,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelperConfig {
    pub protocol_version: u32,
    pub bindings: Vec<ShortcutBinding>,
}

impl Default for HelperConfig {
    fn default() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            bindings: vec![ShortcutBinding {
                action: ShortcutAction::Dictation,
                transform_id: None,
                virtual_key: 0x20,
                modifiers: Modifiers {
                    control: true,
                    windows: true,
                    ..Modifiers::default()
                },
            }],
        }
    }
}

impl HelperConfig {
    /// Reject ambiguous, oversized, or modifier-only shortcuts before installing a hook.
    pub fn validate(&self) -> Result<(), InputError> {
        if self.protocol_version != PROTOCOL_VERSION
            || self.bindings.is_empty()
            || self.bindings.len() > MAX_SHORTCUT_BINDINGS
        {
            return Err(InputError::Configuration(
                "expected protocol v1 and 1–32 bindings".into(),
            ));
        }
        for (index, binding) in self.bindings.iter().enumerate() {
            if !(1..=255).contains(&binding.virtual_key)
                || matcher::is_modifier(binding.virtual_key)
            {
                return Err(InputError::Configuration(
                    "shortcut requires a non-modifier virtual key".into(),
                ));
            }
            if binding.transform_id.as_ref().is_some_and(|id| {
                binding.action != ShortcutAction::Transform || id.is_empty() || id.len() > 128
            }) {
                return Err(InputError::Configuration(
                    "only transforms accept a nonempty identifier of at most 128 bytes".into(),
                ));
            }
            if self.bindings[..index].iter().any(|other| {
                (other.action == binding.action
                    && (binding.action != ShortcutAction::Transform
                        || other.transform_id == binding.transform_id))
                    || (other.virtual_key == binding.virtual_key
                        && other.modifiers == binding.modifiers)
            }) {
                return Err(InputError::Configuration(
                    "duplicate action or chord".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum HelperEvent {
    Ready {
        protocol_version: u32,
    },
    Heartbeat {
        uptime_ms: u64,
    },
    Pressed {
        action: ShortcutAction,
        #[serde(default)]
        transform_id: Option<String>,
    },
    Released {
        action: ShortcutAction,
        #[serde(default)]
        transform_id: Option<String>,
    },
    /// Reset application recording state after helper replacement or lost key state.
    Reset {
        reason: String,
    },
    Error {
        code: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase", deny_unknown_fields)]
pub enum HelperCommand {
    SetOperationActive { active: bool },
    Shutdown,
}
