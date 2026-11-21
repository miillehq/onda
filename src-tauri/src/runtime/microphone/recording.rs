//! Capture preparation and completion use the operation admitted by the actor.
use super::*;
pub(super) async fn capture_context(
    host: &Arc<Runtime>,
    id: &str,
    settings: &Settings,
    command_mode: bool,
) -> AppResult<(TargetSnapshot, Option<String>, String)> {
    let context_settings = settings.clone();
    let native = tokio::task::spawn_blocking(move || {
        let targets = WindowsTargetService::new();
        let target = targets.snapshot().map_err(errors::platform)?;
        let selection = if command_mode {
            Some(
                targets
                    .selected_text(&target, onda_platform::MAX_TEXT_CHARACTERS)
                    .map_err(errors::selection)?,
            )
        } else {
            None
        };
        let context_enabled = context_settings.context_enabled
            && context_settings
                .profile_for(&target.executable_name)
                .is_none_or(|profile| profile.context_enabled);
        let context = if context_enabled {
            targets
                .context(&target, CONTEXT_CHARACTERS)
                .unwrap_or_default()
        } else {
            String::new()
        };
        Ok::<_, AppError>((target, selection, context))
    })
    .await
    .map_err(|_| {
        host.operations
            .worker_failure(host, id, "Target worker stopped.")
    })?;
    let (target, selection, mut context) = match native {
        Ok(result) => result,
        Err(error) => {
            host.operations.fail(host, id, error.clone());
            return Err(error);
        }
    };
    if let Some(selected) = selection.as_deref() {
        validate_selected_source(selected)
            .inspect_err(|error| host.operations.fail(host, id, error.clone()))?;
    }
    if settings
        .profile_for(&target.executable_name)
        .is_some_and(|profile| !profile.context_enabled)
    {
        context.clear();
    }
    Ok((target, selection, context))
}
pub(super) async fn start_capture(
    host: &Arc<Runtime>,
    id: &str,
    settings: &Settings,
    start_cancellation: CancellationToken,
) -> AppResult<()> {
    let capture_options = CaptureOptions {
        device_id: settings.microphone_id.clone(),
        max_duration_seconds: settings.max_recording_seconds,
    };
    let started = Instant::now();
    let service = host.clone();
    let capture_id = id.to_owned();
    let capture = tokio::task::spawn_blocking(move || {
        if start_cancellation.is_cancelled() {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        let mut capture = lock(&service.microphone.capture)?;
        capture.start(capture_options).map_err(errors::platform)?;
        *lock(&service.microphone.capture_owner)? = Some(capture_id);
        Ok(())
    })
    .await
    .map_err(|_| {
        host.operations
            .worker_failure(host, id, "Microphone worker stopped.")
    })?;
    host.performance
        .metric(host, id, "audioInitialization", started, capture.is_ok());
    if let Err(error) = capture {
        host.operations.fail(host, id, error.clone());
        return Err(error);
    }
    Ok(())
}
pub(super) fn validate_capture(
    host: &Arc<Runtime>,
    id: &str,
    audio: &onda_platform::CapturedAudio,
) -> AppResult<()> {
    if audio.diagnostics.limit_reached {
        host.domain(AppEvent::Error(AppError::new(
            ErrorCode::AudioLimit,
            "Recording reached its maximum duration. Captured speech is being processed.",
            false,
        )));
    }
    if audio.samples.is_empty() || audio.diagnostics.mostly_silent {
        // A tap of the hold-to-talk keys records a fraction of a second of silence; saying
        // how to hold them helps more than pointing at the microphone.
        let tapped = !lock(&host.operations.state)?.snapshot().hands_free
            && audio.samples.len() < SHORT_PRESS_SAMPLES;
        let error = if tapped {
            AppError::new(
                ErrorCode::ShortPress,
                "Keep holding the keys while you speak, then let go to type.",
                true,
            )
        } else {
            AppError::new(
                ErrorCode::NoSpeech,
                "No speech was detected. Check microphone level and try again.",
                true,
            )
        };
        host.operations.fail(host, id, error.clone());
        return Err(error);
    }
    Ok(())
}
