//! AI polish through providers other than Sign in with ChatGPT. Exactly the provider in the
//! settings is used; a failure keeps the local text and never moves to another provider.
use super::*;
use onda_cloud::{ApiKey, Endpoint, KeySlot, ProviderModel, ProviderRequest};

/// How long a provider's model list is reused, like the ChatGPT account list.
const PROVIDER_MODELS_TTL: Duration = Duration::from_secs(10 * 60);

pub(super) struct CachedProviderModels {
    endpoint: Endpoint,
    fetched: Instant,
    models: Vec<ProviderModel>,
}

pub(super) fn key_slot(provider: AiProvider) -> Option<KeySlot> {
    match provider {
        AiProvider::Chatgpt => None,
        AiProvider::OpenaiCompatible => Some(KeySlot::OpenaiCompatible),
        AiProvider::Anthropic => Some(KeySlot::Anthropic),
        AiProvider::Local => Some(KeySlot::Local),
    }
}

pub(super) fn route(provider: AiProvider) -> DataRoute {
    match provider {
        AiProvider::Chatgpt => DataRoute::ChatgptPlan,
        AiProvider::OpenaiCompatible => DataRoute::OpenaiCompatible,
        AiProvider::Anthropic => DataRoute::Anthropic,
        AiProvider::Local => DataRoute::LocalModel,
    }
}

/// The provider's validated address; the network policy itself lives in onda-cloud.
pub(super) fn endpoint(settings: &Settings, provider: AiProvider) -> AppResult<Endpoint> {
    let missing = |what: &str| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("Add the {what} in AI polish."),
            true,
        )
    };
    match provider {
        AiProvider::Chatgpt => Err(AppError::invalid("ChatGPT uses its own sign-in.")),
        AiProvider::Anthropic => Ok(Endpoint::anthropic()),
        AiProvider::OpenaiCompatible => Endpoint::openai_compatible(
            settings
                .openai_base_url
                .as_deref()
                .ok_or_else(|| missing("server address"))?,
        )
        .map_err(|error| errors::provider(provider, None, error)),
        AiProvider::Local => Endpoint::local(
            settings
                .local_base_url
                .as_deref()
                .ok_or_else(|| missing("local server address"))?,
        )
        .map_err(|error| errors::provider(provider, None, error)),
    }
}

fn model_setting(settings: &Settings, provider: AiProvider) -> Option<&str> {
    match provider {
        AiProvider::Chatgpt => None,
        AiProvider::OpenaiCompatible => settings.openai_model.as_deref(),
        AiProvider::Anthropic => settings.anthropic_model.as_deref(),
        AiProvider::Local => settings.local_model.as_deref(),
    }
}

/// Validates the addresses a settings change would save, so a bad URL is refused at once.
pub(super) fn validate_provider_settings(settings: &Settings) -> AppResult<()> {
    if settings.openai_base_url.is_some() {
        endpoint(settings, AiProvider::OpenaiCompatible)?;
    }
    if settings.local_base_url.is_some() {
        endpoint(settings, AiProvider::Local)?;
    }
    Ok(())
}

impl Runtime {
    fn provider_client(&self) -> AppResult<&onda_cloud::ProviderClient> {
        self.providers.as_ref().ok_or_else(|| {
            AppError::new(
                ErrorCode::Cloud,
                "AI providers could not start. Local dictation remains available.",
                false,
            )
        })
    }

    pub(super) fn provider_keys(&self) -> ProviderKeys {
        ProviderKeys {
            openai_compatible: self.keys.has(KeySlot::OpenaiCompatible),
            anthropic: self.keys.has(KeySlot::Anthropic),
            local: self.keys.has(KeySlot::Local),
        }
    }

    /// Keystore calls can block on the desktop keyring (and its unlock prompt).
    async fn load_key(&self, provider: AiProvider) -> AppResult<Option<ApiKey>> {
        let Some(slot) = key_slot(provider) else {
            return Ok(None);
        };
        if !self.keys.has(slot) {
            return Ok(None);
        }
        let keys = self.keys.clone();
        tokio::task::spawn_blocking(move || keys.load(slot))
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Key storage stopped.", true))?
            .map_err(|error| errors::provider(provider, None, error))
    }

    pub(super) async fn save_provider_key(
        &self,
        provider: AiProvider,
        mut raw: String,
    ) -> AppResult<()> {
        let parsed = ApiKey::parse(&raw);
        zeroize::Zeroize::zeroize(&mut raw);
        let key = parsed.map_err(|error| errors::provider(provider, None, error))?;
        let slot =
            key_slot(provider).ok_or_else(|| AppError::invalid("ChatGPT uses its own sign-in."))?;
        let keys = self.keys.clone();
        tokio::task::spawn_blocking(move || keys.save(slot, &key))
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Key storage stopped.", true))?
            .map_err(|error| errors::provider(provider, None, error))?;
        *lock(&self.provider_models)? = None;
        Ok(())
    }

    pub(super) async fn remove_provider_key(&self, provider: AiProvider) -> AppResult<()> {
        let slot =
            key_slot(provider).ok_or_else(|| AppError::invalid("ChatGPT uses its own sign-in."))?;
        let keys = self.keys.clone();
        tokio::task::spawn_blocking(move || keys.remove(slot))
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Key storage stopped.", true))?
            .map_err(|error| errors::provider(provider, None, error))?;
        *lock(&self.provider_models)? = None;
        Ok(())
    }

    /// Lists models for the configured provider; for a local server this is the "is it running?"
    /// check, so an error names what to start.
    pub(super) async fn provider_models(
        &self,
        provider: AiProvider,
    ) -> AppResult<Vec<ProviderModel>> {
        let settings = self.settings()?;
        let endpoint = endpoint(&settings, provider)?;
        if let Some(cached) = lock(&self.provider_models)?.as_ref().filter(|cached| {
            cached.endpoint == endpoint && cached.fetched.elapsed() < PROVIDER_MODELS_TTL
        }) {
            return Ok(cached.models.clone());
        }
        let key = self.load_key(provider).await?;
        let models = self
            .provider_client()?
            .list_models(&endpoint, key.as_ref())
            .await
            .map_err(|error| errors::provider(provider, Some(&endpoint), error))?;
        *lock(&self.provider_models)? = Some(CachedProviderModels {
            endpoint,
            fetched: Instant::now(),
            models: models.clone(),
        });
        Ok(models)
    }

    /// One polish, rewrite or spoken edit through the configured provider.
    pub(super) async fn provider_text(
        self: &Arc<Self>,
        active: &ActiveOperation,
        provider: AiProvider,
        source: &str,
        instructions: &str,
    ) -> AppResult<String> {
        let endpoint = endpoint(&active.settings, provider)?;
        let model = model_setting(&active.settings, provider)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    "Choose a model in AI polish.",
                    true,
                )
            })?
            .to_owned();
        let key = self.load_key(provider).await?;
        if provider == AiProvider::Anthropic && key.is_none() {
            return Err(AppError::new(
                ErrorCode::Unauthorized,
                "Add your Anthropic API key in AI polish.",
                true,
            ));
        }
        lock(&self.state)?.mark_cloud(&active.id, route(provider));
        self.emit_state();
        // Anthropic models that support it get low effort: cleanup needs little deliberation.
        let low_effort = provider == AiProvider::Anthropic
            && lock(&self.provider_models)?
                .as_ref()
                .filter(|cached| cached.endpoint == endpoint)
                .and_then(|cached| cached.models.iter().find(|listed| listed.id == model))
                .is_some_and(|listed| listed.low_effort);
        tracing::info!(subsystem = "ai", operation_id = active.id, provider = ?provider, "text enhancement started");
        let request = ProviderRequest {
            model,
            instructions: instructions.into(),
            source_text: source.into(),
            context_text: (!active.context.is_empty()).then(|| active.context.clone()),
            low_effort,
        };
        let service = self.clone();
        let id = active.id.clone();
        let started = Instant::now();
        let mut first_token = false;
        let result = self
            .provider_client()?
            .respond(
                &endpoint,
                key.as_ref(),
                &request,
                active.cancellation.clone(),
                move |delta| {
                    if !first_token {
                        service.metric(&id, "providerFirstToken", started, true);
                        first_token = true;
                    }
                    service.domain(AppEvent::CloudDelta {
                        operation_id: id.clone(),
                        delta: delta.into(),
                    });
                },
            )
            .await;
        self.metric(&active.id, "providerCompletion", started, result.is_ok());
        result
            .map(|response| response.text)
            .map_err(|error| errors::provider(provider, Some(&endpoint), error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_settings_are_validated_before_they_are_saved() {
        let mut settings = Settings::default();
        assert!(validate_provider_settings(&settings).is_ok());
        settings.local_base_url = Some("http://192.168.1.4:11434/v1".into());
        let error = validate_provider_settings(&settings).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        settings.local_base_url = Some("http://localhost:11434/v1".into());
        settings.openai_base_url = Some("http://api.example.com/v1".into());
        assert!(validate_provider_settings(&settings).is_err());
        settings.openai_base_url = Some("https://api.example.com/v1".into());
        assert!(validate_provider_settings(&settings).is_ok());
    }

    #[test]
    fn each_provider_has_its_own_route_and_key_slot() {
        assert_eq!(route(AiProvider::Chatgpt), DataRoute::ChatgptPlan);
        assert_eq!(route(AiProvider::Local), DataRoute::LocalModel);
        assert_eq!(key_slot(AiProvider::Chatgpt), None);
        assert_eq!(key_slot(AiProvider::Anthropic), Some(KeySlot::Anthropic));
    }

    #[test]
    fn a_missing_model_or_address_asks_for_it() {
        let settings = Settings::default();
        let error = endpoint(&settings, AiProvider::Local).unwrap_err();
        assert!(error.message.contains("local server address"));
        assert_eq!(model_setting(&settings, AiProvider::Anthropic), None);
        assert_eq!(
            endpoint(&settings, AiProvider::Anthropic).unwrap().host(),
            "api.anthropic.com"
        );
    }
}
