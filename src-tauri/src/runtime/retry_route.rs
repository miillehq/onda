//! A rewrite retry keeps its original destination and ChatGPT account. No credential is cached here.
use super::*;

#[derive(Clone)]
pub(super) struct RetryRoute {
    provider: AiProvider,
    address: Option<String>,
    pub(super) account: Arc<Mutex<Option<AccountModel>>>,
}

pub(super) struct AccountModel {
    id: String,
    preference: Option<String>,
    resolved: Option<String>,
}

fn configured_address(settings: &Settings) -> Option<&str> {
    match settings.ai_provider {
        AiProvider::OpenaiCompatible => settings.openai_base_url.as_deref(),
        AiProvider::Local => settings.local_base_url.as_deref(),
        AiProvider::Chatgpt | AiProvider::Anthropic => None,
    }
}

impl RetryRoute {
    pub(super) fn capture(settings: &Settings) -> Self {
        Self {
            provider: settings.ai_provider,
            address: configured_address(settings).map(str::to_owned),
            account: Arc::default(),
        }
    }

    /// A change of recipient requires a new user action before any credentials or text are sent.
    pub(super) fn apply(&self, current: Settings) -> AppResult<Settings> {
        if current.ai_provider != self.provider
            || configured_address(&current) != self.address.as_deref()
        {
            return Err(AppError::invalid(
                "The rewrite destination changed. Start a new rewrite to use it.",
            ));
        }
        Ok(current)
    }
}

impl AccountModel {
    pub(super) fn bind<'a>(
        slot: &'a mut Option<Self>,
        id: &str,
        preference: Option<&str>,
    ) -> AppResult<&'a mut Self> {
        let account = slot.get_or_insert_with(|| Self {
            id: id.to_owned(),
            preference: preference.map(str::to_owned),
            resolved: None,
        });
        if account.id != id {
            return Err(AppError::invalid(
                "The ChatGPT account changed. Start a new rewrite to use it.",
            ));
        }
        Ok(account)
    }

    pub(super) fn choose(
        &mut self,
        models: &[onda_cloud::ModelChoice],
        preferred: Option<&str>,
    ) -> AppResult<String> {
        if let Some(slug) = &self.resolved {
            return models.iter().find(|model| &model.slug == slug).map(|model| model.slug.clone())
                .ok_or_else(|| AppError::invalid("The original ChatGPT model is unavailable. Start a new rewrite to choose another."));
        }
        let model = [preferred, self.preference.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|slug| models.iter().find(|model| model.slug == slug))
            .or_else(|| super::recommended_model(models))
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Cloud,
                    "This account has no available text models.",
                    true,
                )
            })?;
        self.resolved = Some(model.slug.clone());
        Ok(model.slug.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retry_uses_the_corrected_model_and_refuses_a_new_recipient() {
        let mut settings = Settings {
            ai_provider: AiProvider::Local,
            local_model: Some("first".into()),
            ..Settings::default()
        };
        let route = RetryRoute::capture(&settings);
        settings.local_model = Some("second".into());
        assert_eq!(
            route
                .apply(settings.clone())
                .unwrap()
                .local_model
                .as_deref(),
            Some("second")
        );
        let mut redirected = settings.clone();
        redirected.local_base_url = Some("http://localhost:9876".into());
        assert!(route.apply(redirected).is_err());
        settings.ai_provider = AiProvider::Anthropic;
        assert!(route.apply(settings).is_err());
    }

    #[test]
    fn chatgpt_retries_keep_the_account_and_first_resolved_model() {
        let model = |slug: &str| onda_cloud::ModelChoice {
            slug: slug.into(),
            display_name: slug.into(),
        };
        let models = [model("first"), model("second")];
        let mut slot = None;
        let chosen = AccountModel::bind(&mut slot, "one", Some("first")).unwrap();
        assert_eq!(chosen.choose(&models, None).unwrap(), "first");
        let same = AccountModel::bind(&mut slot, "one", Some("second")).unwrap();
        assert_eq!(same.choose(&models, None).unwrap(), "first");
        assert!(same.choose(&models[1..], None).is_err());
        assert!(AccountModel::bind(&mut slot, "two", None).is_err());
    }
}
