//! Owns the installed model directory and cancellable downloads.
use super::*;

pub(super) struct ModelLibrary {
    pub(super) models: Mutex<ModelManager>,
    pub(super) downloads: Mutex<HashMap<String, CancellationToken>>,
}

impl ModelLibrary {
    pub(super) fn open(settings: &Settings, profile: &std::path::Path) -> AppResult<Self> {
        let directory = settings
            .model_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| profile.join("models"));
        Ok(Self {
            models: Mutex::new(ModelManager::new(directory).map_err(errors::asr)?),
            downloads: Mutex::default(),
        })
    }
}
