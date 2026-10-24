use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable error classes. Messages must never contain user text or credentials.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Busy,
    Cancelled,
    InvalidInput,
    Storage,
    NoMicrophone,
    NoSpeech,
    SelectionRequired,
    AudioDisconnected,
    AudioRouteChanged,
    AudioPermissionDenied,
    AudioBackend,
    AudioLimit,
    NoModel,
    CorruptModel,
    Download,
    InsufficientDisk,
    Inference,
    InferenceTimeout,
    Offline,
    Unauthorized,
    Quota,
    Cloud,
    InvalidEnhancement,
    FocusChanged,
    TargetExited,
    PasswordField,
    ElevatedTarget,
    UnsupportedControl,
    ClipboardBusy,
    ClipboardUnsupported,
    Insertion,
    Shortcut,
    Internal,
}

/// Serializable application error with a recovery hint, never an OS/HTTP body.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub fn invalid(message: &str) -> Self {
        Self::new(ErrorCode::InvalidInput, message, false)
    }

    pub fn storage() -> Self {
        Self::new(
            ErrorCode::Storage,
            "Local data could not be read or saved.",
            true,
        )
    }
}

pub type AppResult<T> = Result<T, AppError>;
