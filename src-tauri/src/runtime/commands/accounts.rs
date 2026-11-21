use super::*;
use ApiResponse as O;

pub(super) fn start_chatgpt_sign_in(
    service: &Arc<Runtime>,
    profile_id: Option<String>,
) -> AppResult<ApiResponse> {
    service
        .rewrites
        .accounts
        .start_sign_in(service, profile_id, false)?;
    Ok(O::Unit)
}

pub(super) fn enable_chatgpt_plan_usage(
    service: &Arc<Runtime>,
    profile_id: String,
) -> AppResult<ApiResponse> {
    service
        .rewrites
        .accounts
        .start_sign_in(service, Some(profile_id), true)?;
    Ok(O::Unit)
}

pub(super) fn cancel_chatgpt_sign_in(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    if let Some(cancellation) = lock(&service.rewrites.accounts.oauth)?.as_ref() {
        cancellation.cancel();
    }
    Ok(O::Unit)
}

pub(super) async fn select_chatgpt_profile(
    service: &Arc<Runtime>,
    profile_id: String,
) -> AppResult<ApiResponse> {
    service.operations.cancel(service).await?;
    service
        .rewrites
        .accounts
        .cloud()?
        .select_profile(&profile_id)
        .await
        .map_err(errors::cloud)?;
    service.event(RuntimeEvent::OauthChanged {
        status: "selected".into(),
    });
    Ok(O::Unit)
}

pub(super) async fn remove_chatgpt_profile(
    service: &Arc<Runtime>,
    profile_id: String,
) -> AppResult<ApiResponse> {
    service.operations.cancel(service).await?;
    let removed = service
        .rewrites
        .accounts
        .cloud()?
        .remove_profile(&profile_id)
        .await
        .map_err(errors::cloud)?;
    service.event(RuntimeEvent::OauthChanged {
        status: "removed".into(),
    });
    Ok(O::SignOut {
        remote_revocation_confirmed: removed.remote_revocation_confirmed,
    })
}

pub(super) async fn sign_out_chatgpt_profile(
    service: &Arc<Runtime>,
    profile_id: String,
) -> AppResult<ApiResponse> {
    service.operations.cancel(service).await?;
    let signed_out = service
        .rewrites
        .accounts
        .cloud()?
        .sign_out(&profile_id)
        .await
        .map_err(errors::cloud)?;
    service.event(RuntimeEvent::OauthChanged {
        status: "signedOut".into(),
    });
    Ok(O::SignOut {
        remote_revocation_confirmed: signed_out.remote_revocation_confirmed,
    })
}

pub(super) async fn list_available_models(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    let models = service
        .rewrites
        .accounts
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

pub(super) async fn set_chatgpt_model(
    service: &Arc<Runtime>,
    profile_id: String,
    slug: String,
) -> AppResult<ApiResponse> {
    service
        .rewrites
        .accounts
        .cloud()?
        .set_preferred_model(&profile_id, &slug)
        .await
        .map_err(errors::cloud)?;
    Ok(O::Unit)
}

pub(super) async fn save_provider_key(
    service: &Arc<Runtime>,
    provider: AiProvider,
    key: String,
) -> AppResult<ApiResponse> {
    service
        .rewrites
        .provider_access
        .save_provider_key(provider, key)
        .await?;
    Ok(O::Unit)
}

pub(super) async fn remove_provider_key(
    service: &Arc<Runtime>,
    provider: AiProvider,
) -> AppResult<ApiResponse> {
    service
        .rewrites
        .provider_access
        .remove_provider_key(provider)
        .await?;
    Ok(O::Unit)
}

impl ConnectedAccounts {
    pub(in crate::runtime) fn start_sign_in(
        &self,
        host: &Arc<Runtime>,
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
        let service = host.clone();
        host.event(RuntimeEvent::OauthChanged {
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
            if let Ok(mut oauth) = lock(&service.rewrites.accounts.oauth) {
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
}
