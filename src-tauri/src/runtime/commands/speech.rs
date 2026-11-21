use super::*;
use ApiResponse as O;

pub(super) async fn list_audio_devices() -> AppResult<ApiResponse> {
    let devices = tokio::task::spawn_blocking(AudioCapture::enumerate_devices)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Device enumeration stopped.", true))?
        .map_err(errors::platform)?;
    Ok(O::AudioDevices(
        devices
            .into_iter()
            .map(|device| AudioDevice {
                id: device.id,
                name: device.name,
                is_default: device.is_default,
            })
            .collect(),
    ))
}

pub(super) async fn test_microphone(
    service: &Arc<Runtime>,
    duration_ms: u32,
) -> AppResult<ApiResponse> {
    Ok(O::Microphone(
        service
            .microphone
            .test_microphone(service, duration_ms)
            .await?,
    ))
}

pub(super) fn start_model_download(
    service: &Arc<Runtime>,
    model_id: String,
) -> AppResult<ApiResponse> {
    service.model_library.start_download(service, model_id)?;
    Ok(O::Unit)
}

pub(super) fn cancel_model_download(
    service: &Arc<Runtime>,
    model_id: String,
) -> AppResult<ApiResponse> {
    if let Some(cancellation) = lock(&service.model_library.downloads)?.get(&model_id) {
        cancellation.cancel();
    }
    Ok(O::Unit)
}

pub(super) async fn uninstall_model(
    service: &Arc<Runtime>,
    model_id: String,
) -> AppResult<ApiResponse> {
    if lock(&service.operations.state)?.is_active() {
        return Err(AppError::new(
            ErrorCode::Busy,
            "Wait for the operation before removing a model.",
            true,
        ));
    }
    let manager = lock(&service.model_library.models)?.clone();
    manager.uninstall(&model_id).await.map_err(errors::asr)?;
    Ok(O::Unit)
}

impl MicrophoneSession {
    pub(in crate::runtime) async fn test_microphone(
        &self,
        host: &Arc<Runtime>,
        duration_ms: u32,
    ) -> AppResult<MicrophoneReport> {
        if !(500..=10000).contains(&duration_ms) {
            return Err(AppError::invalid(
                "Microphone test must last 0.5–10 seconds.",
            ));
        }
        {
            let state = lock(&host.operations.state)?;
            host.updates.ensure_not_updating()?;
            if state.is_active() || self.microphone_testing.swap(true, Ordering::AcqRel) {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "Microphone is in use.",
                    true,
                ));
            }
        }
        let result = async {
            let settings = lock(&host.data.store)?.settings()?;
            let service = host.clone();
            tokio::task::spawn_blocking(move || {
                lock(&service.microphone.capture)?
                    .start(CaptureOptions {
                        device_id: settings.microphone_id,
                        max_duration_seconds: 15,
                    })
                    .map_err(errors::platform)
            })
            .await
            .map_err(|_| {
                AppError::new(ErrorCode::Internal, "Microphone worker stopped.", true)
            })??;
            host.stream_levels(|service| {
                service
                    .microphone
                    .microphone_testing
                    .load(Ordering::Acquire)
            });
            tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;
            let service = host.clone();
            let audio = tokio::task::spawn_blocking(move || {
                lock(&service.microphone.capture)?
                    .stop()
                    .map_err(errors::platform)
            })
            .await
            .map_err(|_| {
                AppError::new(ErrorCode::Internal, "Microphone worker stopped.", true)
            })??;
            let diagnostics = audio.diagnostics;
            Ok(MicrophoneReport {
                duration_ms: diagnostics.duration_ms as f64,
                peak: diagnostics.peak as f64,
                rms: diagnostics.rms,
                clipped_samples: diagnostics.clipped_samples as f64,
                discontinuities: diagnostics.discontinuities as f64,
                mostly_silent: diagnostics.mostly_silent,
                source_sample_rate: diagnostics.source_sample_rate,
                source_channels: diagnostics.source_channels,
            })
        }
        .await;
        self.microphone_testing.store(false, Ordering::Release);
        result
    }
}

impl SpeechRecognition {
    pub(in crate::runtime) fn hardware_report(&self) -> HardwareReport {
        let hardware = self.asr.hardware();
        HardwareReport {
            logical_cpus: hardware.logical_cpus,
            physical_cpus: hardware.physical_cpus,
            total_memory_bytes: hardware.total_memory_bytes as f64,
            available_memory_bytes: hardware.available_memory_bytes as f64,
            inference_threads: hardware.inference_threads(),
            simd: hardware.simd.clone(),
            gpus: hardware
                .gpus
                .iter()
                .filter(|gpu| !gpu.software_adapter)
                .map(|gpu| GpuInfo {
                    name: gpu.name.clone(),
                    vendor_id: gpu.vendor_id,
                    dedicated_memory_bytes: gpu.dedicated_memory_bytes as f64,
                    shared_memory_bytes: gpu.shared_memory_bytes as f64,
                })
                .collect(),
            vulkan_loader_present: hardware.vulkan_loader_present,
            cuda_driver_present: hardware.cuda_driver_present,
            directml_library_present: hardware.directml_library_present,
            vulkan_worker_installed: self.asr.vulkan_worker_installed(),
        }
    }

    /// One take of a word for the dictionary: record, then transcribe with dictation's model,
    /// language and vocabulary prompt. Audio and text stay in memory; the user decides which
    /// misrecognitions become replacements.
    pub(in crate::runtime) async fn teach_word(
        &self,
        host: &Arc<Runtime>,
        duration_ms: u32,
    ) -> AppResult<TeachTake> {
        if !(1000..=6000).contains(&duration_ms) {
            return Err(AppError::invalid("A teaching take must last 1–6 seconds."));
        }
        let settings = lock(&host.data.store)?.settings()?;
        let model = self.selected_model(&settings)?;
        let installed = lock(&host.model_library.models)?
            .installed()
            .map_err(errors::asr)?;
        require_installed_model(model, &installed)?;
        {
            let state = lock(&host.operations.state)?;
            host.updates.ensure_not_updating()?;
            if state.is_active()
                || host
                    .microphone
                    .microphone_testing
                    .swap(true, Ordering::AcqRel)
            {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "Microphone is in use.",
                    true,
                ));
            }
        }
        let result = async {
            let audio =
                capture_teaching_take(host, settings.microphone_id.clone(), duration_ms).await?;
            let cancellation = CancellationToken::new();
            let models = lock(&host.model_library.models)?.clone();
            let path = models
                .verified_model_path(model.id, &cancellation)
                .await
                .map_err(errors::asr)?;
            let options = host.transcription_options(&settings, path, "")?;
            let transcription = self
                .asr
                .transcribe(&audio.samples, options, cancellation)
                .await
                .map_err(errors::asr)?;
            drop(audio);
            let heard = transcription
                .text
                .trim_matches(|character: char| !character.is_alphanumeric())
                .to_owned();
            if heard.is_empty() {
                return Err(AppError::new(
                    ErrorCode::NoSpeech,
                    "Onda didn't catch a word. Try again, saying just the word.",
                    true,
                ));
            }
            let written = lock(&host.data.index)?.normalize(&heard)?;
            Ok(TeachTake { heard, written })
        }
        .await;
        host.microphone
            .microphone_testing
            .store(false, Ordering::Release);
        result
    }
}

impl ModelLibrary {
    pub(in crate::runtime) fn start_download(
        &self,
        host: &Arc<Runtime>,
        id: String,
    ) -> AppResult<()> {
        if !onda_asr::builtin_models()
            .iter()
            .any(|model| model.id == id)
        {
            return Err(AppError::invalid("Unknown model."));
        }
        let cancellation = CancellationToken::new();
        {
            let mut downloads = lock(&self.downloads)?;
            if downloads.contains_key(&id) {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "This model is already downloading.",
                    true,
                ));
            }
            downloads.insert(id.clone(), cancellation.clone());
        }
        let manager = lock(&self.models)?.clone();
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            let progress_service = service.clone();
            let result = manager
                .download(&id, &cancellation, move |progress| {
                    progress_service.event(RuntimeEvent::DownloadProgress(DownloadReport {
                        model_id: progress.model_id,
                        stage: match progress.stage {
                            onda_asr::DownloadStage::Downloading => "downloading",
                            onda_asr::DownloadStage::Verifying => "verifying",
                            onda_asr::DownloadStage::Installed => "installed",
                        }
                        .into(),
                        completed_bytes: progress.completed_bytes as f64,
                        total_bytes: progress.total_bytes as f64,
                    }))
                })
                .await;
            if let Ok(mut downloads) = lock(&service.model_library.downloads) {
                downloads.remove(&id);
            }
            if let Err(error) = result {
                service.domain(AppEvent::Error(errors::asr(error)));
            }
        });
        Ok(())
    }
}

/// Capture a bounded in-memory word sample while the teaching session holds the microphone.
async fn capture_teaching_take(
    host: &Arc<Runtime>,
    device_id: Option<String>,
    duration_ms: u32,
) -> AppResult<onda_platform::CapturedAudio> {
    let service = host.clone();
    tokio::task::spawn_blocking(move || {
        lock(&service.microphone.capture)?
            .start(CaptureOptions {
                device_id,
                max_duration_seconds: 15,
            })
            .map_err(errors::platform)
    })
    .await
    .map_err(|_| AppError::new(ErrorCode::Internal, "Microphone worker stopped.", true))??;
    host.stream_levels(|service| {
        service
            .microphone
            .microphone_testing
            .load(Ordering::Acquire)
    });
    tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;
    let service = host.clone();
    let audio = tokio::task::spawn_blocking(move || {
        lock(&service.microphone.capture)?
            .stop()
            .map_err(errors::platform)
    })
    .await
    .map_err(|_| AppError::new(ErrorCode::Internal, "Microphone worker stopped.", true))??;
    if audio.samples.is_empty() || audio.diagnostics.mostly_silent {
        return Err(AppError::new(
            ErrorCode::NoSpeech,
            "Onda didn't hear anything. Say the word a little louder or closer to the microphone.",
            true,
        ));
    }
    Ok(audio)
}
