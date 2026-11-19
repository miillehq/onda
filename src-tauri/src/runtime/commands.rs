//! IPC commands validate policy, then delegate to native/domain services.
use super::*;
use tauri::Manager;

/// Enough for a year of daily bars without paging.
const MAX_REPORTED_USAGE_DAYS: u32 = 366;

mod accounts;
mod debug_checks;
mod diagnostics;
mod library;
mod preferences;
mod results;
mod rewrites;
mod speech;
mod updates;

impl Runtime {
    pub async fn dispatch(self: &Arc<Self>, request: ApiRequest) -> AppResult<ApiResponse> {
        if self.operations.shutdown.load(Ordering::Acquire) {
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
                debug_checks::complete_debug_smoke(self, passed, checks)
            }
            R::GetSnapshot => Ok(O::Snapshot(Box::new(self.snapshot().await?))),
            R::ReplayDebugFixture => Ok(O::Result(self.replay_debug_fixture().await?)),
            R::GetSettings => Ok(O::Settings(Box::new(lock(&self.data.store)?.settings()?))),
            R::UpdateSettings { settings } => preferences::update_settings(self, settings).await,
            R::GetDictionary => Ok(O::Dictionary(lock(&self.data.store)?.dictionary()?)),
            R::SaveDictionaryEntry { entry } => library::save_dictionary_entry(self, entry).await,
            R::DeleteDictionaryEntry { id } => library::delete_dictionary_entry(self, id).await,
            R::ImportDictionary { json } => library::import_dictionary(self, json).await,
            R::ExportDictionary => Ok(O::Text(lock(&self.data.store)?.export_dictionary()?)),
            R::GetSnippets => Ok(O::Snippets(lock(&self.data.store)?.snippets()?)),
            R::SaveSnippet { snippet } => library::save_snippet(self, snippet).await,
            R::DeleteSnippet { id } => library::delete_snippet(self, id).await,
            R::GetTransforms => Ok(O::Transforms(lock(&self.data.store)?.transforms()?)),
            R::SaveTransform { transform } => library::save_transform(self, transform).await,
            R::DeleteTransform { id } => library::delete_transform(self, id).await,
            R::ListAudioDevices => speech::list_audio_devices().await,
            R::TestMicrophone { duration_ms } => speech::test_microphone(self, duration_ms).await,
            R::StartDictation { command_mode } => {
                results::start_dictation(self, command_mode).await
            }
            R::StopDictation => results::stop_dictation(self).await,
            R::CancelOperation => results::cancel_operation(self),
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
                lock(&self.model_library.models)?
                    .installed()
                    .map_err(errors::asr)?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            )),
            R::StartModelDownload { model_id } => speech::start_model_download(self, model_id),
            R::CancelModelDownload { model_id } => speech::cancel_model_download(self, model_id),
            R::UninstallModel { model_id } => speech::uninstall_model(self, model_id).await,
            R::StartChatgptSignIn { profile_id } => {
                accounts::start_chatgpt_sign_in(self, profile_id)
            }
            R::EnableChatgptPlanUsage { profile_id } => {
                accounts::enable_chatgpt_plan_usage(self, profile_id)
            }
            R::CancelChatgptSignIn => accounts::cancel_chatgpt_sign_in(self),
            R::ListChatgptProfiles => Ok(O::Profiles(
                self.rewrites
                    .accounts
                    .cloud()?
                    .list_profiles()
                    .await
                    .map_err(errors::cloud)?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            )),
            R::SelectChatgptProfile { profile_id } => {
                accounts::select_chatgpt_profile(self, profile_id).await
            }
            R::RemoveChatgptProfile { profile_id } => {
                accounts::remove_chatgpt_profile(self, profile_id).await
            }
            R::SignOutChatgptProfile { profile_id } => {
                accounts::sign_out_chatgpt_profile(self, profile_id).await
            }
            R::ListAvailableModels => accounts::list_available_models(self).await,
            R::SetChatgptModel { profile_id, slug } => {
                accounts::set_chatgpt_model(self, profile_id, slug).await
            }
            R::PreviewTransform {
                source,
                transform_id,
            } => Ok(O::Result(
                self.rewrites
                    .preview_transform(self, source, transform_id)
                    .await?,
            )),
            R::TransformSelection { transform_id } => {
                results::transform_selection(self, transform_id).await
            }
            R::RetryResult => results::retry_result(self).await,
            R::RetryTransformation => {
                Ok(O::Result(self.rewrites.retry_transformation(self).await?))
            }
            R::UndoLastTransform => results::undo_last_transform(self).await,
            R::CopyResult => results::copy_result(self).await,
            R::ClearResult => results::clear_result(self),
            R::AcceptLearning { proposal_id } => library::accept_learning(self, proposal_id).await,
            R::ExportDiagnostics => Ok(O::Text(self.performance.diagnostics(self)?)),
            R::GetUsage => Ok(O::Usage(self.data.usage_report(MAX_REPORTED_USAGE_DAYS)?)),
            R::OpenMainWindow { section } => preferences::open_main_window(self, section),
            R::CopySuggestion => results::copy_suggestion(self).await,
            R::DismissPill => preferences::dismiss_pill(self),
            R::DismissError { operation_id } => results::dismiss_error(self, operation_id),
            R::GetHistory => Ok(O::History(self.retention.history_entries(self)?)),
            R::CopyHistorySuggestion { id } => results::copy_history_suggestion(self, id).await,
            R::CopyHistoryEntry { id } => results::copy_history_entry(self, id).await,
            R::DeleteHistoryEntry { id } => results::delete_history_entry(self, id),
            R::TeachWord { duration_ms } => Ok(O::TeachTake(
                self.recognition.teach_word(self, duration_ms).await?,
            )),
            R::GetProfile => Ok(O::Profile(self.data.profile()?)),
            R::SetProfileName { name } => {
                Ok(O::Profile(self.data.set_profile_name(name.as_deref())?))
            }
            R::ChooseProfilePicture => {
                Ok(O::Profile(self.data.choose_profile_picture(self).await?))
            }
            R::RemoveProfilePicture => Ok(O::Profile(self.data.remove_profile_picture()?)),
            R::SaveStatsImage { png_base64 } => Ok(O::SavedFile {
                path: self.data.save_stats_image(self, png_base64).await?,
            }),
            R::ListProviderModels { provider } => Ok(O::CloudModels(
                self.rewrites
                    .provider_access
                    .provider_models(self, provider)
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
                accounts::save_provider_key(self, provider, key).await
            }
            R::RemoveProviderKey { provider } => {
                accounts::remove_provider_key(self, provider).await
            }
            R::GetResourceUsage => Ok(O::Resources(self.performance.resource_usage(self).await?)),
            R::AllowTyping => preferences::allow_typing(self).await,
            R::StopTyping => preferences::stop_typing().await,
            R::RestartShortcuts => preferences::restart_shortcuts(self),
            R::ClearHistory => results::clear_history(self),
            R::CheckUpdate => Ok(O::Update(self.updates.check_update(self).await?)),
            R::InstallUpdate => preferences::install_update(self).await,
        }
    }

    async fn snapshot(&self) -> AppResult<Snapshot> {
        let (profiles, cloud_error) = match self.rewrites.accounts.cloud() {
            Ok(cloud) => match cloud.list_profiles().await {
                Ok(profiles) => (profiles.into_iter().map(Into::into).collect(), None),
                Err(error) => (Vec::new(), Some(errors::cloud(error))),
            },
            Err(error) => (Vec::new(), Some(error)),
        };
        let settings = lock(&self.data.store)?.settings()?;
        let selected_model_id = self
            .recognition
            .selected_model(&settings)
            .ok()
            .map(|model| model.id.to_owned());
        // Awaited before the locks below, which must not be held across it.
        let typing_access = typing_access().await;
        Ok(Snapshot {
            app_name: onda_core::APP_NAME.into(),
            version: onda_core::APP_VERSION.into(),
            contract_version: CONTRACT_VERSION,
            state: lock(&self.operations.state)?.snapshot(),
            settings,
            selected_model_id,
            result: lock(&self.results.result)?
                .as_ref()
                .map(|result| result.result.clone()),
            hardware: self.recognition.hardware_report(),
            installed_models: lock(&self.model_library.models)?
                .installed()
                .map_err(errors::asr)?
                .into_iter()
                .map(Into::into)
                .collect(),
            profiles,
            cloud_error,
            shortcut_status: lock(&self.shortcuts.shortcut_status)?.clone(),
            updater_configured: updater_configured(),
            platform: std::env::consts::OS.into(),
            provider_keys: self.rewrites.provider_access.provider_keys(),
            typing_access,
        })
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
            let mut settings = lock(&self.data.store)?.settings()?;
            settings.execution_preference = ExecutionPreference::CpuOnly;
            settings.quality_profile = QualityProfile::Custom;
            settings.model_id = Some("whisper-tiny-en".into());
            settings.language = Some("en".into());
            settings.enhancement_enabled = false;
            settings.context_enabled = false;
            settings.learning_enabled = false;
            let id = self
                .operations
                .begin_operation(self, OperationMode::Dictation)?;
            let active = ActiveOperation {
                id: id.clone(),
                target: Some(target),
                mode: OperationMode::Dictation,
                selection: None,
                context: String::new(),
                settings,
                cancellation: CancellationToken::new(),
                // A real dictation through the full pipeline, in the isolated smoke profile only.
                history: true,
                usage: None,
            };
            self.operations.set_active(active.clone())?;
            self.operations
                .transition(self, &id, Phase::FinalizingAudio)?;
            self.operations.transition(self, &id, Phase::Transcribing)?;
            if let Err(error) = self.recognition.process_audio(self, active, samples).await {
                self.operations.fail(self, &id, error.clone());
                return Err(error);
            }
            let actual = window.read_text().map_err(errors::platform)?;
            let result = lock(&self.results.result)?
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
/// "allowed", "notAllowed" or "unavailable": whether dictation may press paste in other apps.
async fn typing_access() -> String {
    #[cfg(target_os = "linux")]
    {
        use onda_platform::TypingAccess as A;
        let access = tokio::task::spawn_blocking(onda_platform::typing_access)
            .await
            .unwrap_or(A::Unavailable);
        match access {
            A::Allowed => "allowed",
            A::NotAllowed => "notAllowed",
            A::Unavailable => "unavailable",
        }
        .into()
    }
    #[cfg(target_os = "macos")]
    {
        use onda_platform::TypingAccess as A;
        match tokio::task::spawn_blocking(onda_platform::typing_access).await {
            Ok(A::Allowed) => "allowed",
            Ok(A::NotAllowed) | Err(_) => "notAllowed",
        }
        .into()
    }
    #[cfg(windows)]
    {
        "allowed".into()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        "unavailable".into()
    }
}

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
