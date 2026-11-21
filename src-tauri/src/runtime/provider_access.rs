//! Owns configured provider transports, protected keys and model discovery.
use super::*;

pub(super) struct ProviderAccess {
    pub(super) providers: Option<ProviderClient>,
    pub(super) keys: KeyStore,
    pub(super) provider_models: Mutex<Option<providers::CachedProviderModels>>,
}

impl ProviderAccess {
    pub(super) fn new(profile: &std::path::Path) -> Self {
        Self {
            providers: ProviderClient::new().ok(),
            keys: KeyStore::new(profile.to_owned(), onda_core::APP_IDENTIFIER),
            provider_models: Mutex::default(),
        }
    }
}
