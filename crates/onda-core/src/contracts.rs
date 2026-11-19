mod settings;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const CONTRACT_VERSION: u32 = 1;
pub const DEFAULT_MAX_RECORDING_SECONDS: u32 = 180;
pub const MAX_RECORDING_SECONDS: u32 = 600;
pub const DEFAULT_HISTORY_DAYS: u32 = 3;
pub const MAX_HISTORY_DAYS: u32 = 30;
pub const MAX_PROVIDER_SETTING_BYTES: usize = 2048;

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionPreference {
    Auto,
    CpuOnly,
    GpuPreferred,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QualityProfile {
    Fast,
    Balanced,
    BestQuality,
    Custom,
}

/// Window appearance; `System` follows the Windows app theme.
#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

/// Colour of the notification-area (tray) mark. `Auto` picks white on a dark taskbar and black on
/// a light one; the fixed choices are for taskbars Onda cannot read, such as Linux panels.
#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrayIconStyle {
    Auto,
    Black,
    White,
}

/// Where AI polish, rewrites and spoken edits send text. Only the chosen provider is ever used:
/// a failure keeps the local text and never retries with another provider.
#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AiProvider {
    /// Sign in with ChatGPT, using the account's plan.
    Chatgpt,
    /// A user-configured server that speaks the OpenAI Chat Completions API.
    OpenaiCompatible,
    /// Anthropic's Messages API with the user's own API key.
    Anthropic,
    /// An OpenAI-compatible server on this computer (Ollama, LM Studio, llama.cpp); loopback only.
    Local,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PasteShortcut {
    ControlV,
    ControlShiftV,
    ShiftInsert,
}

/// Virtual-key chord interpreted only by the native shortcut process.
#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub keys: Vec<u16>,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppProfile {
    pub executable: String,
    pub context_enabled: bool,
    pub enhancement_enabled: bool,
    pub cleanup_instruction: Option<String>,
    pub paste_shortcut: PasteShortcut,
}

/// Durable preferences contain no tokens, transcripts or microphone recordings.
#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub execution_preference: ExecutionPreference,
    pub quality_profile: QualityProfile,
    pub model_id: Option<String>,
    pub model_directory: Option<String>,
    pub language: Option<String>,
    pub microphone_id: Option<String>,
    pub max_recording_seconds: u32,
    pub enhancement_enabled: bool,
    pub context_enabled: bool,
    pub learning_enabled: bool,
    pub push_to_talk: Shortcut,
    pub transform_shortcut: Shortcut,
    pub command_shortcut: Shortcut,
    pub toggle_shortcut: Shortcut,
    pub toggle_enabled: bool,
    pub default_transform_id: String,
    pub app_profiles: Vec<AppProfile>,
    pub theme: ThemePreference,
    /// Registered under the current user's Run key with `--background`.
    pub launch_at_login: bool,
    /// The non-activating dictation indicator shown above the taskbar.
    pub show_pill: bool,
    pub play_sounds: bool,
    pub onboarding_completed: bool,
    /// Days to keep dictation history, encrypted to the Windows user. 0 keeps it for this
    /// session only, in memory.
    pub history_days: u32,
    pub tray_icon: TrayIconStyle,
    pub ai_provider: AiProvider,
    /// Base URL of the OpenAI-compatible server, e.g. `https://api.example.com/v1`. API keys are
    /// kept in protected storage, never here.
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub anthropic_model: Option<String>,
    /// Base URL of the local server; must be a loopback address.
    pub local_base_url: Option<String>,
    pub local_model: Option<String>,
}

/// What a fresh profile holds to dictate. Settings store Windows virtual keys on every platform:
/// Ctrl+Win+Space on Windows, Ctrl+Super+Space on Linux. A Mac reads the Windows key as Command,
/// and Control+Command+Space opens the Emoji & Symbols viewer, so macOS starts with
/// Control+Shift+Space. macOS and the usual launchers leave it free: Option+Space belongs to
/// Raycast, Alfred and ChatGPT, and Control+Option+Space switches input sources.
pub fn default_push_to_talk(macos: bool) -> Shortcut {
    Shortcut {
        keys: if macos {
            vec![0xA2, 0xA0, 0x20]
        } else {
            vec![0xA2, 0x5B, 0x20]
        },
    }
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntrySource {
    Manual,
    AutoLearned,
    Imported,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DictionaryKind {
    Vocabulary,
    Replacement,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictionaryEntry {
    pub id: String,
    pub kind: DictionaryKind,
    pub phrase: String,
    pub replacement: Option<String>,
    pub enabled: bool,
    pub starred: bool,
    pub source: EntrySource,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub trigger: String,
    pub expansion: String,
    pub enabled: bool,
}

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub id: String,
    pub name: String,
    pub instruction: String,
    pub shortcut: Option<Shortcut>,
    pub model_preference: Option<String>,
    pub preserve_protected_tokens: bool,
    pub built_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Recording,
    FinalizingAudio,
    Transcribing,
    Enhancing,
    Inserting,
    Completed,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationMode {
    Dictation,
    SpokenCommand,
    Transform,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DataRoute {
    Local,
    ChatgptPlan,
    /// Sent to the user's OpenAI-compatible endpoint.
    OpenaiCompatible,
    Anthropic,
    /// Processed by a model server on this computer; nothing leaves the PC.
    LocalModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OperationState {
    pub operation_id: Option<String>,
    pub phase: Phase,
    pub mode: OperationMode,
    pub route: DataRoute,
    pub error: Option<AppError>,
    pub fallback_used: bool,
    /// Started by the toggle shortcut, so recording continues until it is pressed again.
    #[serde(default)]
    pub hands_free: bool,
}

/// One dictation or edit, kept so it can be found again after the next one starts.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    /// Unix milliseconds.
    pub created_at: f64,
    pub mode: OperationMode,
    pub route: DataRoute,
    /// The local transcript (or, for edits, the selected text).
    pub original: String,
    pub output: String,
    pub command_text: Option<String>,
    pub inserted: bool,
    pub fallback_used: bool,
    /// Set when the operation failed after producing text, e.g. it could not be typed.
    pub error_code: Option<crate::error::ErrorCode>,
    /// Executable of the app it was typed into, e.g. "slack.exe".
    pub app: Option<String>,
    /// The AI provider's version when it failed Onda's checks and `output` kept the original.
    #[serde(default)]
    pub suggestion: Option<String>,
}

/// Content-free daily aggregate keyed by the local calendar day (YYYY-MM-DD).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub day: String,
    pub dictations: u32,
    pub words: u32,
    pub audio_ms: u32,
}

/// Lifetime totals, statistics and streaks derived only from content-free daily counts.
/// Days are the local calendar day when each dictation was typed.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// Dictations typed since `since`.
    pub dictations: u32,
    pub words: u32,
    pub audio_ms: f64,
    /// First day Onda counted, or null before the first dictation.
    pub since: Option<String>,
    /// Words per minute of recorded audio; null until there is a minute of audio.
    pub words_per_minute: Option<f64>,
    /// Consecutive days with a dictation, ending today, or yesterday if today has none yet.
    pub current_streak: u32,
    pub longest_streak: u32,
    pub active_days: u32,
}

/// The local profile shown in greetings and, only if the user chooses, on shared images.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: Option<String>,
    /// Base64 PNG, 256×256, re-encoded by Onda so no original file metadata survives.
    pub picture: Option<String>,
}

/// Result retained only in memory for recovery/copy/retry; no history database.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub command_text: Option<String>,
    pub operation_id: String,
    pub original: String,
    pub output: String,
    pub inserted: bool,
    pub fallback_used: bool,
    pub diff: Vec<DiffSegment>,
    /// The AI provider's version when it failed Onda's checks, so it can still be copied.
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiffKind {
    Equal,
    Removed,
    Added,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DiffSegment {
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LearningProposal {
    pub id: String,
    pub original: String,
    pub corrected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SpanMetric {
    pub operation_id: String,
    pub subsystem: String,
    pub duration_ms: f64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AppEvent {
    StateChanged(OperationState),
    ResultReady(OperationResult),
    LearningProposed(LearningProposal),
    Metric(SpanMetric),
    Error(AppError),
    CloudDelta { operation_id: String, delta: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_dictates_with_a_chord_macos_leaves_free() {
        // Ctrl+Win+Space on a Mac is Control+Command+Space, which opens Emoji & Symbols.
        let elsewhere = Settings::defaults_for(false);
        assert_eq!(elsewhere.push_to_talk.keys, [0xA2, 0x5B, 0x20]);
        let mac = Settings::defaults_for(true);
        assert_eq!(
            mac.push_to_talk.keys,
            [0xA2, 0xA0, 0x20],
            "Control+Shift+Space"
        );
        mac.validate().unwrap();
        // Only the dictation chord differs.
        let mac_with_the_other_chord = Settings {
            push_to_talk: elsewhere.push_to_talk.clone(),
            ..mac
        };
        assert_eq!(mac_with_the_other_chord, elsewhere);
        // A fresh profile starts with the defaults of the platform it runs on.
        assert_eq!(
            Settings::default(),
            Settings::defaults_for(cfg!(target_os = "macos"))
        );
    }
}
