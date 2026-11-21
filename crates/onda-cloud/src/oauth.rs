//! Public-client OAuth, signed identity verification, and session lifecycle.

mod wire;
pub(crate) use wire::*;
mod authority;
mod sessions;
use authority::Authority;
mod callback;
use crate::{
    storage::{expiry_after, AccountStore, ProfileRecord, SecretTokens},
    unix_time, AccountProfile, CloudConfig, CloudError, Result, SignOutResult,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use callback::*;
use futures_util::StreamExt;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use rand::{rngs::OsRng, RngCore};
use reqwest::{Client, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

pub(crate) const ISSUER: &str = "https://auth.openai.com";
pub(crate) const RESOURCE: &str = "https://api.openai.com/v1";
const AUTHORIZATION_URL: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const DISCOVERY_URL: &str = "https://auth.openai.com/.well-known/openid-configuration";
const JWKS_URL: &str = "https://auth.openai.com/.well-known/jwks.json";
const REVOCATION_URL: &str = "https://auth.openai.com/api/accounts/oauth/revoke";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const DYNAMIC_CLIENT: &str = "dynamic_agent_client";
const CALLBACK_PATH: &str = "/auth/callback";
const MAX_HTTP_BYTES: usize = 16 * 1024;
pub(crate) const MAX_JSON_BYTES: usize = 1024 * 1024;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const CREDENTIAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CACHE_LIFETIME: Duration = Duration::from_secs(300);
const REFRESH_MARGIN_SECONDS: u64 = 60;
const MAX_CALLBACK_CONNECTIONS: usize = 16;
const MAX_ACCESS_TOKEN_LIFETIME_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone)]
pub(crate) struct Endpoints {
    pub discovery: String,
    pub models: String,
    pub responses: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            discovery: DISCOVERY_URL.into(),
            models: format!("{RESOURCE}/models"),
            responses: format!("{RESOURCE}/responses"),
        }
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    pub token_endpoint: String,
    jwks_uri: String,
    revocation_endpoint: String,
}

pub(crate) struct ServiceInner {
    pub store: AccountStore,
    pub(crate) authority: Authority,
    app_name: String,
    binding: Uuid,
    pub session_cancellations: Mutex<HashMap<String, CancellationToken>>,
}

/// Cloneable native service. It contains no serializable credential-bearing API.
#[derive(Clone)]
pub struct ChatGptService {
    pub(crate) inner: Arc<ServiceInner>,
}

/// One-use transaction retaining a bound listener and private PKCE state.
///
/// Do not serialize or log this transaction or its browser URL: returning flows
/// contain a retained ID-token hint, intended only for OpenAI authorization.
pub struct PendingSignIn {
    listener: TcpListener,
    authorization_url: Url,
    redirect_uri: String,
    state: String,
    nonce: String,
    verifier: Zeroizing<String>,
    selected_profile: Option<String>,
    expected_client_id: Option<String>,
    binding: Uuid,
    created_at: Instant,
}

impl PendingSignIn {
    /// Opens the system browser after the listener has already been bound.
    pub fn open_browser(&self) -> Result<()> {
        webbrowser::open(self.authorization_url.as_str())
            .map(|_| ())
            .map_err(|_| CloudError::Browser)
    }
}

impl ChatGptService {
    /// Creates fresh state, nonce, PKCE, and a 127.0.0.1 listener before browsing.
    pub async fn begin_sign_in(&self, profile_id: Option<String>) -> Result<PendingSignIn> {
        self.inner.begin_sign_in(profile_id).await
    }

    async fn complete_sign_in_inner(
        &self,
        pending: PendingSignIn,
        cancellation: CancellationToken,
    ) -> Result<AccountProfile> {
        self.inner
            .complete_sign_in_inner(pending, cancellation)
            .await
    }

    /// Cross-process locking prevents concurrent use of rotating refresh tokens.
    pub(crate) async fn access_token(&self, profile_id: &str) -> Result<Zeroizing<String>> {
        self.inner.access_token(profile_id).await
    }

    pub(crate) async fn session_cancellation(&self, profile_id: &str) -> CancellationToken {
        self.inner.session_cancellation(profile_id).await
    }

    /// Initializes a Windows-native service with fixed official HTTPS endpoints.
    pub fn new(config: CloudConfig) -> Result<Self> {
        if config.app_name.trim().is_empty()
            || config.app_name.len() > 128
            || !config.data_directory.is_absolute()
        {
            return Err(CloudError::Configuration);
        }
        let http = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| CloudError::Configuration)?;
        Self::build(config, http, Endpoints::default())
    }

    fn build(config: CloudConfig, http: Client, endpoints: Endpoints) -> Result<Self> {
        let store = AccountStore::new(config.data_directory)?;
        Ok(Self {
            inner: Arc::new(ServiceInner {
                store,
                authority: Authority::new(http, endpoints),
                app_name: config.app_name,
                binding: Uuid::new_v4(),
                session_cancellations: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Consumes a pending transaction, validates its callback, and commits identity.
    pub async fn complete_sign_in(
        &self,
        pending: PendingSignIn,
        cancellation: CancellationToken,
    ) -> Result<AccountProfile> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(CloudError::Cancelled),
            result = self.complete_sign_in_inner(pending, cancellation.clone()) => result,
        }
    }

    /// Opens the browser and waits for a single validated authorization result.
    pub async fn sign_in(
        &self,
        profile_id: Option<String>,
        cancellation: CancellationToken,
    ) -> Result<AccountProfile> {
        let pending = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(CloudError::Cancelled),
            result = self.begin_sign_in(profile_id) => result?,
        };
        if cancellation.is_cancelled() {
            return Err(CloudError::Cancelled);
        }
        pending.open_browser()?;
        self.complete_sign_in(pending, cancellation).await
    }

    /// Explicit user-requested consent to enable plan usage after an earlier decline.
    /// Ordinary returning sign-in never forces consent.
    pub async fn enable_plan_usage(
        &self,
        profile_id: String,
        cancellation: CancellationToken,
    ) -> Result<AccountProfile> {
        let mut pending = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(CloudError::Cancelled),
            result = self.begin_sign_in(Some(profile_id)) => result?,
        };
        if cancellation.is_cancelled() {
            return Err(CloudError::Cancelled);
        }
        pending
            .authorization_url
            .query_pairs_mut()
            .append_pair("prompt", "consent");
        pending.open_browser()?;
        self.complete_sign_in(pending, cancellation).await
    }

    pub async fn list_profiles(&self) -> Result<Vec<AccountProfile>> {
        let active = {
            let _lock = self.inner.store.lock_host().await?;
            self.inner.store.active_profile()?
        };
        let mut profiles = Vec::new();
        for id in self.inner.store.profile_ids()? {
            let _lock = self.inner.store.lock_profile(&id).await?;
            profiles.push(
                self.inner
                    .store
                    .load(&id)?
                    .public(active.as_deref() == Some(&id)),
            );
        }
        Ok(profiles)
    }

    /// Selection changes no other account's credentials or registration.
    pub async fn select_profile(&self, profile_id: &str) -> Result<AccountProfile> {
        let _profile_lock = self.inner.store.lock_profile(profile_id).await?;
        let record = self.inner.store.load(profile_id)?;
        if record.tokens.is_none() {
            return Err(CloudError::SignInRequired);
        }
        let _host_lock = self.inner.store.lock_host().await?;
        self.inner.store.select(Some(profile_id.to_owned()))?;
        Ok(record.public(true))
    }

    /// Persists a preference by account model slug after checking its catalog.
    pub async fn set_preferred_model(&self, profile_id: &str, slug: &str) -> Result<()> {
        let models = self.list_models(Some(profile_id.to_owned())).await?;
        if !models.iter().any(|model| model.slug == slug) {
            return Err(CloudError::ModelUnavailable);
        }
        let _lock = self.inner.store.lock_profile(profile_id).await?;
        let mut record = self.inner.store.load(profile_id)?;
        record.preferred_model = Some(slug.to_owned());
        self.inner.store.save(&record)
    }

    /// Cancels native requests, revokes with bounded retry, and clears local tokens.
    pub async fn sign_out(&self, profile_id: &str) -> Result<SignOutResult> {
        self.session_cancellation(profile_id).await.cancel();
        let _lock = self.inner.store.lock_profile(profile_id).await?;
        let mut record = self.inner.store.load(profile_id)?;
        let mut confirmed = record.tokens.is_none();
        if let Some(tokens) = &record.tokens {
            if tokens.refresh_token.is_empty() {
                confirmed = true;
            } else {
                if let Ok(discovery) = self.inner.authority.discovery().await {
                    for attempt in 0..3 {
                        let response = self
                            .inner
                            .authority
                            .http
                            .post(&discovery.revocation_endpoint)
                            .timeout(CREDENTIAL_REQUEST_TIMEOUT)
                            .form(&[
                                ("token", tokens.refresh_token.as_str()),
                                ("token_type_hint", "refresh_token"),
                                ("client_id", record.client_id.as_str()),
                            ])
                            .send()
                            .await;
                        match response {
                            Ok(response) if response.status() == reqwest::StatusCode::OK => {
                                confirmed = true;
                                break;
                            }
                            Ok(response) if !response.status().is_server_error() => break,
                            _ => {
                                if attempt < 2 {
                                    tokio::time::sleep(Duration::from_millis(250 * (1 << attempt)))
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }
        record.tokens = None;
        record.refresh_pending = false;
        self.inner.store.save(&record)?;
        let _host_lock = self.inner.store.lock_host().await?;
        if self.inner.store.active_profile()?.as_deref() == Some(profile_id) {
            self.inner.store.select(None)?;
        }
        Ok(SignOutResult {
            remote_revocation_confirmed: confirmed,
        })
    }

    /// Forget an account on this PC. A still-signed-in account is signed out first so its
    /// refresh token is revoked rather than abandoned; an unreadable registration is deleted
    /// because the user asked for it and it can never be used again.
    pub async fn remove_profile(&self, profile_id: &str) -> Result<SignOutResult> {
        let signed_in = match self.inner.store.load(profile_id) {
            Ok(record) => record.tokens.is_some(),
            Err(CloudError::UnknownProfile) => return Err(CloudError::UnknownProfile),
            Err(_) => false,
        };
        let result = if signed_in {
            self.sign_out(profile_id).await?
        } else {
            self.session_cancellation(profile_id).await.cancel();
            SignOutResult {
                remote_revocation_confirmed: true,
            }
        };
        {
            let _lock = self.inner.store.lock_profile(profile_id).await?;
            self.inner.store.remove(profile_id)?;
        }
        self.inner.store.remove_lock_file(profile_id);
        let _host_lock = self.inner.store.lock_host().await?;
        if self.inner.store.active_profile()?.as_deref() == Some(profile_id) {
            self.inner.store.select(None)?;
        }
        Ok(result)
    }

    pub(crate) async fn resolve_profile(&self, id: Option<String>) -> Result<String> {
        if let Some(id) = id {
            return Ok(id);
        }
        let _lock = self.inner.store.lock_host().await?;
        self.inner
            .store
            .active_profile()?
            .ok_or(CloudError::NoAccount)
    }
}

#[cfg(test)]
mod tests;
