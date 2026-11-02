//! Native-only Sign in with ChatGPT and plan-backed text inference, plus user-configured
//! providers (OpenAI-compatible servers, Anthropic, and local model servers).
//!
//! OAuth credentials and API keys never leave this crate. Public types deliberately expose
//! only account metadata, model choices, text, and actionable, redacted errors.

mod keys;
mod oauth;
mod providers;
mod responses;
mod sse;
mod storage;

pub use keys::{ApiKey, KeySlot, KeyStore};
pub use oauth::{ChatGptService, PendingSignIn};
pub use providers::{Endpoint, ProviderClient, ProviderKind, ProviderModel, ProviderRequest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub use tokio_util::sync::CancellationToken;

/// Native initialization settings. Network destinations are deliberately fixed.
#[derive(Clone)]
pub struct CloudConfig {
    pub data_directory: PathBuf,
    pub app_name: String,
}

/// A distinct account/workspace registration, suitable for the frontend.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountProfile {
    pub id: String,
    pub label: String,
    pub email: Option<String>,
    pub signed_in: bool,
    pub plan_usage_enabled: bool,
    pub active: bool,
    pub preferred_model: Option<String>,
}

/// An account-specific model returned in the server's display ordering.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelChoice {
    pub slug: String,
    pub display_name: String,
}

/// A text-only request. The caller owns instruction and output-validation policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseRequest {
    pub profile_id: Option<String>,
    pub model: String,
    pub instructions: String,
    pub source_text: String,
    pub context_text: Option<String>,
}

/// Complete output, returned only after the stream's successful terminal event.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseResult {
    pub text: String,
    pub response_id: Option<String>,
}

/// Remote revocation can fail independently of local sign-out.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignOutResult {
    pub remote_revocation_confirmed: bool,
}

/// Redacted errors. Bodies/messages from remote services never become logs or UI.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudError {
    #[error("Cloud credentials are supported only on Windows and Linux")]
    UnsupportedPlatform,
    #[error("The cloud configuration is invalid")]
    Configuration,
    #[error("Protected account storage is unavailable")]
    Storage,
    #[error("No ChatGPT account is selected")]
    NoAccount,
    #[error("The account registration was not found")]
    UnknownProfile,
    #[error("Sign in again to renew this account")]
    SignInRequired,
    #[error("A previous rotating-token refresh was interrupted; sign in again")]
    InterruptedRefresh,
    #[error("ChatGPT plan usage is not enabled for this account")]
    PlanPermissionRequired,
    #[error("The system browser could not be opened")]
    Browser,
    #[error("Sign-in was declined")]
    AccessDenied,
    #[error("The sign-in callback was not valid")]
    InvalidCallback,
    #[error("The ChatGPT identity could not be verified")]
    InvalidIdentity,
    #[error("The operation was cancelled")]
    Cancelled,
    #[error("The cloud operation timed out")]
    Timeout,
    #[error("The cloud connection failed")]
    Transport,
    #[error("The server response exceeded a safety limit")]
    SizeLimit,
    #[error("The server response was malformed")]
    MalformedResponse,
    #[error("The selected model is no longer available for this account")]
    ModelUnavailable,
    #[error("The response stream ended before completion")]
    InterruptedStream,
    #[error("The response was incomplete")]
    IncompleteResponse,
    #[error("The provider address is not allowed")]
    EndpointNotAllowed,
    #[error("The provider needs an API key")]
    MissingApiKey,
    #[error("The API key is not valid")]
    InvalidApiKey,
    #[error("The model declined to answer")]
    Refusal,
    #[error("The provider could not be reached")]
    Unreachable,
    #[error("The cloud service rejected the request ({status})")]
    Remote {
        status: u16,
        code: Option<String>,
        parameter: Option<String>,
        request_id: Option<String>,
        body_shape: String,
    },
}

pub type Result<T> = std::result::Result<T, CloudError>;

pub(crate) fn unix_time() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| CloudError::Configuration)
}
