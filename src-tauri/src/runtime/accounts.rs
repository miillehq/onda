//! Owns the sign-in transaction, account model cache and selected ChatGPT connection.
use super::*;

pub(super) struct ConnectedAccounts {
    pub(super) cloud: Option<ChatGptService>,
    pub(super) cloud_error: Option<AppError>,
    pub(super) cloud_models: Mutex<Option<CachedModels>>,
    pub(super) oauth: Mutex<Option<CancellationToken>>,
}

impl ConnectedAccounts {
    pub(super) fn cloud(&self) -> AppResult<&ChatGptService> {
        self.cloud.as_ref().ok_or_else(|| {
            self.cloud_error.clone().unwrap_or_else(|| {
                AppError::new(
                    ErrorCode::Cloud,
                    "Protected ChatGPT storage is unavailable. Local dictation remains available.",
                    true,
                )
            })
        })
    }

    /// The account's models, fetched at most every half hour so polish can start at once.
    pub(super) async fn account_models(
        &self,
        profile_id: &str,
    ) -> AppResult<Vec<onda_cloud::ModelChoice>> {
        if let Some(cached) = lock(&self.cloud_models)?.as_ref().filter(|cached| {
            cached.profile_id == profile_id && cached.fetched.elapsed() < MODEL_LIST_TTL
        }) {
            return Ok(cached.models.clone());
        }
        let models = self
            .cloud()?
            .list_models(Some(profile_id.to_owned()))
            .await
            .map_err(errors::cloud)?;
        *lock(&self.cloud_models)? = Some(CachedModels {
            profile_id: profile_id.to_owned(),
            fetched: Instant::now(),
            models: models.clone(),
        });
        Ok(models)
    }

    /// Look up the account and its models while the user speaks; on a cold start this took
    /// one to four seconds before ChatGPT was even asked.
    pub(super) fn prefetch_polish(&self, host: &Arc<Runtime>) {
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(cloud) = service.rewrites.accounts.cloud() else {
                return;
            };
            let Ok(profiles) = cloud.list_profiles().await else {
                return;
            };
            if let Some(profile) = profiles
                .iter()
                .find(|profile| profile.active && profile.signed_in)
            {
                let _ = service.rewrites.accounts.account_models(&profile.id).await;
            }
        });
    }

    pub(super) async fn chatgpt_text(
        &self,
        host: &Arc<Runtime>,
        active: &ActiveOperation,
        source: &str,
        instructions: &str,
        preferred: Option<&str>,
    ) -> AppResult<String> {
        let profiles = self.cloud()?.list_profiles().await.map_err(errors::cloud)?;
        let profile = profiles
            .iter()
            .find(|profile| profile.active)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Unauthorized,
                    "No ChatGPT account is connected. Connect one in AI polish to use this.",
                    true,
                )
            })?;
        let selected_model = self
            .resolve_operation_model(host, active, profile, preferred)
            .await?;
        tracing::info!(
            subsystem = "chatgpt",
            operation_id = active.id,
            model = selected_model,
            "text enhancement started"
        );
        let service = host.clone();
        let id = active.id.clone();
        let started = Instant::now();
        let mut first_token = false;
        let request = ResponseRequest {
            profile_id: Some(profile.id.clone()),
            model: selected_model,
            instructions: instructions.into(),
            source_text: source.into(),
            context_text: if active.context.is_empty() {
                None
            } else {
                Some(active.context.clone())
            },
        };
        let result = self
            .cloud()?
            .respond(request, active.cancellation.clone(), move |delta| {
                if !first_token {
                    service
                        .performance
                        .metric(&service, &id, "chatgptFirstToken", started, true);
                    first_token = true;
                }
                service.domain(AppEvent::CloudDelta {
                    operation_id: id.clone(),
                    delta: delta.into(),
                });
            })
            .await;
        host.performance.metric(
            host,
            &active.id,
            "chatgptCompletion",
            started,
            result.is_ok(),
        );
        result.map(|response| response.text).map_err(errors::cloud)
    }
}

impl ConnectedAccounts {
    pub(super) fn new(profile: &std::path::Path) -> Self {
        let result = ChatGptService::new(CloudConfig {
            data_directory: profile.join("accounts"),
            app_name: onda_core::APP_NAME.into(),
        });
        let (cloud, cloud_error) = match result {
            Ok(cloud) => (Some(cloud), None),
            Err(error) => (None, Some(errors::cloud(error))),
        };
        Self {
            cloud,
            cloud_error,
            cloud_models: Mutex::default(),
            oauth: Mutex::default(),
        }
    }
}

impl ConnectedAccounts {
    /// Pin the account before discovery and keep a resolved model for any retry.
    async fn resolve_operation_model(
        &self,
        host: &Runtime,
        active: &ActiveOperation,
        profile: &onda_cloud::AccountProfile,
        preferred: Option<&str>,
    ) -> AppResult<String> {
        let account = lock(&host.results.result)?
            .as_ref()
            .filter(|saved| saved.result.operation_id == active.id)
            .and_then(|saved| saved.retry.as_ref())
            .map(|retry| retry.route.account.clone())
            .unwrap_or_default();
        // Bind before discovery so a failed refresh cannot retarget the next attempt.
        AccountModel::bind(
            &mut *lock(&account)?,
            &profile.id,
            profile.preferred_model.as_deref(),
        )?;
        lock(&host.operations.state)?.mark_cloud(&active.id, DataRoute::ChatgptPlan);
        host.operations.emit_state(host);
        let models = self.account_models(&profile.id).await?;
        let selected_model = AccountModel::bind(
            &mut *lock(&account)?,
            &profile.id,
            profile.preferred_model.as_deref(),
        )?
        .choose(&models, preferred)?;
        Ok(selected_model)
    }
}
