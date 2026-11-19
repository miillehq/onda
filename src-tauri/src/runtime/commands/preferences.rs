use super::*;
use ApiResponse as O;

pub(super) async fn update_settings(
    service: &Arc<Runtime>,
    settings: Box<Settings>,
) -> AppResult<ApiResponse> {
    let _configuration = service.data.configuration_gate.lock().await;
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
    service.recognition.selected_model(&settings)?;
    let config = shortcut_config(&settings, &lock(&service.data.store)?.transforms()?)?;
    service.operations.cancel(service).await?;
    let current = lock(&service.data.store)?.settings()?;
    if current.launch_at_login != settings.launch_at_login {
        crate::apply_launch_at_login(settings.launch_at_login)?;
    }
    let migrated = migrate_model_directory(service, &current, &settings).await?;
    service.retention.save_preferences(service, &settings)?;
    if let Some(manager) = migrated {
        *lock(&service.model_library.models)? = manager;
    }
    // Restarting the shortcut helper drops shortcuts for a moment; only do it when
    // the shortcuts themselves changed, not for every quality or theme switch.
    let mut helper_config = lock(&service.shortcuts.helper_config)?;
    if *helper_config != config {
        *helper_config = config;
        service
            .shortcuts
            .helper_generation
            .fetch_add(1, Ordering::AcqRel);
    }
    drop(helper_config);
    lock(&service.operations.state)?.clear_no_model_error();
    service
        .pill
        .configure(&service.app, settings.show_pill, settings.play_sounds);
    if current.theme != settings.theme {
        crate::apply_theme(&service.app, settings.theme);
    }
    if current.tray_icon != settings.tray_icon {
        crate::apply_tray_icon(&service.app, settings.tray_icon);
    }
    if (&current.openai_base_url, &current.local_base_url)
        != (&settings.openai_base_url, &settings.local_base_url)
    {
        *lock(&service.rewrites.provider_access.provider_models)? = None;
    }
    service.operations.emit_state(service);
    Ok(O::Settings(settings))
}

pub(super) fn open_main_window(
    service: &Arc<Runtime>,
    section: Option<String>,
) -> AppResult<ApiResponse> {
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
    service.pill.hide_now(&service.app);
    let created = crate::show_main_at(&service.app, section.as_deref());
    if let Some(section) = section.filter(|_| !created) {
        service.event(RuntimeEvent::Navigate { section });
    }
    Ok(O::Unit)
}

pub(super) fn dismiss_pill(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.pill.hide_now(&service.app);
    Ok(O::Unit)
}

pub(super) async fn allow_typing(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.targets.allow_typing().await?;
    Ok(O::Unit)
}

pub(super) async fn stop_typing() -> AppResult<ApiResponse> {
    // On a Mac this opens the Accessibility list, the only place to turn it off.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    tokio::task::spawn_blocking(onda_platform::stop_typing)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Typing worker stopped.", true))?;
    Ok(O::Unit)
}

pub(super) fn restart_shortcuts(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.shortcuts.restart_shortcuts(service)?;
    Ok(O::Unit)
}

pub(super) async fn install_update(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.updates.install_update(service).await?;
    Ok(O::Unit)
}

impl TargetDelivery {
    /// Linux asks the desktop, and a Mac the user, for permission to press paste in other apps.
    pub(in crate::runtime) async fn allow_typing(&self) -> AppResult<()> {
        #[cfg(target_os = "linux")]
        {
            let allowed = tokio::task::spawn_blocking(onda_platform::allow_typing)
                .await
                .map_err(|_| AppError::new(ErrorCode::Internal, "Typing worker stopped.", true))?;
            allowed.map_err(|error| match error {
                onda_platform::PlatformError::Cancelled => AppError::new(
                    ErrorCode::Cancelled,
                    "Typing wasn't allowed. Onda keeps copying dictation for you to paste.",
                    true,
                ),
                onda_platform::PlatformError::UnsupportedPlatform => AppError::new(
                    ErrorCode::UnsupportedControl,
                    "This desktop can't let apps type. Onda keeps copying dictation for you to paste.",
                    false,
                ),
                _ => AppError::new(
                    ErrorCode::Insertion,
                    "The desktop couldn't start typing. Try again.",
                    true,
                ),
            })?;
        }
        // A Mac shows its own dialog and opens the Accessibility list; the switch is there, so
        // typing is usually still off when this returns.
        #[cfg(target_os = "macos")]
        {
            tokio::task::spawn_blocking(onda_platform::allow_typing)
                .await
                .map_err(|_| AppError::new(ErrorCode::Internal, "Typing worker stopped.", true))?
                .map_err(|_| {
                    AppError::new(
                        ErrorCode::Cancelled,
                        "Turn on Onda in System Settings › Privacy & Security › Accessibility. Until then, dictation is copied for you to paste.",
                        true,
                    )
                })?;
        }
        Ok(())
    }
}

async fn migrate_model_directory(
    service: &Arc<Runtime>,
    current: &Settings,
    settings: &Settings,
) -> AppResult<Option<ModelManager>> {
    let migrated = if current.model_directory != settings.model_directory {
        let path = settings
            .model_directory
            .as_ref()
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| {
                service
                    .app
                    .path()
                    .app_local_data_dir()
                    .map(|directory| directory.join("models"))
                    .map_err(|_| AppError::storage())
            })?;
        validate_model_directory(&path)?;
        if !lock(&service.model_library.downloads)?.is_empty() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "Finish model downloads before moving the model directory.",
                true,
            ));
        }
        let manager = lock(&service.model_library.models)?.clone();
        Some(
            manager
                .migrate_to(path, &CancellationToken::new())
                .await
                .map_err(errors::asr)?,
        )
    } else {
        None
    };
    Ok(migrated)
}
