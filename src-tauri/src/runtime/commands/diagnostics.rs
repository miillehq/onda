use super::*;

impl Performance {
    pub(in crate::runtime) fn diagnostics(&self, host: &Runtime) -> AppResult<String> {
        let settings = lock(&host.data.store)?.settings()?;
        let payload = serde_json::json!({"app":onda_core::APP_NAME,"version":onda_core::APP_VERSION,"contractVersion":CONTRACT_VERSION,"hardware":host.recognition.hardware_report(),"preferences":{"executionPreference":settings.execution_preference,"qualityProfile":settings.quality_profile,"contextEnabled":settings.context_enabled,"enhancementEnabled":settings.enhancement_enabled,"learningEnabled":settings.learning_enabled},"shortcutStatus":lock(&host.shortcuts.shortcut_status)?.clone(),"operationError":lock(&host.operations.state)?.snapshot().error,"metrics":lock(&self.metrics)?.iter().cloned().collect::<Vec<_>>()});
        serde_json::to_string_pretty(&payload).map_err(|_| {
            AppError::new(
                ErrorCode::Internal,
                "Diagnostics could not be encoded.",
                true,
            )
        })
    }
}
