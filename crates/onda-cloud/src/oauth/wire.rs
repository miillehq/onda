use super::*;

#[derive(Deserialize)]
pub(super) struct IdentityClaims {
    pub(super) sub: String,
    pub(super) iat: u64,
    pub(super) nonce: Option<String>,
    pub(super) email: Option<String>,
    pub(super) name: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct TokenResponse {
    #[serde(default)]
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) id_token: Option<String>,
    #[serde(default)]
    pub(super) token_type: String,
    #[serde(default)]
    pub(super) expires_in: u64,
    pub(super) scope: Option<String>,
    #[serde(default, deserialize_with = "deserialize_refresh_hint")]
    pub(super) earliest_refresh_at: Option<u64>,
}

/// The reference names this optional field without fixing its JSON type. Accept
/// epoch hints when recognizable; expiry remains the standard renewal contract.
pub(super) fn deserialize_refresh_hint<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<u64>, D::Error>
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
    pub(super) fn into_secrets(mut self, previous: Option<&SecretTokens>) -> Result<SecretTokens> {
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

pub(super) fn terminal_refresh_error(error: &CloudError) -> bool {
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
