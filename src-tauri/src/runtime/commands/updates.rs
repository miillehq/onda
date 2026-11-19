use super::*;

impl Updates {
    pub(in crate::runtime) fn updater(
        &self,
        host: &Runtime,
    ) -> AppResult<tauri_plugin_updater::Updater> {
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
        host.app
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

    pub(in crate::runtime) async fn check_update(&self, host: &Runtime) -> AppResult<UpdateInfo> {
        let update =
            self.updater(host)?.check().await.map_err(|_| {
                AppError::new(ErrorCode::Download, "Signed update check failed.", true)
            })?;
        Ok(UpdateInfo {
            available: update.is_some(),
            version: update.map(|update| update.version),
        })
    }

    pub(in crate::runtime) async fn install_update(&self, host: &Arc<Runtime>) -> AppResult<()> {
        let _reservation = {
            let state = lock(&host.operations.state)?;
            host.operations.ensure_recovery_resolved(host)?;
            if state.is_active()
                || host.microphone.microphone_testing.load(Ordering::Acquire)
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
            .updater(host)?
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
        host.shutdown();
        update.install(bytes).map_err(|_| {
            AppError::new(
                ErrorCode::Download,
                "The verified update could not be installed. Restart the application.",
                true,
            )
        })?;
        host.app.restart();
    }
}
