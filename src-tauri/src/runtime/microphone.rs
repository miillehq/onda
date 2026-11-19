//! Owns capture, its operation identity and level meter for recording and microphone checks.
use super::*;

pub(super) struct MicrophoneSession {
    pub(super) capture: Mutex<AudioCapture>,
    pub(super) level_meter: onda_platform::LevelMeter,
    pub(super) capture_owner: Mutex<Option<String>>,
    pub(super) microphone_testing: AtomicBool,
}

impl MicrophoneSession {
    pub(super) async fn begin_recording(
        &self,
        host: &Arc<Runtime>,
        command_mode: bool,
        hands_free: bool,
    ) -> AppResult<()> {
        if self.microphone_testing.load(Ordering::Acquire) {
            return Err(AppError::new(
                ErrorCode::Busy,
                "Microphone test is active.",
                true,
            ));
        }
        let settings = lock(&host.data.store)?.settings()?;
        let mode = if command_mode {
            OperationMode::SpokenCommand
        } else {
            OperationMode::Dictation
        };
        let id = {
            let mut state = lock(&host.operations.state)?;
            host.updates.ensure_not_updating()?;
            let id = state.begin(mode.clone())?;
            if hands_free {
                state.mark_hands_free(&id);
            }
            *lock(&host.results.result)? = None;
            *lock(&host.results.proposal)? = None;
            id
        };
        host.operations.emit_state(host);
        let started = Instant::now();
        validate_recording_model(host, &settings)
            .inspect_err(|error| host.operations.fail(host, &id, error.clone()))?;
        let (target, selection, context) =
            capture_context(host, &id, &settings, command_mode).await?;
        let operation = ActiveOperation {
            id: id.clone(),
            target: Some(target),
            mode,
            selection,
            context,
            settings: settings.clone(),
            cancellation: CancellationToken::new(),
            history: true,
            usage: None,
        };
        let start_cancellation = operation.cancellation.clone();
        let warm_context = operation.context.clone();
        let warm_cancellation = operation.cancellation.clone();
        host.operations.set_active(operation)?;
        if !lock(&host.operations.state)?.matches(&id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        host.performance
            .metric(host, &id, "targetSnapshot", started, true);
        start_capture(host, &id, &settings, start_cancellation).await?;
        let level_id = id.clone();
        host.stream_levels(move |service| {
            lock(&service.operations.state)
                .map(|state| state.matches(&level_id) && state.snapshot().phase == Phase::Recording)
                .unwrap_or(false)
        });
        host.recognition
            .start_warm_worker(host, &id, &settings, &warm_context, warm_cancellation);
        // Only ChatGPT's account and model list are worth warming; other providers are asked
        // nothing until there is text, and an unchosen provider is never contacted.
        if (settings.enhancement_enabled || command_mode)
            && settings.ai_provider == AiProvider::Chatgpt
        {
            host.rewrites.accounts.prefetch_polish(host);
        }
        self.monitor_capture(host, id);
        Ok(())
    }

    /// Publish the live input level while `open` holds; levels carry no audio content.
    pub(super) fn stream_levels(
        &self,
        host: &Arc<Runtime>,
        open: impl Fn(&Runtime) -> bool + Send + 'static,
    ) {
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            while open(&service) && !service.operations.shutdown.load(Ordering::Acquire) {
                service.event(RuntimeEvent::AudioLevel {
                    level: service.microphone.level_meter.level(),
                });
                tokio::time::sleep(LEVEL_INTERVAL).await;
            }
            service.event(RuntimeEvent::AudioLevel { level: 0.0 });
        });
    }

    pub(super) fn monitor_capture(&self, host: &Arc<Runtime>, id: String) {
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let recording = lock(&service.operations.state)
                    .map(|state| state.matches(&id) && state.snapshot().phase == Phase::Recording)
                    .unwrap_or(false);
                if !recording {
                    return;
                }
                let native_service = service.clone();
                let status = tokio::task::spawn_blocking(move || {
                    lock(&native_service.microphone.capture)?
                        .poll_completion()
                        .map_err(errors::platform)
                })
                .await;
                match status {
                    Ok(Ok(onda_platform::CaptureCompletion::Running)) => {}
                    Ok(Ok(onda_platform::CaptureCompletion::Finished)) => {
                        let _ = service
                            .operations
                            .send_action(Action::StopGeneration { id })
                            .await;
                        return;
                    }
                    Ok(Err(error)) => {
                        service.operations.fail(&service, &id, error);
                        let _ = service.microphone.cancel_generation(&service, &id).await;
                        return;
                    }
                    Err(_) => {
                        let error = service.operations.worker_failure(
                            &service,
                            &id,
                            "Capture status worker stopped.",
                        );
                        service.domain(AppEvent::Error(error));
                        let _ = service.microphone.cancel_generation(&service, &id).await;
                        return;
                    }
                }
            }
        });
    }

    pub(super) async fn finish_recording(&self, host: &Arc<Runtime>) -> AppResult<()> {
        if lock(&host.operations.state)?.snapshot().phase != Phase::Recording {
            return Ok(());
        }
        let active = lock(&host.operations.active)?
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                AppError::new(ErrorCode::Internal, "Recording session is missing.", true)
            })?;
        host.operations
            .transition(host, &active.id, Phase::FinalizingAudio)?;
        let service = host.clone();
        let started = Instant::now();
        let audio = tokio::task::spawn_blocking(move || {
            let mut capture = lock(&service.microphone.capture)?;
            let result = capture.stop().map_err(errors::platform);
            *lock(&service.microphone.capture_owner)? = None;
            result
        })
        .await
        .map_err(|_| {
            host.operations
                .worker_failure(host, &active.id, "Audio finalization stopped.")
        })?;
        host.performance.metric(
            host,
            &active.id,
            "audioFinalization",
            started,
            audio.is_ok(),
        );
        let audio = match audio {
            Ok(audio) => audio,
            Err(error) => {
                host.operations.fail(host, &active.id, error.clone());
                return Err(error);
            }
        };
        validate_capture(host, &active.id, &audio)?;
        host.operations
            .transition(host, &active.id, Phase::Transcribing)?;
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            let id = active.id.clone();
            if let Err(error) = service
                .recognition
                .process_audio(&service, active, audio.samples)
                .await
            {
                if error.code != ErrorCode::Cancelled {
                    service.operations.fail(&service, &id, error);
                }
            }
        });
        Ok(())
    }

    pub(super) async fn cancel_generation(&self, host: &Arc<Runtime>, id: &str) -> AppResult<()> {
        let service = host.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut capture = lock(&service.microphone.capture)?;
            let mut owner = lock(&service.microphone.capture_owner)?;
            if owner.as_deref() == Some(&id) {
                let result = capture.cancel().map_err(errors::platform);
                *owner = None;
                result
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Microphone cleanup stopped.", true))?
    }
}

impl MicrophoneSession {
    pub(super) fn new() -> Self {
        let capture = AudioCapture::new();
        let level_meter = capture.level_meter();
        Self {
            capture: Mutex::new(capture),
            level_meter,
            capture_owner: Mutex::new(None),
            microphone_testing: AtomicBool::new(false),
        }
    }
}

mod recording;
use recording::{capture_context, start_capture, validate_capture};

fn validate_recording_model(host: &Runtime, settings: &Settings) -> AppResult<()> {
    let model = host.recognition.selected_model(settings)?;
    let installed = lock(&host.model_library.models)?
        .installed()
        .map_err(errors::asr)?;
    require_installed_model(model, &installed)
}
