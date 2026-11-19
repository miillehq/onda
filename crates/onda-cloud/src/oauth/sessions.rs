//! Account session operations share the original protected store and profile locks.
use super::*;

impl ServiceInner {
    /// Creates fresh state, nonce, PKCE, and a 127.0.0.1 listener before browsing.
    pub(super) async fn begin_sign_in(&self, profile_id: Option<String>) -> Result<PendingSignIn> {
        let discovery = self.authority.discovery().await?;
        let host_id = {
            let _lock = self.store.lock_host().await?;
            self.store.host_id()?
        };
        let profile = if let Some(id) = &profile_id {
            let _lock = self.store.lock_profile(id).await?;
            Some(self.store.load(id)?)
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
                query.append_pair("agent_name_hint", &self.app_name);
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
            binding: self.binding,
            created_at: Instant::now(),
        })
    }

    pub(super) async fn complete_sign_in_inner(
        &self,
        pending: PendingSignIn,
        cancellation: CancellationToken,
    ) -> Result<AccountProfile> {
        if pending.binding != self.binding {
            return Err(CloudError::InvalidCallback);
        }
        let remaining = CALLBACK_TIMEOUT
            .checked_sub(pending.created_at.elapsed())
            .ok_or(CloudError::Timeout)?;
        let callback = tokio::select! {
            _ = cancellation.cancelled() => return Err(CloudError::Cancelled),
            result = tokio::time::timeout(remaining, receive_callback(&pending)) => result.map_err(|_| CloudError::Timeout)??,
        };
        let discovery = self.authority.discovery().await?;
        let profile_id = pending
            .selected_profile
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let _profile_lock = self.store.lock_profile(&profile_id).await?;
        let mut record = if pending.selected_profile.is_some() {
            self.store.load(&profile_id)?
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
        self.store.save(&record)?;
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(CloudError::Cancelled),
            result = self.authority.http.post(&discovery.token_endpoint).timeout(CREDENTIAL_REQUEST_TIMEOUT).form(&[
                ("grant_type", "authorization_code"), ("client_id", callback.client_id.as_str()),
                ("code", callback.code.as_str()), ("code_verifier", pending.verifier.as_str()),
                ("redirect_uri", pending.redirect_uri.as_str()), ("resource", RESOURCE),
            ]).send() => result.map_err(transport_error)?,
        };
        let tokens: TokenResponse = checked_json(response).await?;
        let identity = self
            .authority
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
        self.store.save(&record)?;
        {
            let _host_lock = self.store.lock_host().await?;
            self.store.select(Some(profile_id.clone()))?;
        }
        self.renew_session_cancellation(&profile_id).await;
        Ok(record.public(true))
    }

    /// Cross-process locking prevents concurrent use of rotating refresh tokens.
    pub(super) async fn access_token(&self, profile_id: &str) -> Result<Zeroizing<String>> {
        let _lock = self.store.lock_profile(profile_id).await?;
        let mut record = self.store.load(profile_id)?;
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
        let discovery = self.authority.discovery().await?;
        record.refresh_pending = true;
        self.store.save(&record)?;
        let tokens = record.tokens.as_ref().ok_or(CloudError::SignInRequired)?;
        let response = self
            .authority
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
            self.store.save(&record)?;
            return Err(if terminal_refresh_error(&error) {
                CloudError::SignInRequired
            } else {
                error
            });
        }
        let replacement: TokenResponse = checked_json(response).await?;
        if let Some(id_token) = &replacement.id_token {
            let identity = self
                .authority
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
        self.store.save(&record)?;
        Ok(access_token)
    }

    pub(super) async fn session_cancellation(&self, profile_id: &str) -> CancellationToken {
        self.session_cancellations
            .lock()
            .await
            .entry(profile_id.to_owned())
            .or_default()
            .clone()
    }

    pub(super) async fn renew_session_cancellation(&self, profile_id: &str) {
        let previous = self
            .session_cancellations
            .lock()
            .await
            .insert(profile_id.to_owned(), CancellationToken::new());
        if let Some(previous) = previous {
            previous.cancel();
        }
    }
}
