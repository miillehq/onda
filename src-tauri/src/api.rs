//! Versioned IPC facade. Renderer capabilities never include tokens, arbitrary files or HTTP.
use onda_core::contracts::*;
use onda_core::error::AppError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
        settings: Settings,
    },
    GetDictionary,
    SaveDictionaryEntry {
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
        snippet: Snippet,
    },
    DeleteSnippet {
        id: String,
    },
    GetTransforms,
    SaveTransform {
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
}

/// What the local model heard for one spoken take of a word.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TeachTake {
    /// The transcript, trimmed of surrounding punctuation.
    pub heard: String,
    /// The same text after the current dictionary and snippets are applied.
    pub written: String,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub download_size_bytes: f64,
    pub memory_bytes: f64,
    pub license: String,
    pub english_only: bool,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelInstallation {
    pub model_id: String,
    pub version: String,
    pub size_bytes: f64,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    pub label: String,
    pub email: Option<String>,
    pub signed_in: bool,
    pub plan_usage_enabled: bool,
    pub active: bool,
    pub preferred_model: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudModel {
    pub slug: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneReport {
    pub duration_ms: f64,
    pub peak: f64,
    pub rms: f64,
    pub clipped_samples: f64,
    pub discontinuities: f64,
    pub mostly_silent: bool,
    pub source_sample_rate: u32,
    pub source_channels: u16,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor_id: u32,
    pub dedicated_memory_bytes: f64,
    pub shared_memory_bytes: f64,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HardwareReport {
    pub logical_cpus: usize,
    pub physical_cpus: usize,
    pub total_memory_bytes: f64,
    pub available_memory_bytes: f64,
    pub inference_threads: usize,
    pub simd: Vec<String>,
    pub gpus: Vec<GpuInfo>,
    pub vulkan_loader_present: bool,
    pub cuda_driver_present: bool,
    pub directml_library_present: bool,
    pub vulkan_worker_installed: bool,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadReport {
    pub model_id: String,
    pub stage: String,
    pub completed_bytes: f64,
    pub total_bytes: f64,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub app_name: String,
    pub version: String,
    pub contract_version: u32,
    pub state: OperationState,
    pub settings: Settings,
    /// Native profile resolution; downloaded assets do not implicitly become selected.
    pub selected_model_id: Option<String>,
    pub result: Option<OperationResult>,
    pub hardware: HardwareReport,
    pub installed_models: Vec<ModelInstallation>,
    pub profiles: Vec<AccountInfo>,
    pub cloud_error: Option<AppError>,
    pub shortcut_status: String,
    pub updater_configured: bool,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    /// The local calendar day used for recording, so the UI never guesses time zones.
    pub today: String,
    /// Most recent first.
    pub days: Vec<DailyUsage>,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Serialize, TS)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApiResponse {
    Unit,
    Snapshot(Box<Snapshot>),
    Settings(Settings),
    Dictionary(Vec<DictionaryEntry>),
    Snippets(Vec<Snippet>),
    Transforms(Vec<Transform>),
    Text(String),
    Count(usize),
    AudioDevices(Vec<AudioDevice>),
    Microphone(MicrophoneReport),
    ModelCatalog(Vec<ModelInfo>),
    InstalledModels(Vec<ModelInstallation>),
    Profiles(Vec<AccountInfo>),
    CloudModels(Vec<CloudModel>),
    Result(OperationResult),
    Update(UpdateInfo),
    SignOut { remote_revocation_confirmed: bool },
    Usage(UsageReport),
    History(Vec<HistoryEntry>),
    TeachTake(TeachTake),
}

#[derive(Clone, Serialize, TS)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeEvent {
    Domain(AppEvent),
    DownloadProgress(DownloadReport),
    OauthChanged {
        status: String,
    },
    ShortcutStatus {
        status: String,
    },
    /// Short-window input RMS (0–1) about 20 times a second while the microphone is open.
    AudioLevel {
        level: f32,
    },
    /// Ask the main window to show a section, e.g. after a pill action.
    Navigate {
        section: String,
    },
    /// One history entry was added or changed.
    HistoryUpdated {
        entry: HistoryEntry,
    },
    /// Entries were removed or retention changed; fetch the list again.
    HistoryReset,
}

pub fn export_contracts(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/generated/api.ts");
    let declarations = [
        ApiRequest::decl(&ts_rs::Config::default()),
        ModelInfo::decl(&ts_rs::Config::default()),
        ModelInstallation::decl(&ts_rs::Config::default()),
        AccountInfo::decl(&ts_rs::Config::default()),
        CloudModel::decl(&ts_rs::Config::default()),
        AudioDevice::decl(&ts_rs::Config::default()),
        MicrophoneReport::decl(&ts_rs::Config::default()),
        GpuInfo::decl(&ts_rs::Config::default()),
        HardwareReport::decl(&ts_rs::Config::default()),
        DownloadReport::decl(&ts_rs::Config::default()),
        Snapshot::decl(&ts_rs::Config::default()),
        UsageReport::decl(&ts_rs::Config::default()),
        TeachTake::decl(&ts_rs::Config::default()),
        UpdateInfo::decl(&ts_rs::Config::default()),
        ApiResponse::decl(&ts_rs::Config::default()),
        RuntimeEvent::decl(&ts_rs::Config::default()),
    ];
    let imports="import type { AppError, Settings, DictionaryEntry, Snippet, Transform, OperationState, OperationResult, AppEvent, DailyUsage, HistoryEntry } from './core';";
    let content = format!(
        "// Generated by onda-api-contracts. Do not edit.\n{imports}\n{}\n",
        declarations
            .iter()
            .map(|value| format!("export {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if check {
        if std::fs::read_to_string(path)? != content {
            return Err("IPC contracts drifted; run npm run contracts.".into());
        }
    } else {
        std::fs::create_dir_all(path.parent().ok_or("Missing contract parent")?)?;
        std::fs::write(path, content)?;
    }
    Ok(())
}

impl From<onda_cloud::AccountProfile> for AccountInfo {
    fn from(value: onda_cloud::AccountProfile) -> Self {
        Self {
            id: value.id,
            label: value.label,
            email: value.email,
            signed_in: value.signed_in,
            plan_usage_enabled: value.plan_usage_enabled,
            active: value.active,
            preferred_model: value.preferred_model,
        }
    }
}
impl From<onda_asr::InstalledModel> for ModelInstallation {
    fn from(value: onda_asr::InstalledModel) -> Self {
        Self {
            model_id: value.model_id,
            version: value.version,
            size_bytes: value.size_bytes as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ipc_uses_camel_case_args_and_rejects_unknown_fields() {
        let request: ApiRequest = serde_json::from_value(
            serde_json::json!({"command":"startDictation","args":{"commandMode":true}}),
        )
        .unwrap();
        assert!(matches!(
            request,
            ApiRequest::StartDictation { command_mode: true }
        ));
        assert!(serde_json::from_value::<ApiRequest>(serde_json::json!({"command":"startDictation","args":{"commandMode":true,"unexpected":true}})).is_err());
    }
    #[test]
    fn public_account_dto_contains_no_credentials() {
        let profile = AccountInfo {
            id: "safe-id".into(),
            label: "account".into(),
            email: None,
            signed_in: false,
            plan_usage_enabled: false,
            active: false,
            preferred_model: None,
        };
        let json = serde_json::to_string(&profile).unwrap();
        for secret_field in [
            "access_token",
            "refresh_token",
            "id_token",
            "code_verifier",
            "authorization",
        ] {
            assert!(!json.contains(secret_field));
        }
    }
}
