//! Text-only inference through providers the user configures: any server that speaks OpenAI's
//! Chat Completions API (remote, or on this computer) and Anthropic's Messages API, each through
//! its own adapter.
//!
//! The caller picks exactly one provider and this module never falls back to another. Requests
//! carry only the instructions and the text; responses are size- and time-bounded; response
//! bodies never reach logs or the interface. Redirects are refused so a key or text cannot be
//! forwarded to a host the user did not name, and the local adapter bypasses system proxies so
//! loopback traffic cannot be routed off the computer.

use crate::{
    keys::ApiKey,
    oauth::safe_identifier,
    sse::{ServerEvent, SseDecoder},
    CloudError, ResponseResult, Result,
};
use futures_util::StreamExt;
use reqwest::{header, redirect, Client, Response};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::{Host, Url};

const MAX_REQUEST_TEXT_BYTES: usize = 128 * 1024;
const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_STREAM_BYTES: usize = 8 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_MODELS: usize = 1000;
const MAX_URL_BYTES: usize = 2048;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
const REMOTE_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const REMOTE_INFERENCE_TIMEOUT: Duration = Duration::from_secs(90);
/// A local server may first have to load the model from disk, which takes a while on a CPU.
const LOCAL_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const LOCAL_INFERENCE_TIMEOUT: Duration = Duration::from_secs(180);
const ANTHROPIC_BASE: &str = "https://api.anthropic.com/v1/";
const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Thinking counts against this on models that always think, so leave room beyond the text.
const ANTHROPIC_MAX_TOKENS: u32 = 16_000;
const LOCAL_MAX_TOKENS: u32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    OpenaiCompatible,
    Anthropic,
    Local,
}

/// A validated provider address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    kind: ProviderKind,
    base: Url,
}

impl Endpoint {
    /// HTTPS anywhere, or plain HTTP only to this computer.
    pub fn openai_compatible(base_url: &str) -> Result<Self> {
        let base = parse_base(base_url)?;
        if base.scheme() != "https" && !is_loopback(&base) {
            return Err(CloudError::EndpointNotAllowed);
        }
        Ok(Self {
            kind: ProviderKind::OpenaiCompatible,
            base,
        })
    }

    /// Loopback only (127.0.0.0/8, ::1 or localhost), so text never leaves the computer.
    pub fn local(base_url: &str) -> Result<Self> {
        let mut base = parse_base(base_url)?;
        if !is_loopback(&base) {
            return Err(CloudError::EndpointNotAllowed);
        }
        // Resolve "localhost" ourselves; a hosts-file or resolver entry cannot redirect it.
        if base.host_str() == Some("localhost") {
            base.set_host(Some("127.0.0.1"))
                .map_err(|_| CloudError::EndpointNotAllowed)?;
        }
        Ok(Self {
            kind: ProviderKind::Local,
            base,
        })
    }

    pub fn anthropic() -> Self {
        Self {
            kind: ProviderKind::Anthropic,
            base: Url::parse(ANTHROPIC_BASE).expect("fixed Anthropic URL"),
        }
    }

    #[cfg(test)]
    fn anthropic_at(base: &str) -> Self {
        Self {
            kind: ProviderKind::Anthropic,
            base: parse_base(base).unwrap(),
        }
    }

    pub fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Host and port for "text is sent to …" labels.
    pub fn host(&self) -> String {
        let host = self.base.host_str().unwrap_or_default();
        match self.base.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        }
    }

    /// The normalized base URL, ending in a slash.
    pub fn base_url(&self) -> &str {
        self.base.as_str()
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base.join(path).map_err(|_| CloudError::Configuration)
    }
}

fn parse_base(raw: &str) -> Result<Url> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_URL_BYTES {
        return Err(CloudError::EndpointNotAllowed);
    }
    let mut url = Url::parse(raw).map_err(|_| CloudError::EndpointNotAllowed)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CloudError::EndpointNotAllowed);
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

/// A model the provider lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderModel {
    pub id: String,
    pub display_name: String,
    /// Anthropic models that accept `effort: "low"`, which keeps cleanup quick.
    pub low_effort: bool,
}

/// One text request. Instructions are separate from the JSON-encoded source and context data.
#[derive(Clone, Debug)]
pub struct ProviderRequest {
    pub model: String,
    pub instructions: String,
    pub source_text: String,
    pub context_text: Option<String>,
    pub low_effort: bool,
}

#[derive(Clone)]
pub struct ProviderClient {
    remote: Client,
    local: Client,
}

impl ProviderClient {
    pub fn new() -> Result<Self> {
        let builder = || {
            Client::builder()
                .redirect(redirect::Policy::none())
                .connect_timeout(CONNECT_TIMEOUT)
                .user_agent(concat!("Onda/", env!("CARGO_PKG_VERSION")))
        };
        Ok(Self {
            remote: builder().build().map_err(|_| CloudError::Configuration)?,
            local: builder()
                .no_proxy()
                .build()
                .map_err(|_| CloudError::Configuration)?,
        })
    }

    fn http(&self, endpoint: &Endpoint) -> &Client {
        if endpoint.kind == ProviderKind::Local {
            &self.local
        } else {
            &self.remote
        }
    }

    /// Lists models in the provider's order. Also the availability check for a local server.
    pub async fn list_models(
        &self,
        endpoint: &Endpoint,
        key: Option<&ApiKey>,
    ) -> Result<Vec<ProviderModel>> {
        let mut request = self
            .http(endpoint)
            .get(endpoint.url(match endpoint.kind {
                ProviderKind::Anthropic => "models?limit=1000",
                _ => "models",
            })?)
            .timeout(CATALOG_TIMEOUT);
        request = authorize(request, endpoint, key)?;
        let response = request.send().await.map_err(transport)?;
        let body = read_json(response).await?;
        parse_models(endpoint.kind, &body)
    }

    /// Streams one completion; success requires the provider's explicit, complete end.
    pub async fn respond<F>(
        &self,
        endpoint: &Endpoint,
        key: Option<&ApiKey>,
        request: &ProviderRequest,
        cancellation: CancellationToken,
        mut on_delta: F,
    ) -> Result<ResponseResult>
    where
        F: FnMut(&str) + Send,
    {
        validate(request)?;
        let total = if endpoint.kind == ProviderKind::Local {
            LOCAL_INFERENCE_TIMEOUT
        } else {
            REMOTE_INFERENCE_TIMEOUT
        };
        tokio::select! {
            _ = cancellation.cancelled() => Err(CloudError::Cancelled),
            result = tokio::time::timeout(total, self.respond_inner(endpoint, key, request, &mut on_delta)) => result.map_err(|_| CloudError::Timeout)?,
        }
    }

    async fn respond_inner<F>(
        &self,
        endpoint: &Endpoint,
        key: Option<&ApiKey>,
        request: &ProviderRequest,
        on_delta: &mut F,
    ) -> Result<ResponseResult>
    where
        F: FnMut(&str) + Send,
    {
        let source =
            json!({ "source_text": request.source_text, "context_text": request.context_text })
                .to_string();
        match endpoint.kind {
            ProviderKind::Anthropic => {
                let mut body = json!({
                    "model": request.model,
                    "max_tokens": ANTHROPIC_MAX_TOKENS,
                    "system": request.instructions,
                    "messages": [{ "role": "user", "content": source }],
                    "stream": true,
                });
                if request.low_effort {
                    body["output_config"] = json!({ "effort": "low" });
                }
                let response = self.post(endpoint, key, "messages", &body).await?;
                self.stream(endpoint, response, AnthropicStream::default(), on_delta)
                    .await
            }
            ProviderKind::OpenaiCompatible | ProviderKind::Local => {
                let local = endpoint.kind == ProviderKind::Local;
                let mut body = json!({
                    "model": request.model,
                    "messages": [
                        { "role": "system", "content": request.instructions },
                        { "role": "user", "content": source },
                    ],
                    "stream": true,
                });
                if local {
                    // Small local models think by default under llama.cpp and Ollama, which is
                    // slow and can loop; ask both conventions for a direct answer. Sampling is set
                    // here because some servers ship creative-writing defaults.
                    body["max_tokens"] = json!(LOCAL_MAX_TOKENS);
                    body["temperature"] = json!(0.2);
                    body["reasoning_effort"] = json!("none");
                    body["chat_template_kwargs"] = json!({ "enable_thinking": false });
                }
                let response = match self.post(endpoint, key, "chat/completions", &body).await {
                    // Servers that reject the thinking switches get one plain retry on the same
                    // local server: still nothing leaves the computer and nothing is billed.
                    Err(CloudError::Remote {
                        status: 400 | 422,
                        parameter,
                        ..
                    }) if local && rejects_thinking_switch(parameter.as_deref()) => {
                        if let Some(object) = body.as_object_mut() {
                            object.remove("reasoning_effort");
                            object.remove("chat_template_kwargs");
                        }
                        self.post(endpoint, key, "chat/completions", &body).await?
                    }
                    result => result?,
                };
                self.stream(endpoint, response, ChatStream::default(), on_delta)
                    .await
            }
        }
    }

    async fn post(
        &self,
        endpoint: &Endpoint,
        key: Option<&ApiKey>,
        path: &str,
        body: &Value,
    ) -> Result<Response> {
        let request = self
            .http(endpoint)
            .post(endpoint.url(path)?)
            .header(header::ACCEPT, "text/event-stream")
            .json(body);
        let response = authorize(request, endpoint, key)?
            .send()
            .await
            .map_err(transport)?;
        if !response.status().is_success() {
            return Err(remote_error(response).await);
        }
        Ok(response)
    }

    async fn stream<S, F>(
        &self,
        endpoint: &Endpoint,
        response: Response,
        mut state: S,
        on_delta: &mut F,
    ) -> Result<ResponseResult>
    where
        S: StreamState,
        F: FnMut(&str) + Send,
    {
        let idle = if endpoint.kind == ProviderKind::Local {
            LOCAL_IDLE_TIMEOUT
        } else {
            REMOTE_IDLE_TIMEOUT
        };
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut text = VisibleText::default();
        let mut received = 0usize;
        loop {
            let chunk = tokio::time::timeout(idle, stream.next())
                .await
                .map_err(|_| CloudError::Timeout)?;
            let Some(chunk) = chunk else {
                // Some servers close right after their final chunk instead of sending [DONE].
                return if state.finished() {
                    text.finish()
                } else {
                    Err(CloudError::InterruptedStream)
                };
            };
            let chunk = chunk.map_err(transport)?;
            received = received.saturating_add(chunk.len());
            if received > MAX_STREAM_BYTES {
                return Err(CloudError::SizeLimit);
            }
            for event in decoder.push(&chunk)? {
                match state.consume(&event)? {
                    Step::Delta(delta) => {
                        if let Some(visible) = text.push(&delta)? {
                            on_delta(&visible);
                        }
                    }
                    Step::Continue => {}
                    Step::Done => return text.finish(),
                }
            }
        }
    }
}

fn authorize(
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

fn validate(request: &ProviderRequest) -> Result<()> {
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

fn rejects_thinking_switch(parameter: Option<&str>) -> bool {
    matches!(
        parameter,
        Some("reasoning_effort" | "chat_template_kwargs" | "reasoning")
    )
}

fn transport(error: reqwest::Error) -> CloudError {
    if error.is_timeout() {
        CloudError::Timeout
    } else if error.is_connect() {
        CloudError::Unreachable
    } else {
        CloudError::Transport
    }
}

async fn read_body(response: Response) -> Result<Vec<u8>> {
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

async fn read_json(response: Response) -> Result<Value> {
    if !response.status().is_success() {
        return Err(remote_error(response).await);
    }
    serde_json::from_slice(&read_body(response).await?).map_err(|_| CloudError::MalformedResponse)
}

/// Status, error code/type, parameter and request id only; never the message or body.
async fn remote_error(response: Response) -> CloudError {
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

fn error_fields(value: Option<&Value>) -> (Option<String>, Option<String>, &'static str) {
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

fn parse_models(kind: ProviderKind, body: &Value) -> Result<Vec<ProviderModel>> {
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

enum Step {
    Delta(String),
    Continue,
    Done,
}

trait StreamState {
    fn consume(&mut self, event: &ServerEvent) -> Result<Step>;
    /// The provider has said the answer is complete.
    fn finished(&self) -> bool;
}

/// OpenAI Chat Completions chunks: `choices[0].delta.content`, a `finish_reason`, then `[DONE]`.
#[derive(Default)]
struct ChatStream {
    finish: Option<String>,
}

impl StreamState for ChatStream {
    fn consume(&mut self, event: &ServerEvent) -> Result<Step> {
        if event.data.trim() == "[DONE]" {
            return if self.finished() {
                Ok(Step::Done)
            } else {
                Err(self.unfinished())
            };
        }
        let value: Value =
            serde_json::from_str(&event.data).map_err(|_| CloudError::MalformedResponse)?;
        if value.get("error").is_some() {
            let (code, parameter, _) = error_fields(Some(&value));
            return Err(CloudError::Remote {
                status: 200,
                code,
                parameter,
                request_id: None,
                body_shape: "stream error".into(),
            });
        }
        let Some(choice) = value.pointer("/choices/0") else {
            // Usage-only and keep-alive chunks carry no choice.
            return Ok(Step::Continue);
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish = Some(reason.to_owned());
        }
        // Reasoning arrives in separate fields (`reasoning_content`, `reasoning`) and is ignored.
        match choice.pointer("/delta/content") {
            Some(Value::String(delta)) if !delta.is_empty() => Ok(Step::Delta(delta.clone())),
            Some(Value::String(_) | Value::Null) | None => Ok(Step::Continue),
            Some(_) => Err(CloudError::MalformedResponse),
        }
    }

    fn finished(&self) -> bool {
        self.finish.as_deref() == Some("stop")
    }
}

impl ChatStream {
    fn unfinished(&self) -> CloudError {
        match self.finish.as_deref() {
            Some("content_filter") => CloudError::Refusal,
            Some("length") => CloudError::IncompleteResponse,
            Some(_) => CloudError::MalformedResponse,
            None => CloudError::InterruptedStream,
        }
    }
}

/// Anthropic Messages events: text from `text_delta` in text blocks, `message_delta` with the
/// stop reason, then `message_stop`. Thinking blocks are never collected.
#[derive(Default)]
struct AnthropicStream {
    started: bool,
    stop_reason: Option<String>,
}

impl StreamState for AnthropicStream {
    fn consume(&mut self, event: &ServerEvent) -> Result<Step> {
        let value: Value =
            serde_json::from_str(&event.data).map_err(|_| CloudError::MalformedResponse)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or(CloudError::MalformedResponse)?;
        match kind {
            "message_start" => self.started = true,
            "content_block_delta"
                if self.started
                    && value.pointer("/delta/type").and_then(Value::as_str)
                        == Some("text_delta") =>
            {
                let text = value
                    .pointer("/delta/text")
                    .and_then(Value::as_str)
                    .ok_or(CloudError::MalformedResponse)?;
                return Ok(Step::Delta(text.to_owned()));
            }
            // Thinking and signature deltas.
            "content_block_delta" if self.started => {}
            "message_delta" => {
                if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(reason.to_owned());
                }
            }
            "message_stop" => {
                return match self.stop_reason.as_deref() {
                    Some("end_turn" | "stop_sequence") if self.started => Ok(Step::Done),
                    Some("refusal") => Err(CloudError::Refusal),
                    Some("max_tokens") => Err(CloudError::IncompleteResponse),
                    _ => Err(CloudError::MalformedResponse),
                };
            }
            "error" => {
                let (code, parameter, _) = error_fields(Some(&value));
                return Err(CloudError::Remote {
                    status: 200,
                    code,
                    parameter,
                    request_id: None,
                    body_shape: "stream error".into(),
                });
            }
            "content_block_start" | "content_block_stop" | "ping" => {}
            "content_block_delta" => return Err(CloudError::MalformedResponse),
            // New event types are allowed by Anthropic's versioning policy.
            _ => {}
        }
        Ok(Step::Continue)
    }

    fn finished(&self) -> bool {
        false
    }
}

/// The answer without a leading `<think>…</think>` block, which some models put in the text.
/// Deltas are released only once it is clear they are not inside such a block.
#[derive(Default)]
struct VisibleText {
    raw: String,
    emitted: usize,
}

impl VisibleText {
    const OPEN: &'static str = "<think>";
    const CLOSE: &'static str = "</think>";

    fn push(&mut self, delta: &str) -> Result<Option<String>> {
        if self.raw.len().saturating_add(delta.len()) > MAX_OUTPUT_BYTES {
            return Err(CloudError::SizeLimit);
        }
        self.raw.push_str(delta);
        let Some(visible) = Self::visible(&self.raw) else {
            return Ok(None);
        };
        if visible.len() <= self.emitted {
            return Ok(None);
        }
        let fresh = visible[self.emitted..].to_owned();
        self.emitted = visible.len();
        Ok(Some(fresh))
    }

    /// `None` while undecided: a possible `<think>` prefix, or an unclosed block.
    fn visible(raw: &str) -> Option<&str> {
        let trimmed = raw.trim_start();
        if trimmed.len() < Self::OPEN.len() && Self::OPEN.starts_with(trimmed) {
            return None;
        }
        match trimmed.strip_prefix(Self::OPEN) {
            Some(rest) => rest
                .find(Self::CLOSE)
                .map(|end| rest[end + Self::CLOSE.len()..].trim_start()),
            None => Some(raw),
        }
    }

    fn finish(self) -> Result<ResponseResult> {
        if self.raw.trim().is_empty() {
            return Err(CloudError::MalformedResponse);
        }
        let text = Self::visible(&self.raw).ok_or(CloudError::IncompleteResponse)?;
        if text.trim().is_empty() {
            return Err(CloudError::MalformedResponse);
        }
        Ok(ResponseResult {
            text: text.to_owned(),
            response_id: None,
        })
    }
}

#[cfg(test)]
mod tests;
