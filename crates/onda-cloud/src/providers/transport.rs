use super::*;

pub(super) fn authorize(
    request: reqwest::RequestBuilder,
    endpoint: &Endpoint,
    key: Option<&ApiKey>,
) -> Result<reqwest::RequestBuilder> {
    Ok(match (endpoint.kind, key) {
        (ProviderKind::Anthropic, Some(key)) => request
            .header("x-api-key", key.expose())
            .header("anthropic-version", ANTHROPIC_VERSION),
        (ProviderKind::Anthropic, None) => return Err(CloudError::MissingApiKey),
        (_, Some(key)) => request.bearer_auth(key.expose()),
        (_, None) => request,
    })
}

pub(super) fn validate(request: &ProviderRequest) -> Result<()> {
    if request.model.is_empty()
        || request.model.len() > 256
        || request
            .model
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
        || request.instructions.trim().is_empty()
        || request.source_text.trim().is_empty()
    {
        return Err(CloudError::Configuration);
    }
    let bytes = request
        .instructions
        .len()
        .saturating_add(request.source_text.len())
        .saturating_add(request.context_text.as_ref().map_or(0, String::len));
    if bytes > MAX_REQUEST_TEXT_BYTES {
        return Err(CloudError::SizeLimit);
    }
    Ok(())
}

pub(super) fn rejects_thinking_switch(parameter: Option<&str>) -> bool {
    matches!(
        parameter,
        Some("reasoning_effort" | "chat_template_kwargs" | "reasoning")
    )
}

pub(super) fn transport(error: reqwest::Error) -> CloudError {
    if error.is_timeout() {
        CloudError::Timeout
    } else if error.is_connect() {
        CloudError::Unreachable
    } else {
        CloudError::Transport
    }
}

pub(super) async fn read_body(response: Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_JSON_BYTES as u64)
    {
        return Err(CloudError::SizeLimit);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(transport)?;
        if bytes.len() + chunk.len() > MAX_JSON_BYTES {
            return Err(CloudError::SizeLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) async fn read_json(response: Response) -> Result<Value> {
    if !response.status().is_success() {
        return Err(remote_error(response).await);
    }
    serde_json::from_slice(&read_body(response).await?).map_err(|_| CloudError::MalformedResponse)
}

/// Status, error code/type, parameter and request id only; never the message or body.
pub(super) async fn remote_error(response: Response) -> CloudError {
    let status = response.status().as_u16();
    let request_id = ["request-id", "x-request-id"]
        .iter()
        .find_map(|name| response.headers().get(*name))
        .and_then(|value| value.to_str().ok())
        .and_then(safe_identifier);
    let value: Option<Value> = read_body(response)
        .await
        .ok()
        .and_then(|body| serde_json::from_slice(&body).ok());
    let (code, parameter, body_shape) = error_fields(value.as_ref());
    CloudError::Remote {
        status,
        code,
        parameter,
        request_id,
        body_shape: body_shape.into(),
    }
}

pub(super) fn error_fields(
    value: Option<&Value>,
) -> (Option<String>, Option<String>, &'static str) {
    let error = value.and_then(|value| value.get("error"));
    let text = |field: &str| {
        error
            .and_then(|error| error.get(field))
            .and_then(Value::as_str)
            .and_then(safe_identifier)
    };
    let code = text("code")
        .or_else(|| text("type"))
        .or_else(|| error.and_then(Value::as_str).and_then(safe_identifier));
    let shape = match (value, error) {
        (_, Some(_)) => "error",
        (Some(_), None) => "json",
        (None, None) => "unreadable",
    };
    (code, text("param"), shape)
}

pub(super) fn parse_models(kind: ProviderKind, body: &Value) -> Result<Vec<ProviderModel>> {
    let list = body
        .get("data")
        .and_then(Value::as_array)
        // Some servers answer with the list itself or under "models".
        .or_else(|| body.as_array())
        .or_else(|| body.get("models").and_then(Value::as_array))
        .ok_or(CloudError::MalformedResponse)?;
    if list.len() > MAX_MODELS {
        return Err(CloudError::SizeLimit);
    }
    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    for item in list {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            return Err(CloudError::MalformedResponse);
        };
        if id.is_empty() || id.len() > 256 || id.chars().any(|c| c.is_control()) {
            return Err(CloudError::MalformedResponse);
        }
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let display_name = item
            .get("display_name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty() && name.len() <= 256)
            .unwrap_or(id)
            .to_owned();
        let low_effort = kind == ProviderKind::Anthropic
            && item
                .pointer("/capabilities/effort/low/supported")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        models.push(ProviderModel {
            id: id.to_owned(),
            display_name,
            low_effort,
        });
    }
    Ok(models)
}
