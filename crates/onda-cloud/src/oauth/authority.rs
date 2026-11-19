//! HTTPS discovery and identity verification for one authority and its cached keys.
use super::*;

pub(crate) struct Authority {
    pub(crate) http: Client,
    pub(crate) endpoints: Endpoints,
    discovery_cache: Mutex<Option<(Instant, Discovery)>>,
    jwks_cache: Mutex<Option<(Instant, JwkSet)>>,
}
impl Authority {
    pub(super) fn new(http: Client, endpoints: Endpoints) -> Self {
        Self {
            http,
            endpoints,
            discovery_cache: Mutex::new(None),
            jwks_cache: Mutex::new(None),
        }
    }
    pub(super) async fn discovery(&self) -> Result<Discovery> {
        let mut cache = self.discovery_cache.lock().await;
        if let Some((time, discovery)) = &*cache {
            if time.elapsed() < CACHE_LIFETIME {
                return Ok(discovery.clone());
            }
        }
        let response = self
            .http
            .get(&self.endpoints.discovery)
            .timeout(CREDENTIAL_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(transport_error)?;
        let discovery: Discovery = checked_json(response).await?;
        self.validate_discovery(&discovery)?;
        *cache = Some((Instant::now(), discovery.clone()));
        Ok(discovery)
    }

    pub(super) fn validate_discovery(&self, discovery: &Discovery) -> Result<()> {
        if discovery.issuer != ISSUER {
            return Err(CloudError::InvalidIdentity);
        }
        #[cfg(test)]
        if self.endpoints.discovery.starts_with("http://127.0.0.1:") {
            let origin = Url::parse(&self.endpoints.discovery)
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

    pub(super) async fn verify_identity(
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
        let mut cache = self.jwks_cache.lock().await;
        let refresh = cache.as_ref().is_none_or(|(time, keys)| {
            time.elapsed() >= CACHE_LIFETIME || keys.find(&kid).is_none()
        });
        if refresh {
            let response = self
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
