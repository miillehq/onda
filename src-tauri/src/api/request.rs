//! Requests accepted by the desktop command.
use onda_core::contracts::*;
use serde::{Deserialize, Deserializer};
use ts_rs::TS;

#[cfg_attr(feature = "wire-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize, TS)]
#[serde(
    tag = "command",
    content = "args",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ApiRequest {
    CompleteDebugSmoke {
        passed: bool,
        checks: Vec<String>,
    },
    ReplayDebugFixture,
    GetSnapshot,
    GetSettings,
    UpdateSettings {
        #[cfg_attr(feature = "wire-schema", schemars(with = "Settings"))]
        #[serde(deserialize_with = "complete_object")]
        settings: Box<Settings>,
    },
    GetDictionary,
    SaveDictionaryEntry {
        #[cfg_attr(feature = "wire-schema", schemars(with = "DictionaryEntry"))]
        #[serde(deserialize_with = "complete_object")]
        entry: DictionaryEntry,
    },
    DeleteDictionaryEntry {
        id: String,
    },
    ImportDictionary {
        json: String,
    },
    ExportDictionary,
    GetSnippets,
    SaveSnippet {
        #[cfg_attr(feature = "wire-schema", schemars(with = "Snippet"))]
        #[serde(deserialize_with = "complete_object")]
        snippet: Snippet,
    },
    DeleteSnippet {
        id: String,
    },
    GetTransforms,
    SaveTransform {
        #[cfg_attr(feature = "wire-schema", schemars(with = "Transform"))]
        #[serde(deserialize_with = "complete_object")]
        transform: Transform,
    },
    DeleteTransform {
        id: String,
    },
    ListAudioDevices,
    TestMicrophone {
        duration_ms: u32,
    },
    StartDictation {
        command_mode: bool,
    },
    StopDictation,
    CancelOperation,
    GetModelCatalog,
    GetInstalledModels,
    StartModelDownload {
        model_id: String,
    },
    CancelModelDownload {
        model_id: String,
    },
    UninstallModel {
        model_id: String,
    },
    StartChatgptSignIn {
        #[cfg_attr(feature = "wire-schema", schemars(with = "Option<String>"))]
        #[serde(deserialize_with = "Option::deserialize")]
        profile_id: Option<String>,
    },
    EnableChatgptPlanUsage {
        profile_id: String,
    },
    CancelChatgptSignIn,
    ListChatgptProfiles,
    SelectChatgptProfile {
        profile_id: String,
    },
    SignOutChatgptProfile {
        profile_id: String,
    },
    /// Forget an account on this PC, signing it out first if needed.
    RemoveChatgptProfile {
        profile_id: String,
    },
    ListAvailableModels,
    SetChatgptModel {
        profile_id: String,
        slug: String,
    },
    PreviewTransform {
        source: String,
        transform_id: String,
    },
    TransformSelection {
        transform_id: String,
    },
    RetryResult,
    RetryTransformation,
    CopyResult,
    /// Copy ChatGPT's version of the current result after Onda's checks kept the original.
    CopySuggestion,
    UndoLastTransform,
    ClearResult,
    AcceptLearning {
        proposal_id: String,
    },
    ExportDiagnostics,
    CheckUpdate,
    InstallUpdate,
    /// Content-free daily dictation counts for the home screen.
    GetUsage,
    /// Bring the main window forward, optionally at a section such as `settings/microphone`.
    OpenMainWindow {
        #[cfg_attr(feature = "wire-schema", schemars(with = "Option<String>"))]
        #[serde(deserialize_with = "Option::deserialize")]
        section: Option<String>,
    },
    /// Hide the dictation pill early, e.g. after reading an error.
    DismissPill,
    /// Acknowledge a failed operation so Onda reads as ready again. Its text stays in History.
    DismissError {
        operation_id: String,
    },
    /// Retained dictations and edits, newest first.
    GetHistory,
    CopyHistoryEntry {
        id: String,
    },
    /// Copy ChatGPT's version kept with a history entry whose polish was skipped.
    CopyHistorySuggestion {
        id: String,
    },
    DeleteHistoryEntry {
        id: String,
    },
    ClearHistory,
    /// Record one short take of a dictionary word and transcribe it as dictation would,
    /// without typing it anywhere or keeping it in history.
    TeachWord {
        duration_ms: u32,
    },
    GetProfile,
    SetProfileName {
        #[cfg_attr(feature = "wire-schema", schemars(with = "Option<String>"))]
        #[serde(deserialize_with = "Option::deserialize")]
        name: Option<String>,
    },
    /// Opens the system file picker natively, so the original file never passes through the
    /// webview; the picture is re-encoded before it is stored.
    ChooseProfilePicture,
    RemoveProfilePicture,
    /// A PNG the statistics page drew from content-free numbers; native code asks where to save it.
    SaveStatsImage {
        png_base64: String,
    },
    /// Models the configured provider offers; also the connection check for a local server.
    ListProviderModels {
        provider: AiProvider,
    },
    SaveProviderKey {
        provider: AiProvider,
        key: String,
    },
    RemoveProviderKey {
        provider: AiProvider,
    },
    /// Memory and CPU of Onda's own process tree, including its web view processes.
    GetResourceUsage,
    /// Linux: asks the desktop to let Onda press the paste shortcut in other apps.
    AllowTyping,
    /// Linux: stops typing into other apps and forgets the permission.
    StopTyping,
    /// Asks the desktop again to approve Onda's shortcuts, or restarts a stopped helper.
    RestartShortcuts,
}

impl ApiRequest {
    /// Decode the one IPC envelope before any handler can read or change application state.
    pub fn decode(received: serde_json::Value) -> onda_core::error::AppResult<Self> {
        let invalid =
            || onda_core::error::AppError::invalid("The desktop request has an invalid shape.");
        let envelope = received.as_object().ok_or_else(invalid)?;
        if envelope.get("args").is_some_and(|args| !args.is_object()) {
            return Err(invalid());
        }
        serde_json::from_value(received).map_err(|_| invalid())
    }
}

/// Small structured replacements must carry exactly the fields the renderer was given.
/// Durable records retain their independent backwards-compatible decoding rules.
fn complete_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    use serde::de::Error;
    let received = serde_json::Value::deserialize(deserializer)?;
    let value = T::deserialize(&received).map_err(D::Error::custom)?;
    if !received.is_object() || serde_json::to_value(&value).map_err(D::Error::custom)? != received
    {
        return Err(D::Error::custom("Expected a complete desktop object."));
    }
    Ok(value)
}
