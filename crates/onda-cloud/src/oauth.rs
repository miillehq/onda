//! Public-client OAuth, signed identity verification, and session lifecycle.

use crate::{
    storage::{expiry_after, AccountStore, ProfileRecord, SecretTokens},
    unix_time, AccountProfile, CloudConfig, CloudError, Result, SignOutResult,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
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
    pub http: Client,
    pub endpoints: Endpoints,
    app_name: String,
    binding: Uuid,
    discovery_cache: Mutex<Option<(Instant, Discovery)>>,
    jwks_cache: Mutex<Option<(Instant, JwkSet)>>,
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
                http,
                endpoints,
                app_name: config.app_name,
                binding: Uuid::new_v4(),
                discovery_cache: Mutex::new(None),
                jwks_cache: Mutex::new(None),
                session_cancellations: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Creates fresh state, nonce, PKCE, and a 127.0.0.1 listener before browsing.
    pub async fn begin_sign_in(&self, profile_id: Option<String>) -> Result<PendingSignIn> {
        let discovery = self.discovery().await?;
        let host_id = {
            let _lock = self.inner.store.lock_host().await?;
            self.inner.store.host_id()?
        };
        let profile = if let Some(id) = &profile_id {
            let _lock = self.inner.store.lock_profile(id).await?;
            Some(self.inner.store.load(id)?)
        } else {
            None
        };
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| CloudError::Transport)?;
        let port = listener
            .local_addr()
            .map_err(|_| CloudError::Transport)?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
        let state = random_secret(32)?;
        let nonce = random_secret(32)?;
        let verifier = Zeroizing::new(random_secret(64)?);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut authorization_url =
            Url::parse(&discovery.authorization_endpoint).map_err(|_| CloudError::Configuration)?;
        {
            let mut query = authorization_url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair(
                    "client_id",
                    profile
                        .as_ref()
                        .map(|record| record.client_id.as_str())
                        .unwrap_or(DYNAMIC_CLIENT),
                )
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair("scope", SCOPES)
                .append_pair("resource", RESOURCE)
                .append_pair("ext_agent_host_id", &host_id)
                .append_pair("state", &state)
                .append_pair("nonce", &nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &challenge);
            if let Some(record) = &profile {
                if let Some(tokens) = &record.tokens {
                    query.append_pair("id_token_hint", &tokens.id_token);
                }
                if let Some(email) = &record.email {
                    query.append_pair("login_hint", email);
                }
            } else {
                query.append_pair("agent_name_hint", &self.inner.app_name);
            }
        }
        Ok(PendingSignIn {
            listener,
            authorization_url,
            redirect_uri,
            state,
            nonce,
            verifier,
            selected_profile: profile_id,
            expected_client_id: profile.map(|record| record.client_id.clone()),
            binding: self.inner.binding,
            created_at: Instant::now(),
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

    async fn complete_sign_in_inner(
        &self,
        pending: PendingSignIn,
        cancellation: CancellationToken,
    ) -> Result<AccountProfile> {
        if pending.binding != self.inner.binding {
            return Err(CloudError::InvalidCallback);
        }
        let remaining = CALLBACK_TIMEOUT
            .checked_sub(pending.created_at.elapsed())
            .ok_or(CloudError::Timeout)?;
        let callback = tokio::select! {
            _ = cancellation.cancelled() => return Err(CloudError::Cancelled),
            result = tokio::time::timeout(remaining, receive_callback(&pending)) => result.map_err(|_| CloudError::Timeout)??,
        };
        let discovery = self.discovery().await?;
        let profile_id = pending
            .selected_profile
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let _profile_lock = self.inner.store.lock_profile(&profile_id).await?;
        let mut record = if pending.selected_profile.is_some() {
            self.inner.store.load(&profile_id)?
        } else {
            ProfileRecord {
                id: profile_id.clone(),
                client_id: callback.client_id.clone(),
                issuer: ISSUER.into(),
                subject: None,
                email: None,
                name: None,
                preferred_model: None,
                tokens: None,
                refresh_pending: false,
            }
        };
        // Persist registration before exchanging a one-use code so reauthorization
        // can reuse the issued client if exchange fails or the process stops.
        self.inner.store.save(&record)?;
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(CloudError::Cancelled),
            result = self.inner.http.post(&discovery.token_endpoint).timeout(CREDENTIAL_REQUEST_TIMEOUT).form(&[
                ("grant_type", "authorization_code"), ("client_id", callback.client_id.as_str()),
                ("code", callback.code.as_str()), ("code_verifier", pending.verifier.as_str()),
                ("redirect_uri", pending.redirect_uri.as_str()), ("resource", RESOURCE),
            ]).send() => result.map_err(transport_error)?,
        };
        let tokens: TokenResponse = checked_json(response).await?;
        let identity = self
            .verify_identity(
                tokens
                    .id_token
                    .as_deref()
                    .ok_or(CloudError::InvalidIdentity)?,
                &record.client_id,
                Some(&pending.nonce),
            )
            .await?;
        if record
            .subject
            .as_ref()
            .is_some_and(|subject| subject != &identity.sub)
        {
            return Err(CloudError::InvalidIdentity);
        }
        let secret_tokens = tokens.into_secrets(None)?;
        record.subject = Some(identity.sub);
        record.email = identity.email;
        record.name = identity.name;
        record.tokens = Some(secret_tokens);
        record.refresh_pending = false;
        self.inner.store.save(&record)?;
        {
            let _host_lock = self.inner.store.lock_host().await?;
            self.inner.store.select(Some(profile_id.clone()))?;
        }
        self.renew_session_cancellation(&profile_id).await;
        Ok(record.public(true))
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
                if let Ok(discovery) = self.discovery().await {
                    for attempt in 0..3 {
                        let response = self
                            .inner
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

    /// Cross-process locking prevents concurrent use of rotating refresh tokens.
    pub(crate) async fn access_token(&self, profile_id: &str) -> Result<Zeroizing<String>> {
        let _lock = self.inner.store.lock_profile(profile_id).await?;
        let mut record = self.inner.store.load(profile_id)?;
        let tokens = record.tokens.as_ref().ok_or(CloudError::SignInRequired)?;
        if !tokens
            .scopes
            .iter()
            .any(|scope| scope == "chatgpt.tokens.use.direct")
        {
            return Err(CloudError::PlanPermissionRequired);
        }
        let now = unix_time()?;
        if tokens.expires_at > now.saturating_add(REFRESH_MARGIN_SECONDS) {
            return Ok(Zeroizing::new(tokens.access_token.clone()));
        }
        if record.refresh_pending {
            return Err(CloudError::InterruptedRefresh);
        }
        if tokens
            .earliest_refresh_at
            .is_some_and(|earliest| now < earliest)
        {
            if tokens.expires_at > now {
                return Ok(Zeroizing::new(tokens.access_token.clone()));
            }
            return Err(CloudError::SignInRequired);
        }
        let discovery = self.discovery().await?;
        record.refresh_pending = true;
        self.inner.store.save(&record)?;
        let tokens = record.tokens.as_ref().ok_or(CloudError::SignInRequired)?;
        let response = self
            .inner
            .http
            .post(&discovery.token_endpoint)
            .timeout(CREDENTIAL_REQUEST_TIMEOUT)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", record.client_id.as_str()),
                ("refresh_token", tokens.refresh_token.as_str()),
                ("resource", RESOURCE),
            ])
            .send()
            .await
            .map_err(transport_error)?;
        if !response.status().is_success() {
            let error = remote_error(response).await;
            if terminal_refresh_error(&error) {
                record.tokens = None;
            }
            record.refresh_pending = false;
            self.inner.store.save(&record)?;
            return Err(if terminal_refresh_error(&error) {
                CloudError::SignInRequired
            } else {
                error
            });
        }
        let replacement: TokenResponse = checked_json(response).await?;
        if let Some(id_token) = &replacement.id_token {
            let identity = self
                .verify_identity(id_token, &record.client_id, None)
                .await?;
            if record.subject.as_deref() != Some(&identity.sub) {
                return Err(CloudError::InvalidIdentity);
            }
        }
        let secrets = replacement.into_secrets(record.tokens.as_ref())?;
        let access_token = Zeroizing::new(secrets.access_token.clone());
        record.tokens = Some(secrets);
        record.refresh_pending = false;
        self.inner.store.save(&record)?;
        Ok(access_token)
    }

    pub(crate) async fn session_cancellation(&self, profile_id: &str) -> CancellationToken {
        self.inner
            .session_cancellations
            .lock()
            .await
            .entry(profile_id.to_owned())
            .or_default()
            .clone()
    }

    async fn renew_session_cancellation(&self, profile_id: &str) {
        let previous = self
            .inner
            .session_cancellations
            .lock()
            .await
            .insert(profile_id.to_owned(), CancellationToken::new());
        if let Some(previous) = previous {
            previous.cancel();
        }
    }

    async fn discovery(&self) -> Result<Discovery> {
        let mut cache = self.inner.discovery_cache.lock().await;
        if let Some((time, discovery)) = &*cache {
            if time.elapsed() < CACHE_LIFETIME {
                return Ok(discovery.clone());
            }
        }
        let response = self
            .inner
            .http
            .get(&self.inner.endpoints.discovery)
            .timeout(CREDENTIAL_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(transport_error)?;
        let discovery: Discovery = checked_json(response).await?;
        self.validate_discovery(&discovery)?;
        *cache = Some((Instant::now(), discovery.clone()));
        Ok(discovery)
    }

    fn validate_discovery(&self, discovery: &Discovery) -> Result<()> {
        if discovery.issuer != ISSUER {
            return Err(CloudError::InvalidIdentity);
        }
        #[cfg(test)]
        if self
            .inner
            .endpoints
            .discovery
            .starts_with("http://127.0.0.1:")
        {
            let origin = Url::parse(&self.inner.endpoints.discovery)
                .map_err(|_| CloudError::Configuration)?
                .origin();
            for endpoint in [
                &discovery.authorization_endpoint,
                &discovery.token_endpoint,
                &discovery.jwks_uri,
                &discovery.revocation_endpoint,
            ] {
                let url = Url::parse(endpoint).map_err(|_| CloudError::Configuration)?;
                if url.origin() != origin || url.host_str() != Some("127.0.0.1") {
                    return Err(CloudError::Configuration);
                }
            }
            return Ok(());
        }
        if discovery.authorization_endpoint != AUTHORIZATION_URL
            || discovery.token_endpoint != TOKEN_URL
            || discovery.jwks_uri != JWKS_URL
            || discovery.revocation_endpoint != REVOCATION_URL
        {
            return Err(CloudError::Configuration);
        }
        Ok(())
    }

    async fn verify_identity(
        &self,
        id_token: &str,
        client_id: &str,
        nonce: Option<&str>,
    ) -> Result<IdentityClaims> {
        if id_token.len() > MAX_HTTP_BYTES {
            return Err(CloudError::InvalidIdentity);
        }
        let header = decode_header(id_token).map_err(|_| CloudError::InvalidIdentity)?;
        if header.alg != Algorithm::RS256 {
            return Err(CloudError::InvalidIdentity);
        }
        let kid = header.kid.ok_or(CloudError::InvalidIdentity)?;
        let discovery = self.discovery().await?;
        let mut cache = self.inner.jwks_cache.lock().await;
        let refresh = cache.as_ref().is_none_or(|(time, keys)| {
            time.elapsed() >= CACHE_LIFETIME || keys.find(&kid).is_none()
        });
        if refresh {
            let response = self
                .inner
                .http
                .get(&discovery.jwks_uri)
                .timeout(CREDENTIAL_REQUEST_TIMEOUT)
                .send()
                .await
                .map_err(transport_error)?;
            *cache = Some((Instant::now(), checked_json(response).await?));
        }
        let key = cache
            .as_ref()
            .and_then(|(_, keys)| keys.find(&kid))
            .ok_or(CloudError::InvalidIdentity)?;
        let decoding_key = DecodingKey::from_jwk(key).map_err(|_| CloudError::InvalidIdentity)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[client_id]);
        validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
        validation.validate_nbf = true;
        validation.leeway = 5;
        let identity = decode::<IdentityClaims>(id_token, &decoding_key, &validation)
            .map_err(|_| CloudError::InvalidIdentity)?
            .claims;
        if identity.sub.is_empty()
            || identity.sub.len() > 1024
            || identity.iat > unix_time()?.saturating_add(5)
        {
            return Err(CloudError::InvalidIdentity);
        }
        if let Some(nonce) = nonce {
            let actual = identity
                .nonce
                .as_deref()
                .ok_or(CloudError::InvalidIdentity)?;
            if actual.as_bytes().ct_eq(nonce.as_bytes()).unwrap_u8() != 1 {
                return Err(CloudError::InvalidIdentity);
            }
        }
        Ok(identity)
    }
}

#[derive(Deserialize)]
struct IdentityClaims {
    sub: String,
    iat: u64,
    nonce: Option<String>,
    email: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: u64,
    scope: Option<String>,
    #[serde(default, deserialize_with = "deserialize_refresh_hint")]
    earliest_refresh_at: Option<u64>,
}

/// The reference names this optional field without fixing its JSON type. Accept
/// epoch hints when recognizable; expiry remains the standard renewal contract.
fn deserialize_refresh_hint<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.as_ref().and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    }))
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.access_token.zeroize();
        self.refresh_token.zeroize();
        self.id_token.zeroize();
    }
}

impl TokenResponse {
    fn into_secrets(mut self, previous: Option<&SecretTokens>) -> Result<SecretTokens> {
        let scopes: Vec<String> = self
            .scope
            .as_ref()
            .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_else(|| {
                previous
                    .map(|tokens| tokens.scopes.clone())
                    .unwrap_or_default()
            });
        let has_plan_usage = scopes
            .iter()
            .any(|scope| scope == "chatgpt.tokens.use.direct");
        if has_plan_usage
            && (!self.token_type.eq_ignore_ascii_case("Bearer")
                || self.access_token.is_empty()
                || self.expires_in == 0
                || self.expires_in > MAX_ACCESS_TOKEN_LIFETIME_SECONDS)
        {
            return Err(CloudError::MalformedResponse);
        }
        let refresh_token = self.refresh_token.take().unwrap_or_default();
        if has_plan_usage && refresh_token.is_empty() {
            return Err(CloudError::MalformedResponse);
        }
        let id_token = self
            .id_token
            .take()
            .or_else(|| previous.map(|tokens| tokens.id_token.clone()))
            .ok_or(CloudError::InvalidIdentity)?;
        Ok(SecretTokens {
            access_token: std::mem::take(&mut self.access_token),
            refresh_token,
            id_token,
            scopes,
            expires_at: expiry_after(self.expires_in)?,
            earliest_refresh_at: self.earliest_refresh_at,
        })
    }
}

struct Callback {
    code: Zeroizing<String>,
    client_id: String,
}

async fn receive_callback(pending: &PendingSignIn) -> Result<Callback> {
    for _ in 0..MAX_CALLBACK_CONNECTIONS {
        let (mut socket, address) = pending
            .listener
            .accept()
            .await
            .map_err(|_| CloudError::Transport)?;
        if !address.ip().is_loopback() {
            continue;
        }
        let request = match tokio::time::timeout(SOCKET_TIMEOUT, read_request(&mut socket)).await {
            Ok(Ok(request)) => request,
            _ => {
                write_callback_response(&mut socket, false).await;
                continue;
            }
        };
        let result = parse_callback(&request, pending);
        match result {
            Ok(Some(callback)) => {
                write_callback_response(&mut socket, true).await;
                return Ok(callback);
            }
            Ok(None) => {
                write_callback_response(&mut socket, false).await;
            }
            Err(error) => {
                write_callback_response(&mut socket, false).await;
                return Err(error);
            }
        }
    }
    Err(CloudError::InvalidCallback)
}

async fn read_request(socket: &mut TcpStream) -> Result<Zeroizing<String>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(2048));
    let mut buffer = [0u8; 2048];
    loop {
        let read = socket
            .read(&mut buffer)
            .await
            .map_err(|_| CloudError::InvalidCallback)?;
        if read == 0 {
            return Err(CloudError::InvalidCallback);
        }
        if bytes.len() + read > MAX_HTTP_BYTES {
            return Err(CloudError::SizeLimit);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(std::mem::take(&mut *bytes))
        .map(Zeroizing::new)
        .map_err(|_| CloudError::InvalidCallback)
}

fn parse_callback(request: &str, pending: &PendingSignIn) -> Result<Option<Callback>> {
    let mut lines = request.split("\r\n");
    let mut request_line = lines
        .next()
        .ok_or(CloudError::InvalidCallback)?
        .split_whitespace();
    if request_line.next() != Some("GET") {
        return Err(CloudError::InvalidCallback);
    }
    let target = request_line.next().ok_or(CloudError::InvalidCallback)?;
    if request_line.next() != Some("HTTP/1.1")
        || request_line.next().is_some()
        || !target.starts_with('/')
        || target.starts_with("//")
    {
        return Err(CloudError::InvalidCallback);
    }
    let expected_authority = pending
        .redirect_uri
        .strip_prefix("http://")
        .and_then(|uri| uri.strip_suffix(CALLBACK_PATH))
        .ok_or(CloudError::Configuration)?;
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    if hosts != [expected_authority] {
        return Err(CloudError::InvalidCallback);
    }
    let url = Url::parse(&format!("http://{expected_authority}{target}"))
        .map_err(|_| CloudError::InvalidCallback)?;
    if url.path() != CALLBACK_PATH {
        return Ok(None);
    }
    let mut values = HashMap::new();
    for (key, value) in url.query_pairs() {
        if values
            .insert(key.into_owned(), Zeroizing::new(value.into_owned()))
            .is_some()
        {
            return Err(CloudError::InvalidCallback);
        }
    }
    let state = values.get("state").ok_or(CloudError::InvalidCallback)?;
    if state.as_bytes().ct_eq(pending.state.as_bytes()).unwrap_u8() != 1 {
        return Err(CloudError::InvalidCallback);
    }
    if let Some(error) = values.get("error") {
        return Err(if error.as_str() == "access_denied" {
            CloudError::AccessDenied
        } else {
            CloudError::InvalidCallback
        });
    }
    let code = values
        .remove("code")
        .filter(|code| !code.is_empty() && code.len() <= 8192)
        .ok_or(CloudError::InvalidCallback)?;
    let issued = values.remove("client_id").map(|client| client.to_string());
    let client_id = if let Some(expected) = &pending.expected_client_id {
        if issued.as_ref().is_some_and(|issued| issued != expected) {
            return Err(CloudError::InvalidCallback);
        }
        expected.clone()
    } else {
        issued
            .filter(|client| valid_issued_client(client))
            .ok_or(CloudError::InvalidCallback)?
    };
    Ok(Some(Callback { code, client_id }))
}

fn valid_issued_client(client: &str) -> bool {
    client != DYNAMIC_CLIENT
        && !client.is_empty()
        && client.len() <= 256
        && client
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

async fn write_callback_response(socket: &mut TcpStream, success: bool) {
    let (status, body) = if success {
        (
            "200 OK",
            "Sign-in received. You may return to the application.",
        )
    } else {
        (
            "400 Bad Request",
            "Sign-in could not be verified. Return to the application.",
        )
    };
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    let _ = tokio::time::timeout(SOCKET_TIMEOUT, socket.write_all(response.as_bytes())).await;
}

fn random_secret(length: usize) -> Result<String> {
    let mut bytes = Zeroizing::new(vec![0; length]);
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| CloudError::Configuration)?;
    Ok(URL_SAFE_NO_PAD.encode(&*bytes))
}

pub(crate) fn transport_error(error: reqwest::Error) -> CloudError {
    if error.is_timeout() {
        CloudError::Timeout
    } else {
        CloudError::Transport
    }
}

pub(crate) async fn read_body(response: Response) -> Result<Zeroizing<Vec<u8>>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_JSON_BYTES as u64)
    {
        return Err(CloudError::SizeLimit);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(transport_error)?;
        if bytes.len() + chunk.len() > MAX_JSON_BYTES {
            return Err(CloudError::SizeLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) async fn checked_json<T: serde::de::DeserializeOwned>(response: Response) -> Result<T> {
    if !response.status().is_success() {
        return Err(remote_error(response).await);
    }
    serde_json::from_slice(&read_body(response).await?).map_err(|_| CloudError::MalformedResponse)
}

pub(crate) async fn remote_error(response: Response) -> CloudError {
    let status = response.status().as_u16();
    let request_id = response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("openai-request-id"))
        .and_then(|value| value.to_str().ok())
        .and_then(safe_identifier);
    let value: Option<serde_json::Value> = match read_body(response).await {
        Ok(body) => serde_json::from_slice(&body).ok(),
        Err(_) => None,
    };
    let error = value.as_ref().and_then(|value| value.get("error"));
    let code = error
        .and_then(|error| error.get("code"))
        .or_else(|| error.filter(|error| error.is_string()))
        .and_then(|code| code.as_str())
        .and_then(safe_identifier);
    let parameter = error
        .and_then(|error| error.get("param"))
        .and_then(|param| param.as_str())
        .and_then(safe_identifier);
    let body_shape = if error.is_some() {
        "error"
    } else if value
        .as_ref()
        .is_some_and(|value| value.get("detail").is_some())
    {
        "detail"
    } else {
        "unknown"
    }
    .to_owned();
    CloudError::Remote {
        status,
        code,
        parameter,
        request_id,
        body_shape,
    }
}

pub(crate) fn safe_identifier(value: &str) -> Option<String> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.[]/".contains(&byte))
    {
        return None;
    }
    Some(value.to_owned())
}

fn terminal_refresh_error(error: &CloudError) -> bool {
    const TERMINAL_CODES: &[&str] = &[
        "invalid_grant",
        "invalid_refresh_token",
        "token_expired",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "refresh_token_reused",
    ];
    matches!(error, CloudError::Remote { code: Some(code), .. } if TERMINAL_CODES.contains(&code.as_str()))
}

#[cfg(test)]
mod tests;
