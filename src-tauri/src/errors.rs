//! OS and remote diagnostics are mapped to content-free UI errors.
use onda_core::error::{AppError, ErrorCode};

pub fn platform(error: onda_platform::PlatformError) -> AppError {
    use onda_platform::PlatformError as E;
    // Fixed phrases, HRESULT numbers or clipboard format numbers only; never text.
    match &error {
        E::TargetUnavailable(detail) => {
            tracing::warn!(subsystem = "target", detail = %detail, "target unavailable")
        }
        E::Clipboard(detail) => {
            tracing::warn!(subsystem = "clipboard", detail = %detail, "clipboard transaction failed")
        }
        _ => {}
    }
    let (code,message,retryable)=match error {
        E::Cancelled=>(ErrorCode::Cancelled,"Native insertion was cancelled.",false),
        E::AlreadyRecording=>(ErrorCode::Busy,"Microphone capture is already active.",true),
        E::NotRecording=>(ErrorCode::InvalidInput,"No microphone capture is active.",false),
        E::DeviceDisconnected=>(ErrorCode::AudioDisconnected,"Microphone disconnected. Choose a connected input and retry.",true),
        E::AudioRouteChanged=>(ErrorCode::AudioRouteChanged,"Windows changed or invalidated the microphone stream. Select your microphone in Settings and retry.",true),
        E::AudioPermissionDenied=>(ErrorCode::AudioPermissionDenied,"Windows denied microphone access. Enable microphone access for desktop apps in Windows privacy settings and retry.",true),
        E::AudioBackendFailed(kind)=>return audio_backend(kind),
        E::RecordingLimit|E::AudioOverflow=>(ErrorCode::AudioLimit,"Recording reached its limit or lost audio; retry a shorter recording.",true),
        E::TargetChanged=>(ErrorCode::FocusChanged,"The focused app, control, or selection changed. Your text is available to copy.",true),
        E::SecureField=>(ErrorCode::PasswordField,"Password and secure controls cannot receive dictation.",false),
        E::ElevatedTarget=>(ErrorCode::ElevatedTarget,"Windows blocked insertion into an elevated app. Copy the result manually.",false),
        E::TargetUnavailable(_)|E::SelectionUnavailable=>(ErrorCode::UnsupportedControl,"This app did not expose a safe accessible text target.",true),
        E::EmptySelection=>(ErrorCode::SelectionRequired,"Select text in your target app before using a transform or spoken editing shortcut.",true),
        E::UnsupportedClipboardFormat(_)=>(ErrorCode::ClipboardUnsupported,"The clipboard contains a format that cannot be preserved safely. Copy the result manually.",true),
        E::Clipboard(_)=>(ErrorCode::ClipboardBusy,"Clipboard transaction failed. Your result is available to copy.",true),
        E::InjectionFailed|E::ModifiersHeld=>(ErrorCode::Insertion,"Insertion failed or modifier keys are still held. Release the shortcut and retry.",true),
        E::TextLimit|E::InvalidParameter(_)=>(ErrorCode::InvalidInput,"Native operation parameters are invalid.",false),
        E::Audio(_)=>(ErrorCode::NoMicrophone,"The selected microphone is unavailable or could not start.",true),
        E::Timeout=>(ErrorCode::InferenceTimeout,"A Windows operation timed out.",true),
        E::UnsupportedPlatform if cfg!(windows)=>(ErrorCode::Internal,"Windows 10 or Windows 11 is required.",false),
        E::UnsupportedPlatform=>(ErrorCode::UnsupportedControl,"This needs Windows; on this system Onda can only copy dictation to the clipboard.",false),
        E::Protection=>return AppError::storage(),
    };
    AppError::new(code, message, retryable)
}

/// Describes driver failure categories without exposing device/backend diagnostic strings.
fn audio_backend(kind: onda_platform::AudioBackendFailure) -> AppError {
    use onda_platform::AudioBackendFailure as E;
    let message = match kind {
        E::DeviceBusy => "The microphone is busy. Close apps using it exclusively and retry.",
        E::HostUnavailable => "Windows audio is unavailable. Check the audio service and retry.",
        E::UnsupportedConfig | E::UnsupportedOperation | E::InvalidInput => "The microphone driver rejected its recording format. Select another input in Settings and retry.",
        E::ResourceExhausted => "Windows could not allocate resources for recording. Close unused apps and retry.",
        E::BackendError | E::Other => "The microphone driver stopped recording. Check the input or virtual microphone connection and retry.",
    };
    AppError::new(ErrorCode::AudioBackend, message, true)
}

pub fn asr(error: onda_asr::AsrError) -> AppError {
    use onda_asr::AsrError as E;
    let (code, message) = match error {
        E::Cancelled => (ErrorCode::Cancelled, "Local transcription was cancelled."),
        E::Busy | E::DownloadBusy => (
            ErrorCode::Busy,
            "A local model operation is already running.",
        ),
        E::Timeout => (
            ErrorCode::InferenceTimeout,
            "Local transcription timed out; reduce recording length or model size.",
        ),
        E::ModelMissing | E::WorkerMissing => (
            ErrorCode::NoModel,
            "Install a local model and ensure the packaged ASR worker is present.",
        ),
        E::ChecksumMismatch => (
            ErrorCode::CorruptModel,
            "The local model is corrupted. Download it again.",
        ),
        E::InsufficientDisk { .. } => (
            ErrorCode::InsufficientDisk,
            "There is insufficient disk space for the model.",
        ),
        E::DownloadNetwork(_) | E::InvalidDownload => (
            ErrorCode::Download,
            "The model download failed. Retry to resume or recover it.",
        ),
        E::UnknownModel | E::UnsafePath | E::InvalidOptions(_) | E::InvalidAudio(_) => (
            ErrorCode::InvalidInput,
            "The model or audio parameters are invalid.",
        ),
        E::Io(_) | E::WorkerFailed(_) | E::WorkerProtocol | E::Inference(_) => (
            ErrorCode::Inference,
            "Local inference failed; your audio was discarded. Try another model or CPU mode.",
        ),
    };
    AppError::new(code, message, true)
}

pub fn cloud(error: onda_cloud::CloudError) -> AppError {
    use onda_cloud::CloudError as E;
    if !matches!(error, E::Cancelled) {
        // API error codes and parameter names identify the cause; bodies are never logged.
        let (status, code, parameter) = match &error {
            E::Remote {
                status,
                code,
                parameter,
                ..
            } => (Some(*status), code.as_deref(), parameter.as_deref()),
            _ => (None, None, None),
        };
        tracing::warn!(subsystem = "chatgpt", error = %error, status = ?status, code = ?code, parameter = ?parameter, "ChatGPT request failed");
    }
    if matches!(error, E::Storage) {
        return AppError::new(
            ErrorCode::Cloud,
            "Protected ChatGPT storage is unavailable. Local dictation remains available.",
            true,
        );
    }
    if matches!(error, E::UnsupportedPlatform) {
        return AppError::new(
            ErrorCode::Cloud,
            "Sign in with ChatGPT needs Windows' protected storage. On this system, choose another provider in AI polish.",
            false,
        );
    }
    let code = match &error {
        E::Cancelled => ErrorCode::Cancelled,
        E::Timeout => ErrorCode::InferenceTimeout,
        E::NoAccount
        | E::SignInRequired
        | E::InterruptedRefresh
        | E::PlanPermissionRequired
        | E::AccessDenied
        | E::InvalidIdentity => ErrorCode::Unauthorized,
        E::Transport => ErrorCode::Offline,
        E::Remote { status: 429, .. } => ErrorCode::Quota,
        E::Remote {
            status: 401 | 403, ..
        } => ErrorCode::Unauthorized,
        _ => ErrorCode::Cloud,
    };
    let message = match code {
        ErrorCode::Cancelled => "ChatGPT operation was cancelled.",
        ErrorCode::Unauthorized => {
            "Connect an eligible ChatGPT account and enable plan usage, or sign in again."
        }
        ErrorCode::Quota => "ChatGPT plan usage limit reached. Local dictation remains available.",
        ErrorCode::Offline => "ChatGPT connection failed. Your original text was kept.",
        ErrorCode::InferenceTimeout => "ChatGPT timed out. Your original text was kept.",
        _ => "ChatGPT request failed or returned an invalid stream. Your original text was kept.",
    };
    AppError::new(code, message, true)
}

/// Provider-specific, actionable messages. Logs keep the status and error code, never a body.
pub fn provider(
    provider: onda_core::contracts::AiProvider,
    endpoint: Option<&onda_cloud::Endpoint>,
    error: onda_cloud::CloudError,
) -> AppError {
    use onda_cloud::CloudError as E;
    use onda_core::contracts::AiProvider as P;
    if !matches!(error, E::Cancelled) {
        let (status, code) = match &error {
            E::Remote { status, code, .. } => (Some(*status), code.as_deref()),
            _ => (None, None),
        };
        tracing::warn!(subsystem = "ai", provider = ?provider, error = %error, status = ?status, code = ?code, "AI provider request failed");
    }
    let name = match provider {
        P::Chatgpt => "ChatGPT",
        P::OpenaiCompatible => "The AI server",
        P::Anthropic => "Anthropic",
        P::Local => "The local model server",
    };
    let host = endpoint.map(onda_cloud::Endpoint::host).unwrap_or_default();
    let kept = "Your original text was kept.";
    let (code, message) = match error {
        E::Cancelled => (ErrorCode::Cancelled, "Polish was cancelled.".to_owned()),
        E::EndpointNotAllowed if provider == P::Local => (
            ErrorCode::InvalidInput,
            "Local models must use an address on this computer, such as http://127.0.0.1:11434/v1.".to_owned(),
        ),
        E::EndpointNotAllowed => (
            ErrorCode::InvalidInput,
            "Use an https:// address (plain http:// is allowed only for this computer).".to_owned(),
        ),
        E::InvalidApiKey => (
            ErrorCode::InvalidInput,
            "That API key doesn't look right. Paste the whole key, without spaces.".to_owned(),
        ),
        E::MissingApiKey => (
            ErrorCode::Unauthorized,
            format!("Add your {} API key in AI polish.", if provider == P::Anthropic { "Anthropic" } else { "server's" }),
        ),
        E::Storage | E::UnsupportedPlatform if cfg!(windows) => (
            ErrorCode::Cloud,
            "Protected key storage is unavailable, so Onda can't save or read API keys.".to_owned(),
        ),
        E::Storage | E::UnsupportedPlatform => (
            ErrorCode::Cloud,
            "Onda keeps API keys in the desktop keyring (Secret Service), and none is available or it is locked.".to_owned(),
        ),
        E::Unreachable if provider == P::Local => (
            ErrorCode::Offline,
            format!("Nothing answered at {host}. Start your local model server (Ollama, LM Studio or llama.cpp) and try again."),
        ),
        E::Unreachable | E::Transport => (ErrorCode::Offline, format!("Couldn't reach {host}.")),
        E::Timeout => (ErrorCode::InferenceTimeout, format!("{name} took too long. {kept}")),
        E::Remote { status: 401 | 403, .. } => (
            ErrorCode::Unauthorized,
            format!("{name} refused the API key. Check it in AI polish."),
        ),
        E::Remote { status: 404, .. } => (
            ErrorCode::Cloud,
            format!("{name} doesn't offer the chosen model, or the address is wrong. Check AI polish."),
        ),
        E::Remote { status: 429, .. } => (
            ErrorCode::Quota,
            format!("{name} rate limit or quota reached. {kept}"),
        ),
        E::Refusal => (ErrorCode::Cloud, format!("The model declined to rewrite this. {kept}")),
        E::IncompleteResponse => (ErrorCode::Cloud, format!("The answer was cut off. {kept}")),
        _ => (
            ErrorCode::Cloud,
            format!("{name} returned an error or an unreadable answer. {kept}"),
        ),
    };
    let retryable = code != ErrorCode::InvalidInput;
    AppError::new(code, message, retryable)
}

#[cfg(test)]
mod provider_error_tests {
    use super::*;
    use onda_core::contracts::AiProvider;

    #[test]
    fn local_server_errors_name_the_address_and_what_to_start() {
        let endpoint = onda_cloud::Endpoint::local("http://127.0.0.1:11434/v1").unwrap();
        let error = provider(
            AiProvider::Local,
            Some(&endpoint),
            onda_cloud::CloudError::Unreachable,
        );
        assert_eq!(error.code, ErrorCode::Offline);
        assert!(error.message.contains("127.0.0.1:11434") && error.message.contains("Ollama"));
    }

    #[test]
    fn remote_failures_never_include_response_text() {
        let error = provider(
            AiProvider::OpenaiCompatible,
            None,
            onda_cloud::CloudError::Remote {
                status: 401,
                code: Some("invalid_api_key".into()),
                parameter: None,
                request_id: Some("req_1".into()),
                body_shape: "error".into(),
            },
        );
        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(!error.message.contains("invalid_api_key") && !error.message.contains("req_1"));
    }
}

#[cfg(test)]
mod audio_error_tests {
    use super::*;

    #[test]
    fn audio_failure_categories_have_distinct_actionable_errors() {
        use onda_platform::{AudioBackendFailure, PlatformError};
        for (failure, expected_code, recovery_hint) in [
            (
                PlatformError::EmptySelection,
                ErrorCode::SelectionRequired,
                "Select text",
            ),
            (
                PlatformError::DeviceDisconnected,
                ErrorCode::AudioDisconnected,
                "connected input",
            ),
            (
                PlatformError::AudioRouteChanged,
                ErrorCode::AudioRouteChanged,
                "Settings",
            ),
            (
                PlatformError::AudioPermissionDenied,
                ErrorCode::AudioPermissionDenied,
                "privacy settings",
            ),
            (
                PlatformError::AudioBackendFailed(AudioBackendFailure::DeviceBusy),
                ErrorCode::AudioBackend,
                "exclusively",
            ),
            (
                PlatformError::AudioBackendFailed(AudioBackendFailure::BackendError),
                ErrorCode::AudioBackend,
                "virtual microphone",
            ),
        ] {
            let error = platform(failure);
            assert_eq!(error.code, expected_code);
            assert!(error.message.contains(recovery_hint));
        }
    }
}
