//! IPC commands validate policy, then delegate to native/domain services.
use super::*;
use tauri::Manager;

/// Enough for a year of daily bars without paging.
const MAX_REPORTED_USAGE_DAYS: u32 = 366;

impl Runtime {
    pub async fn dispatch(self: &Arc<Self>, request: ApiRequest) -> AppResult<ApiResponse> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "The application is shutting down.",
                false,
            ));
        }
        use ApiRequest as R;
        use ApiResponse as O;
        match request {
            R::CompleteDebugSmoke { passed, checks } => {
                if !cfg!(debug_assertions) {
                    return Err(AppError::invalid(
                        "Debug smoke reporting is unavailable in release builds.",
                    ));
                }
                let directory = crate::debug_smoke_directory().ok_or_else(|| {
                    AppError::invalid("No isolated debug smoke directory was configured.")
                })?;
                if checks.len() > 64
                    || checks.iter().any(|check| {
                        check.len() > 128
                            || !check.chars().all(|character| {
                                character.is_ascii_alphanumeric()
                                    || character == '-'
                                    || character == ' '
                            })
                    })
                {
                    return Err(AppError::invalid("Smoke report was invalid."));
                }
                let report=serde_json::to_vec_pretty(&serde_json::json!({"passed":passed,"checks":checks,"version":onda_core::APP_VERSION})).map_err(|_|AppError::storage())?;
                std::fs::write(directory.join("report.json"), report)
                    .map_err(|_| AppError::storage())?;
                self.app.exit(if passed { 0 } else { 1 });
                Ok(O::Unit)
            }
            R::GetSnapshot => Ok(O::Snapshot(Box::new(self.snapshot().await?))),
            R::ReplayDebugFixture => Ok(O::Result(self.replay_debug_fixture().await?)),
            R::GetSettings => Ok(O::Settings(Box::new(lock(&self.store)?.settings()?))),
            R::UpdateSettings { settings } => {
                let _configuration = self.configuration_gate.lock().await;
                settings.validate()?;
                super::providers::validate_provider_settings(&settings)?;
                if settings
                    .language
                    .as_deref()
                    .is_some_and(|language| !onda_asr::is_supported_language(language))
                {
                    return Err(AppError::invalid(
                        "Onda's speech models don't support that language. Cantonese, for example, needs a large-v3 model, which Onda doesn't offer.",
                    ));
                }
                self.selected_model(&settings)?;
                let config = shortcut_config(&settings, &lock(&self.store)?.transforms()?)?;
                self.cancel().await?;
                let current = lock(&self.store)?.settings()?;
                if current.launch_at_login != settings.launch_at_login {
                    crate::apply_launch_at_login(settings.launch_at_login)?;
                }
                let migrated = if current.model_directory != settings.model_directory {
                    let path = settings
                        .model_directory
                        .as_ref()
                        .map(PathBuf::from)
                        .map(Ok)
                        .unwrap_or_else(|| {
                            self.app
                                .path()
                                .app_local_data_dir()
                                .map(|directory| directory.join("models"))
                                .map_err(|_| AppError::storage())
                        })?;
                    validate_model_directory(&path)?;
                    if !lock(&self.downloads)?.is_empty() {
                        return Err(AppError::new(
                            ErrorCode::Busy,
                            "Finish model downloads before moving the model directory.",
                            true,
                        ));
                    }
                    let manager = lock(&self.models)?.clone();
                    Some(
                        manager
                            .migrate_to(path, &CancellationToken::new())
                            .await
                            .map_err(errors::asr)?,
                    )
                } else {
                    None
                };
                lock(&self.store)?.save_settings(&settings)?;
                self.set_history_days(settings.history_days)?;
                if let Some(manager) = migrated {
                    *lock(&self.models)? = manager;
                }
                // Restarting the shortcut helper drops shortcuts for a moment; only do it when
                // the shortcuts themselves changed, not for every quality or theme switch.
                let mut helper_config = lock(&self.helper_config)?;
                if *helper_config != config {
                    *helper_config = config;
                    self.helper_generation.fetch_add(1, Ordering::AcqRel);
                }
                drop(helper_config);
                lock(&self.state)?.clear_no_model_error();
                self.pill
                    .configure(&self.app, settings.show_pill, settings.play_sounds);
                if current.theme != settings.theme {
                    crate::apply_theme(&self.app, settings.theme);
                }
                if current.tray_icon != settings.tray_icon {
                    crate::apply_tray_icon(&self.app, settings.tray_icon);
                }
                if (&current.openai_base_url, &current.local_base_url)
                    != (&settings.openai_base_url, &settings.local_base_url)
                {
                    *lock(&self.provider_models)? = None;
                }
                self.emit_state();
                Ok(O::Settings(settings))
            }
            R::GetDictionary => Ok(O::Dictionary(lock(&self.store)?.dictionary()?)),
            R::SaveDictionaryEntry { entry } => {
                let _dictionary = self.dictionary_gate.lock().await;
                lock(&self.store)?.save_dictionary_entry(&entry)?;
                self.rebuild_dictionary().await?;
                Ok(O::Dictionary(lock(&self.store)?.dictionary()?))
            }
            R::DeleteDictionaryEntry { id } => {
                let _dictionary = self.dictionary_gate.lock().await;
                lock(&self.store)?.delete_dictionary_entry(&id)?;
                self.rebuild_dictionary().await?;
                Ok(O::Unit)
            }
            R::ImportDictionary { json } => {
                let _dictionary = self.dictionary_gate.lock().await;
                let added = lock(&self.store)?.import_dictionary(&json)?;
                self.rebuild_dictionary().await?;
                Ok(O::Count(added))
            }
            R::ExportDictionary => Ok(O::Text(lock(&self.store)?.export_dictionary()?)),
            R::GetSnippets => Ok(O::Snippets(lock(&self.store)?.snippets()?)),
            R::SaveSnippet { snippet } => {
                let _dictionary = self.dictionary_gate.lock().await;
                lock(&self.store)?.save_snippet(&snippet)?;
                self.rebuild_dictionary().await?;
                Ok(O::Snippets(lock(&self.store)?.snippets()?))
            }
            R::DeleteSnippet { id } => {
                let _dictionary = self.dictionary_gate.lock().await;
                lock(&self.store)?.delete_snippet(&id)?;
                self.rebuild_dictionary().await?;
                Ok(O::Unit)
            }
            R::GetTransforms => Ok(O::Transforms(lock(&self.store)?.transforms()?)),
            R::SaveTransform { transform } => {
                let _configuration = self.configuration_gate.lock().await;
                let settings = lock(&self.store)?.settings()?;
                let mut transforms = lock(&self.store)?.transforms()?;
                transforms.retain(|entry| entry.id != transform.id);
                transforms.push(transform.clone());
                let config = shortcut_config(&settings, &transforms)?;
                lock(&self.store)?.save_transform(&transform)?;
                *lock(&self.helper_config)? = config;
                self.helper_generation.fetch_add(1, Ordering::AcqRel);
                Ok(O::Transforms(transforms))
            }
            R::DeleteTransform { id } => {
                let _configuration = self.configuration_gate.lock().await;
                lock(&self.store)?.delete_transform(&id)?;
                let settings = lock(&self.store)?.settings()?;
                let config = shortcut_config(&settings, &lock(&self.store)?.transforms()?)?;
                *lock(&self.helper_config)? = config;
                self.helper_generation.fetch_add(1, Ordering::AcqRel);
                Ok(O::Unit)
            }
            R::ListAudioDevices => {
                let devices = tokio::task::spawn_blocking(AudioCapture::enumerate_devices)
                    .await
                    .map_err(|_| {
                        AppError::new(ErrorCode::Internal, "Device enumeration stopped.", true)
                    })?
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
            R::TestMicrophone { duration_ms } => {
                Ok(O::Microphone(self.test_microphone(duration_ms).await?))
            }
            R::StartDictation { command_mode } => {
                self.send_action(Action::Start { command_mode }).await?;
                Ok(O::Unit)
            }
            R::StopDictation => {
                self.send_action(Action::Stop).await?;
                Ok(O::Unit)
            }
            R::CancelOperation => {
                self.cancel_immediate();
                Ok(O::Unit)
            }
            R::GetModelCatalog => Ok(O::ModelCatalog(
                onda_asr::builtin_models()
                    .iter()
                    .map(|model| ModelInfo {
                        id: model.id.into(),
                        name: model.display_name.into(),
                        version: model.version.into(),
                        download_size_bytes: model.expected_size_bytes as f64,
                        memory_bytes: model.estimated_memory_bytes as f64,
                        license: model.license.into(),
                        english_only: model.english_only,
                        sha256: model.sha256.into(),
                    })
                    .collect(),
            )),
            R::GetInstalledModels => Ok(O::InstalledModels(
                lock(&self.models)?
                    .installed()
                    .map_err(errors::asr)?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            )),
            R::StartModelDownload { model_id } => {
                self.start_download(model_id)?;
                Ok(O::Unit)
            }
            R::CancelModelDownload { model_id } => {
                if let Some(cancellation) = lock(&self.downloads)?.get(&model_id) {
                    cancellation.cancel();
                }
                Ok(O::Unit)
            }
            R::UninstallModel { model_id } => {
                if lock(&self.state)?.is_active() {
                    return Err(AppError::new(
                        ErrorCode::Busy,
                        "Wait for the operation before removing a model.",
                        true,
                    ));
                }
                let manager = lock(&self.models)?.clone();
                manager.uninstall(&model_id).await.map_err(errors::asr)?;
                Ok(O::Unit)
            }
            R::StartChatgptSignIn { profile_id } => {
                self.start_sign_in(profile_id, false)?;
                Ok(O::Unit)
            }
            R::EnableChatgptPlanUsage { profile_id } => {
                self.start_sign_in(Some(profile_id), true)?;
                Ok(O::Unit)
            }
            R::CancelChatgptSignIn => {
                if let Some(cancellation) = lock(&self.oauth)?.as_ref() {
                    cancellation.cancel();
                }
                Ok(O::Unit)
            }
            R::ListChatgptProfiles => Ok(O::Profiles(
                self.cloud()?
                    .list_profiles()
                    .await
                    .map_err(errors::cloud)?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            )),
            R::SelectChatgptProfile { profile_id } => {
                self.cancel().await?;
                self.cloud()?
                    .select_profile(&profile_id)
                    .await
                    .map_err(errors::cloud)?;
                self.event(RuntimeEvent::OauthChanged {
                    status: "selected".into(),
                });
                Ok(O::Unit)
            }
            R::RemoveChatgptProfile { profile_id } => {
                self.cancel().await?;
                let removed = self
                    .cloud()?
                    .remove_profile(&profile_id)
                    .await
                    .map_err(errors::cloud)?;
                self.event(RuntimeEvent::OauthChanged {
                    status: "removed".into(),
                });
                Ok(O::SignOut {
                    remote_revocation_confirmed: removed.remote_revocation_confirmed,
                })
            }
            R::SignOutChatgptProfile { profile_id } => {
                self.cancel().await?;
                let signed_out = self
                    .cloud()?
                    .sign_out(&profile_id)
                    .await
                    .map_err(errors::cloud)?;
                self.event(RuntimeEvent::OauthChanged {
                    status: "signedOut".into(),
                });
                Ok(O::SignOut {
                    remote_revocation_confirmed: signed_out.remote_revocation_confirmed,
                })
            }
            R::ListAvailableModels => {
                let models = self
                    .cloud()?
                    .list_models(None)
                    .await
                    .map_err(errors::cloud)?;
                let recommended = super::recommended_model(&models).map(|model| model.slug.clone());
                Ok(O::CloudModels(
                    models
                        .into_iter()
                        .map(|model| CloudModel {
                            recommended: recommended.as_deref() == Some(model.slug.as_str()),
                            slug: model.slug,
                            name: model.display_name,
                        })
                        .collect(),
                ))
            }
            R::SetChatgptModel { profile_id, slug } => {
                self.cloud()?
                    .set_preferred_model(&profile_id, &slug)
                    .await
                    .map_err(errors::cloud)?;
                Ok(O::Unit)
            }
            R::PreviewTransform {
                source,
                transform_id,
            } => Ok(O::Result(
                self.preview_transform(source, transform_id).await?,
            )),
            R::TransformSelection { transform_id } => {
                self.send_action(Action::Transform { transform_id }).await?;
                Ok(O::Unit)
            }
            R::RetryResult => {
                self.send_action(Action::Retry).await?;
                Ok(O::Unit)
            }
            R::RetryTransformation => Ok(O::Result(self.retry_transformation().await?)),
            R::UndoLastTransform => {
                self.send_action(Action::Undo).await?;
                Ok(O::Unit)
            }
            R::CopyResult => {
                let saved = lock(&self.result)?
                    .as_ref()
                    .map(|result| result.result.clone())
                    .ok_or_else(|| AppError::invalid("No result is available to copy."))?;
                copy_text(saved.output.clone()).await?;
                if let Some(current) = lock(&self.result)?.as_mut() {
                    if current.result.operation_id == saved.operation_id
                        && current.result.output == saved.output
                    {
                        current.copied = true;
                    }
                }
                Ok(O::Unit)
            }
            R::ClearResult => {
                self.cancel_immediate();
                *lock(&self.result)? = None;
                *lock(&self.proposal)? = None;
                Ok(O::Unit)
            }
            R::AcceptLearning { proposal_id } => {
                let _dictionary = self.dictionary_gate.lock().await;
                let proposal = lock(&self.proposal)?
                    .clone()
                    .filter(|proposal| proposal.id == proposal_id)
                    .ok_or_else(|| AppError::invalid("Learning proposal expired."))?;
                let word = DictionaryEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: DictionaryKind::Replacement,
                    phrase: proposal.original,
                    replacement: Some(proposal.corrected),
                    enabled: true,
                    starred: false,
                    source: EntrySource::AutoLearned,
                };
                lock(&self.store)?.save_dictionary_entry(&word)?;
                self.rebuild_dictionary().await?;
                *lock(&self.proposal)? = None;
                Ok(O::Unit)
            }
            R::ExportDiagnostics => Ok(O::Text(self.diagnostics()?)),
            R::GetUsage => Ok(O::Usage(self.usage_report(MAX_REPORTED_USAGE_DAYS)?)),
            R::OpenMainWindow { section } => {
                if let Some(section) = section.as_deref() {
                    if section.is_empty()
                        || section.len() > 48
                        || !section
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c == '/' || c == '-')
                    {
                        return Err(AppError::invalid("Unknown section."));
                    }
                }
                self.pill.hide_now(&self.app);
                let created = crate::show_main_at(&self.app, section.as_deref());
                if let Some(section) = section.filter(|_| !created) {
                    self.event(RuntimeEvent::Navigate { section });
                }
                Ok(O::Unit)
            }
            R::CopySuggestion => {
                let text = lock(&self.result)?
                    .as_ref()
                    .and_then(|saved| saved.result.suggestion.clone())
                    .ok_or_else(|| AppError::invalid("The AI version is no longer available."))?;
                copy_text(text).await?;
                Ok(O::Unit)
            }
            R::DismissPill => {
                self.pill.hide_now(&self.app);
                Ok(O::Unit)
            }
            R::DismissError { operation_id } => {
                if lock(&self.state)?.dismiss_error(&operation_id) {
                    self.pill.hide_now(&self.app);
                    self.emit_state();
                }
                Ok(O::Unit)
            }
            R::GetHistory => Ok(O::History(lock(&self.history)?.iter().cloned().collect())),
            R::CopyHistorySuggestion { id } => {
                let text = lock(&self.history)?
                    .iter()
                    .find(|entry| entry.id == id)
                    .and_then(|entry| entry.suggestion.clone())
                    .ok_or_else(|| AppError::invalid("That history entry is no longer kept."))?;
                copy_text(text).await?;
                Ok(O::Unit)
            }
            R::CopyHistoryEntry { id } => {
                let text = lock(&self.history)?
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.output.clone())
                    .ok_or_else(|| AppError::invalid("That history entry is no longer kept."))?;
                copy_text(text.clone()).await?;
                if let Some(current) = lock(&self.result)?.as_mut() {
                    if current.result.operation_id == id && current.result.output == text {
                        current.copied = true;
                    }
                }
                Ok(O::Unit)
            }
            R::DeleteHistoryEntry { id } => {
                lock(&self.history)?.retain(|entry| entry.id != id);
                lock(&self.store)?.delete_history(&id)?;
                self.forget_result(Some(&id))?;
                self.event(RuntimeEvent::HistoryReset);
                Ok(O::Unit)
            }
            R::TeachWord { duration_ms } => Ok(O::TeachTake(self.teach_word(duration_ms).await?)),
            R::GetProfile => Ok(O::Profile(self.profile()?)),
            R::SetProfileName { name } => Ok(O::Profile(self.set_profile_name(name.as_deref())?)),
            R::ChooseProfilePicture => Ok(O::Profile(self.choose_profile_picture().await?)),
            R::RemoveProfilePicture => Ok(O::Profile(self.remove_profile_picture()?)),
            R::SaveStatsImage { png_base64 } => Ok(O::SavedFile {
                path: self.save_stats_image(&png_base64).await?,
            }),
            R::ListProviderModels { provider } => Ok(O::CloudModels(
                self.provider_models(provider)
                    .await?
                    .into_iter()
                    .map(|model| CloudModel {
                        slug: model.id,
                        name: model.display_name,
                        recommended: false,
                    })
                    .collect(),
            )),
            R::SaveProviderKey { provider, key } => {
                self.save_provider_key(provider, key).await?;
                Ok(O::Unit)
            }
            R::RemoveProviderKey { provider } => {
                self.remove_provider_key(provider).await?;
                Ok(O::Unit)
            }
            R::GetResourceUsage => Ok(O::Resources(self.resource_usage().await?)),
            R::ClearHistory => {
                lock(&self.history)?.clear();
                lock(&self.store)?.prune_history(i64::MAX)?;
                self.forget_result(None)?;
                self.event(RuntimeEvent::HistoryReset);
                Ok(O::Unit)
            }
            R::CheckUpdate => Ok(O::Update(self.check_update().await?)),
            R::InstallUpdate => {
                self.install_update().await?;
                Ok(O::Unit)
            }
        }
    }

    /// Deleted history should not linger as the visible result; an operation in flight keeps it.
    fn forget_result(&self, id: Option<&str>) -> AppResult<()> {
        let state = lock(&self.state)?;
        if state.is_active() {
            return Ok(());
        }
        let mut result = lock(&self.result)?;
        if result
            .as_ref()
            .is_some_and(|saved| id.is_none_or(|id| saved.result.operation_id == id))
        {
            *result = None;
        }
        Ok(())
    }

    async fn rebuild_dictionary(&self) -> AppResult<()> {
        let (entries, snippets) = {
            let store = lock(&self.store)?;
            (store.dictionary()?, store.snippets()?)
        };
        let index =
            tokio::task::spawn_blocking(move || DictionaryIndex::build(&entries, &snippets))
                .await
                .map_err(|_| {
                    AppError::new(
                        ErrorCode::Internal,
                        "Dictionary index worker stopped.",
                        true,
                    )
                })??;
        *lock(&self.index)? = index;
        Ok(())
    }

    async fn snapshot(&self) -> AppResult<Snapshot> {
        let (profiles, cloud_error) = match self.cloud() {
            Ok(cloud) => match cloud.list_profiles().await {
                Ok(profiles) => (profiles.into_iter().map(Into::into).collect(), None),
                Err(error) => (Vec::new(), Some(errors::cloud(error))),
            },
            Err(error) => (Vec::new(), Some(error)),
        };
        let settings = lock(&self.store)?.settings()?;
        let selected_model_id = self
            .selected_model(&settings)
            .ok()
            .map(|model| model.id.to_owned());
        Ok(Snapshot {
            app_name: onda_core::APP_NAME.into(),
            version: onda_core::APP_VERSION.into(),
            contract_version: CONTRACT_VERSION,
            state: lock(&self.state)?.snapshot(),
            settings,
            selected_model_id,
            result: lock(&self.result)?
                .as_ref()
                .map(|result| result.result.clone()),
            hardware: self.hardware_report(),
            installed_models: lock(&self.models)?
                .installed()
                .map_err(errors::asr)?
                .into_iter()
                .map(Into::into)
                .collect(),
            profiles,
            cloud_error,
            shortcut_status: lock(&self.shortcut_status)?.clone(),
            updater_configured: updater_configured(),
            platform: std::env::consts::OS.into(),
            provider_keys: self.provider_keys(),
        })
    }

    fn hardware_report(&self) -> HardwareReport {
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
            vulkan_worker_installed: self
                .worker_directory
                .join("onda-asr-vulkan-worker.exe")
                .is_file(),
        }
    }

    /// One take of a word for the dictionary: record, then transcribe with dictation's model,
    /// language and vocabulary prompt. Audio and text stay in memory; the user decides which
    /// misrecognitions become replacements.
    async fn teach_word(self: &Arc<Self>, duration_ms: u32) -> AppResult<TeachTake> {
        if !(1000..=6000).contains(&duration_ms) {
            return Err(AppError::invalid("A teaching take must last 1–6 seconds."));
        }
        let settings = lock(&self.store)?.settings()?;
        let model = self.selected_model(&settings)?;
        let installed = lock(&self.models)?.installed().map_err(errors::asr)?;
        require_installed_model(model, &installed)?;
        {
            let state = lock(&self.state)?;
            self.ensure_not_updating()?;
            if state.is_active() || self.microphone_testing.swap(true, Ordering::AcqRel) {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "Microphone is in use.",
                    true,
                ));
            }
        }
        let result = async {
            let device_id = settings.microphone_id.clone();
            let service = self.clone();
            tokio::task::spawn_blocking(move || {
                lock(&service.capture)?
                    .start(CaptureOptions {
                        device_id,
                        max_duration_seconds: 15,
                    })
                    .map_err(errors::platform)
            })
            .await
            .map_err(|_| {
                AppError::new(ErrorCode::Internal, "Microphone worker stopped.", true)
            })??;
            self.stream_levels(|service| service.microphone_testing.load(Ordering::Acquire));
            tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;
            let service = self.clone();
            let audio = tokio::task::spawn_blocking(move || {
                lock(&service.capture)?.stop().map_err(errors::platform)
            })
            .await
            .map_err(|_| {
                AppError::new(ErrorCode::Internal, "Microphone worker stopped.", true)
            })??;
            if audio.samples.is_empty() || audio.diagnostics.mostly_silent {
                return Err(AppError::new(
                    ErrorCode::NoSpeech,
                    "Onda didn't hear anything. Say the word a little louder or closer to the microphone.",
                    true,
                ));
            }
            let cancellation = CancellationToken::new();
            let models = lock(&self.models)?.clone();
            let path = models
                .verified_model_path(model.id, &cancellation)
                .await
                .map_err(errors::asr)?;
            let options = self.transcription_options(&settings, path, "")?;
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
            let written = lock(&self.index)?.normalize(&heard)?;
            Ok(TeachTake { heard, written })
        }
        .await;
        self.microphone_testing.store(false, Ordering::Release);
        result
    }

    async fn test_microphone(self: &Arc<Self>, duration_ms: u32) -> AppResult<MicrophoneReport> {
        if !(500..=10000).contains(&duration_ms) {
            return Err(AppError::invalid(
                "Microphone test must last 0.5–10 seconds.",
            ));
        }
        {
            let state = lock(&self.state)?;
            self.ensure_not_updating()?;
            if state.is_active() || self.microphone_testing.swap(true, Ordering::AcqRel) {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "Microphone is in use.",
                    true,
                ));
            }
        }
        let result = async {
            let settings = lock(&self.store)?.settings()?;
            let service = self.clone();
            tokio::task::spawn_blocking(move || {
                lock(&service.capture)?
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
            self.stream_levels(|service| service.microphone_testing.load(Ordering::Acquire));
            tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;
            let service = self.clone();
            let audio = tokio::task::spawn_blocking(move || {
                lock(&service.capture)?.stop().map_err(errors::platform)
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

    fn start_download(self: &Arc<Self>, id: String) -> AppResult<()> {
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
        let service = self.clone();
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
            if let Ok(mut downloads) = lock(&service.downloads) {
                downloads.remove(&id);
            }
            if let Err(error) = result {
                service.domain(AppEvent::Error(errors::asr(error)));
            }
        });
        Ok(())
    }

    fn start_sign_in(
        self: &Arc<Self>,
        profile_id: Option<String>,
        reconsent: bool,
    ) -> AppResult<()> {
        let cloud = self.cloud()?.clone();
        let cancellation = CancellationToken::new();
        {
            let mut oauth = lock(&self.oauth)?;
            if oauth.is_some() {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "A sign-in attempt is already active.",
                    true,
                ));
            }
            *oauth = Some(cancellation.clone());
        }
        let service = self.clone();
        self.event(RuntimeEvent::OauthChanged {
            status: "authorizing".into(),
        });
        tauri::async_runtime::spawn(async move {
            let result = if reconsent {
                match profile_id {
                    Some(id) => cloud.enable_plan_usage(id, cancellation).await,
                    None => Err(onda_cloud::CloudError::UnknownProfile),
                }
            } else {
                cloud.sign_in(profile_id, cancellation).await
            };
            if let Ok(mut oauth) = lock(&service.oauth) {
                *oauth = None;
            }
            match result {
                Ok(_) => service.event(RuntimeEvent::OauthChanged {
                    status: "connected".into(),
                }),
                Err(error) => {
                    service.event(RuntimeEvent::OauthChanged {
                        status: "failed".into(),
                    });
                    service.domain(AppEvent::Error(errors::cloud(error)));
                }
            }
        });
        Ok(())
    }

    async fn preview_transform(
        self: &Arc<Self>,
        source: String,
        transform_id: String,
    ) -> AppResult<OperationResult> {
        onda_core::text::validate_source(&source, None)?;
        let transform = lock(&self.store)?
            .transforms()?
            .into_iter()
            .find(|transform| transform.id == transform_id)
            .ok_or_else(|| AppError::invalid("Unknown transform."))?;
        let settings = lock(&self.store)?.settings()?;
        let id = self.begin_operation(OperationMode::Transform)?;
        let cancellation = CancellationToken::new();
        let active = ActiveOperation {
            id: id.clone(),
            target: TargetSnapshot {
                window_handle: 0,
                process_id: 0,
                process_created_at: 0,
                executable_name: String::new(),
                focused_runtime_id: Vec::new(),
                is_password: false,
                integrity_level: 0,
                focused_control: Default::default(),
            },
            mode: OperationMode::Transform,
            selection: None,
            context: String::new(),
            settings,
            cancellation,
            // Home shows a tried rewrite as the last result, so History keeps it too.
            history: true,
        };
        self.set_active(active.clone())?;
        let original = OperationResult {
            command_text: None,
            operation_id: id.clone(),
            original: source.clone(),
            output: source.clone(),
            inserted: false,
            fallback_used: false,
            diff: onda_core::transforms::diff(&source, &source),
            suggestion: None,
        };
        {
            let state = lock(&self.state)?;
            if !state.matches(&id) {
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "Operation cancelled.",
                    false,
                ));
            }
            *lock(&self.result)? = Some(SavedResult {
                copied: false,
                failed: false,
                retry: None,
                result: original,
                target: None,
                selection: None,
                after_document: None,
                shortcut: onda_platform::PasteShortcut::ControlV,
            });
        }
        self.emit_state();
        self.set_retry(
            &id,
            TransformRetry {
                instructions: transform_instruction(&transform.instruction),
                preferred_model: transform.model_preference.clone(),
                preserve_tokens: transform.preserve_protected_tokens,
                command_text: None,
            },
        )?;
        let result: AppResult<OperationResult> = async {
            let output = self
                .cloud_text(
                    &active,
                    &source,
                    &transform_instruction(&transform.instruction),
                    transform.model_preference.as_deref(),
                )
                .await?;
            if transform.preserve_protected_tokens {
                self.check_enhancement(&active, &source, &output, &[], false)?;
            }
            let result = OperationResult {
                command_text: None,
                operation_id: id.clone(),
                original: source.clone(),
                output: output.clone(),
                inserted: false,
                fallback_used: false,
                diff: onda_core::transforms::diff(&source, &output),
                suggestion: None,
            };
            let route = {
                let mut state = lock(&self.state)?;
                if !state.matches(&id) {
                    return Err(AppError::new(
                        ErrorCode::Cancelled,
                        "Operation cancelled.",
                        false,
                    ));
                }
                let retry = lock(&self.result)?
                    .as_ref()
                    .and_then(|saved| saved.retry.clone());
                *lock(&self.result)? = Some(SavedResult {
                    copied: false,
                    failed: false,
                    retry,
                    result: result.clone(),
                    target: None,
                    selection: None,
                    after_document: None,
                    shortcut: onda_platform::PasteShortcut::ControlV,
                });
                let route = state.snapshot().route;
                state.transition(&id, Phase::Completed)?;
                self.domain(AppEvent::ResultReady(result.clone()));
                route
            };
            self.record_history(&active, &result, route);
            self.emit_state();
            Ok(result)
        }
        .await;
        if let Err(error) = &result {
            self.fail(&id, error.clone());
        }
        result
    }

    async fn retry_transformation(self: &Arc<Self>) -> AppResult<OperationResult> {
        let saved = lock(&self.result)?
            .clone()
            .ok_or_else(|| AppError::invalid("No transform is available to retry."))?;
        if saved.result.inserted {
            return Err(AppError::invalid(
                "This transform was already inserted. Select the text and trigger a new transform.",
            ));
        }
        let retry = saved.retry.ok_or_else(|| {
            AppError::invalid("This result has no cloud transformation to retry.")
        })?;
        let settings = lock(&self.store)?.settings()?;
        let id = self.begin_operation(OperationMode::Transform)?;
        let target = saved.target.clone().unwrap_or(TargetSnapshot {
            window_handle: 0,
            process_id: 0,
            process_created_at: 0,
            executable_name: String::new(),
            focused_runtime_id: Vec::new(),
            is_password: false,
            integrity_level: 0,
            focused_control: Default::default(),
        });
        let active = ActiveOperation {
            id: id.clone(),
            target,
            mode: OperationMode::Transform,
            selection: saved.selection,
            context: String::new(),
            settings,
            cancellation: CancellationToken::new(),
            // A retry asks ChatGPT again, so its new text is kept like the first attempt's.
            history: true,
        };
        self.set_active(active.clone())?;
        self.save_result(
            &active,
            &saved.result.original,
            &saved.result.original,
            false,
            false,
            None,
        )?;
        self.set_retry(&id, retry.clone())?;
        if saved.target.is_none() {
            let state = lock(&self.state)?;
            if state.matches(&id) {
                if let Some(result) = lock(&self.result)?.as_mut() {
                    result.target = None;
                }
            }
        }
        self.emit_state();
        let started = Instant::now();
        let result: AppResult<OperationResult> = async {
            let output = self
                .cloud_text(
                    &active,
                    &saved.result.original,
                    &retry.instructions,
                    retry.preferred_model.as_deref(),
                )
                .await?;
            if retry.preserve_tokens {
                self.check_enhancement(&active, &saved.result.original, &output, &[], false)?;
            }
            self.save_result(&active, &saved.result.original, &output, false, false, None)?;
            if saved.target.is_some() {
                let remaining = FOCUS_RETURN_DELAY.saturating_sub(started.elapsed());
                tokio::time::sleep(remaining).await;
                self.transition(&id, Phase::Inserting)?;
                self.insert(&active, &output, false).await?;
            } else {
                let state = lock(&self.state)?;
                if state.matches(&id) {
                    if let Some(saved) = lock(&self.result)?.as_mut() {
                        saved.target = None;
                    }
                }
            }
            let completed = {
                let mut state = lock(&self.state)?;
                if !state.matches(&id) {
                    return Err(AppError::new(
                        ErrorCode::Cancelled,
                        "Operation cancelled.",
                        false,
                    ));
                }
                let result = lock(&self.result)?
                    .as_ref()
                    .filter(|saved| saved.result.operation_id == id)
                    .map(|saved| saved.result.clone())
                    .ok_or_else(|| AppError::invalid("Transformation result expired."))?;
                state.transition(&id, Phase::Completed)?;
                result
            };
            self.emit_state();
            Ok(completed)
        }
        .await;
        if let Err(error) = &result {
            self.fail(&id, error.clone());
        }
        result
    }

    async fn replay_debug_fixture(self: &Arc<Self>) -> AppResult<OperationResult> {
        #[cfg(all(windows, debug_assertions))]
        {
            if crate::debug_smoke_directory().is_none() {
                return Err(AppError::invalid(
                    "Fixture replay requires an isolated debug smoke profile.",
                ));
            }
            let window = tokio::task::spawn_blocking(
                onda_platform::debug_fixture::OwnedEditWindow::create,
            )
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorCode::Internal,
                    "Fixture target could not start.",
                    false,
                )
            })?
            .map_err(|error| {
                tracing::warn!(subsystem="debugFixture",detail=%error,"owned target setup failed");
                errors::platform(error)
            })?;
            let target = window.snapshot().map_err(|error|{tracing::warn!(subsystem="debugFixture",detail=%error,"owned target snapshot failed");errors::platform(error)})?;
            let reader = hound::WavReader::new(std::io::Cursor::new(include_bytes!(
                "../../../tests/fixtures/asr/jfk.wav"
            )))
            .map_err(|_| AppError::invalid("Bundled fixture audio is invalid."))?;
            let samples: Vec<f32> = reader
                .into_samples::<i16>()
                .map(|sample| {
                    sample
                        .map(|sample| f32::from(sample) / (f32::from(i16::MAX) + 1.0))
                        .map_err(|_| AppError::invalid("Bundled fixture audio is invalid."))
                })
                .collect::<AppResult<_>>()?;
            let mut settings = lock(&self.store)?.settings()?;
            settings.execution_preference = ExecutionPreference::CpuOnly;
            settings.quality_profile = QualityProfile::Custom;
            settings.model_id = Some("whisper-tiny-en".into());
            settings.language = Some("en".into());
            settings.enhancement_enabled = false;
            settings.context_enabled = false;
            settings.learning_enabled = false;
            let id = self.begin_operation(OperationMode::Dictation)?;
            let active = ActiveOperation {
                id: id.clone(),
                target,
                mode: OperationMode::Dictation,
                selection: None,
                context: String::new(),
                settings,
                cancellation: CancellationToken::new(),
                // A real dictation through the full pipeline, in the isolated smoke profile only.
                history: true,
            };
            self.set_active(active.clone())?;
            self.transition(&id, Phase::FinalizingAudio)?;
            self.transition(&id, Phase::Transcribing)?;
            if let Err(error) = self.process_audio(active, samples).await {
                self.fail(&id, error.clone());
                return Err(error);
            }
            let actual = window.read_text().map_err(errors::platform)?;
            let result = lock(&self.result)?
                .as_ref()
                .filter(|saved| saved.result.operation_id == id)
                .map(|saved| saved.result.clone())
                .ok_or_else(|| AppError::invalid("Fixture result expired."))?;
            let normalized = actual.to_lowercase();
            if !result.inserted
                || !normalized.contains("my fellow americans")
                || !normalized.contains("what you can do for your country")
            {
                return Err(AppError::new(
                    ErrorCode::Inference,
                    "Fixture speech did not reach the controlled Windows edit target.",
                    false,
                ));
            }
            Ok(result)
        }
        #[cfg(not(all(windows, debug_assertions)))]
        {
            Err(AppError::invalid(
                "Fixture replay is unavailable in production builds.",
            ))
        }
    }

    fn diagnostics(&self) -> AppResult<String> {
        let settings = lock(&self.store)?.settings()?;
        let payload = serde_json::json!({"app":onda_core::APP_NAME,"version":onda_core::APP_VERSION,"contractVersion":CONTRACT_VERSION,"hardware":self.hardware_report(),"preferences":{"executionPreference":settings.execution_preference,"qualityProfile":settings.quality_profile,"contextEnabled":settings.context_enabled,"enhancementEnabled":settings.enhancement_enabled,"learningEnabled":settings.learning_enabled},"shortcutStatus":lock(&self.shortcut_status)?.clone(),"operationError":lock(&self.state)?.snapshot().error,"metrics":lock(&self.metrics)?.iter().cloned().collect::<Vec<_>>()});
        serde_json::to_string_pretty(&payload).map_err(|_| {
            AppError::new(
                ErrorCode::Internal,
                "Diagnostics could not be encoded.",
                true,
            )
        })
    }

    fn updater(&self) -> AppResult<tauri_plugin_updater::Updater> {
        use tauri_plugin_updater::UpdaterExt;
        let endpoint = option_env!("ONDA_UPDATE_ENDPOINT").ok_or_else(|| {
            AppError::invalid("This build has no signed update channel configured.")
        })?;
        let key = option_env!("ONDA_UPDATE_PUBLIC_KEY").ok_or_else(|| {
            AppError::invalid("This build has no update verification key configured.")
        })?;
        if !endpoint.starts_with("https://") {
            return Err(AppError::invalid("Update channel must use HTTPS."));
        }
        self.app
            .updater_builder()
            .pubkey(key)
            .timeout(Duration::from_secs(600))
            .endpoints(vec![endpoint.parse().map_err(|_| {
                AppError::invalid("Update channel URL is invalid.")
            })?])
            .map_err(|_| AppError::invalid("Update channel configuration failed."))?
            .build()
            .map_err(|_| AppError::new(ErrorCode::Internal, "Updater could not start.", true))
    }
    async fn check_update(&self) -> AppResult<UpdateInfo> {
        let update =
            self.updater()?.check().await.map_err(|_| {
                AppError::new(ErrorCode::Download, "Signed update check failed.", true)
            })?;
        Ok(UpdateInfo {
            available: update.is_some(),
            version: update.map(|update| update.version),
        })
    }
    async fn install_update(self: &Arc<Self>) -> AppResult<()> {
        let _reservation = {
            let state = lock(&self.state)?;
            self.ensure_recovery_resolved()?;
            if state.is_active()
                || self.microphone_testing.load(Ordering::Acquire)
                || self.updating.swap(true, Ordering::AcqRel)
            {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "Finish the current operation before updating.",
                    true,
                ));
            }
            UpdateReservation(&self.updating)
        };
        let update = self
            .updater()?
            .check()
            .await
            .map_err(|_| AppError::new(ErrorCode::Download, "Signed update check failed.", true))?
            .ok_or_else(|| AppError::invalid("No update is available."))?;
        let bytes = update.download(|_, _| {}, || {}).await.map_err(|_| {
            AppError::new(
                ErrorCode::Download,
                "The update failed download or signature verification.",
                true,
            )
        })?;
        self.shutdown();
        update.install(bytes).map_err(|_| {
            AppError::new(
                ErrorCode::Download,
                "The verified update could not be installed. Restart the application.",
                true,
            )
        })?;
        self.app.restart();
    }
}

pub(super) fn updater_configured() -> bool {
    option_env!("ONDA_UPDATE_ENDPOINT").is_some() && option_env!("ONDA_UPDATE_PUBLIC_KEY").is_some()
}

struct UpdateReservation<'a>(&'a AtomicBool);
impl Drop for UpdateReservation<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Put text on the clipboard off the async runtime, with the platform's privacy markers.
async fn copy_text(text: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || onda_platform::copy_text_to_clipboard(&text))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Copy worker stopped.", true))?
        .map_err(errors::platform)
}

#[cfg(windows)]
fn validate_model_directory(path: &std::path::Path) -> AppResult<()> {
    let mut components = path.components();
    let valid_prefix = matches!(components.next(),Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_)));
    if !path.is_absolute() || !valid_prefix || components.count() < 2 {
        return Err(AppError::invalid(
            "Choose an absolute directory on a local Windows drive, below the drive root.",
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn validate_model_directory(path: &std::path::Path) -> AppResult<()> {
    use std::path::Component;
    let below_root = path
        .components()
        .any(|component| matches!(component, Component::Normal(_)));
    let climbs = path
        .components()
        .any(|component| matches!(component, Component::ParentDir));
    if !path.is_absolute() || !below_root || climbs {
        return Err(AppError::invalid(
            "Choose an absolute folder below the root directory.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(windows))]
    #[test]
    fn managed_model_location_refuses_relative_climbing_and_root_paths() {
        for path in ["models", "/", "/home/example/../../etc", "./models"] {
            assert!(
                validate_model_directory(std::path::Path::new(path)).is_err(),
                "{path}"
            );
        }
        assert!(validate_model_directory(std::path::Path::new("/home/example/models")).is_ok());
    }
    #[cfg(windows)]
    #[test]
    fn managed_model_location_refuses_unc_relative_and_drive_roots() {
        for path in ["..\\models", "C:\\", "\\\\server\\share\\models"] {
            assert!(validate_model_directory(std::path::Path::new(path)).is_err());
        }
        assert!(
            validate_model_directory(std::path::Path::new("C:\\Users\\Example\\Models")).is_ok()
        );
    }
}
