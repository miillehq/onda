//! Account-specific model discovery and bounded, cancellable text-only inference.

use crate::{
    oauth::{checked_json, remote_error, safe_identifier, transport_error},
    sse::{ServerEvent, SseDecoder},
    ChatGptService, CloudError, ModelChoice, ResponseRequest, ResponseResult, Result,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MAX_REQUEST_TEXT_BYTES: usize = 128 * 1024;
const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_STREAM_BYTES: usize = 8 * 1024 * 1024;
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(20);
const INFERENCE_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_MODEL_CHOICES: usize = 256;
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
struct ModelCatalog {
    models: Vec<CatalogModel>,
}
#[derive(Deserialize)]
struct CatalogModel {
    slug: String,
    display_name: String,
    visibility: String,
}

#[derive(Serialize)]
struct PlanRequest<'a> {
    model: &'a str,
    instructions: &'a str,
    input: Vec<InputMessage>,
    store: bool,
    stream: bool,
}

#[derive(Serialize)]
struct InputMessage {
    role: &'static str,
    content: String,
}

impl ChatGptService {
    /// Returns displayable models for the selected account in server order.
    pub async fn list_models(&self, profile_id: Option<String>) -> Result<Vec<ModelChoice>> {
        let profile_id = self.resolve_profile(profile_id).await?;
        let session_cancel = self.session_cancellation(&profile_id).await;
        tokio::select! {
            biased;
            _ = session_cancel.cancelled() => Err(CloudError::Cancelled),
            result = self.list_models_inner(&profile_id) => result,
        }
    }

    async fn list_models_inner(&self, profile_id: &str) -> Result<Vec<ModelChoice>> {
        let token = self.access_token(profile_id).await?;
        let response = self
            .inner
            .http
            .get(&self.inner.endpoints.models)
            .timeout(CATALOG_TIMEOUT)
            .bearer_auth(token.as_str())
            .send()
            .await
            .map_err(transport_error)?;
        let catalog: ModelCatalog = checked_json(response).await?;
        if catalog.models.len() > MAX_MODEL_CHOICES {
            return Err(CloudError::SizeLimit);
        }
        let mut models = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for model in catalog.models {
            if model.visibility != "list" {
                continue;
            }
            if safe_identifier(&model.slug).is_none()
                || model.display_name.is_empty()
                || model.display_name.len() > 256
            {
                return Err(CloudError::MalformedResponse);
            }
            if !seen.insert(model.slug.clone()) {
                return Err(CloudError::MalformedResponse);
            }
            models.push(ModelChoice {
                slug: model.slug,
                display_name: model.display_name,
            });
        }
        Ok(models)
    }

    /// Streams eligible plan inference; success requires response.completed.
    ///
    /// Caller-owned instructions are separate from source/context JSON data.
    /// Only text is sent: this API cannot accept microphone audio or API keys.
    pub async fn respond<F>(
        &self,
        request: ResponseRequest,
        cancellation: CancellationToken,
        mut on_delta: F,
    ) -> Result<ResponseResult>
    where
        F: FnMut(&str) + Send,
    {
        validate_request(&request)?;
        let profile_id = self.resolve_profile(request.profile_id.clone()).await?;
        let session_cancel = self.session_cancellation(&profile_id).await;
        tokio::select! {
            _ = cancellation.cancelled() => Err(CloudError::Cancelled),
            _ = session_cancel.cancelled() => Err(CloudError::Cancelled),
            result = tokio::time::timeout(INFERENCE_TIMEOUT, self.respond_inner(&profile_id, &request, &mut on_delta)) => result.map_err(|_| CloudError::Timeout)?,
        }
    }

    async fn respond_inner<F>(
        &self,
        profile_id: &str,
        request: &ResponseRequest,
        on_delta: &mut F,
    ) -> Result<ResponseResult>
    where
        F: FnMut(&str) + Send,
    {
        let token = self.access_token(profile_id).await?;
        let source = serde_json::json!({ "source_text": request.source_text, "context_text": request.context_text });
        let body = PlanRequest {
            model: &request.model,
            instructions: &request.instructions,
            input: vec![InputMessage {
                role: "user",
                content: source.to_string(),
            }],
            store: false,
            stream: true,
        };
        let response = self
            .inner
            .http
            .post(&self.inner.endpoints.responses)
            .bearer_auth(token.as_str())
            .json(&body)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(transport_error)?;
        if !response.status().is_success() {
            return Err(remote_error(response).await);
        }
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|value| value.to_str().map(media_type).unwrap_or_default());
        // The plan endpoint streams without a Content-Type header; the event parser and the
        // required response.completed still validate the body. A stated non-stream type is refused.
        if let Some(kind) = media_type.filter(|kind| kind != "text/event-stream") {
            let observed = if kind.len() <= 64
                && kind
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"/+.-".contains(&byte))
            {
                kind
            } else {
                "unusual".into()
            };
            tracing::warn!(subsystem = "chatgpt", media_type = %observed, "response is not an event stream");
            return Err(CloudError::MalformedResponse);
        }
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut response_state = ResponseState::default();
        let mut received_bytes = 0usize;
        loop {
            let chunk = tokio::time::timeout(STREAM_IDLE_TIMEOUT, stream.next())
                .await
                .map_err(|_| CloudError::Timeout)?;
            let Some(chunk) = chunk else {
                return Err(CloudError::InterruptedStream);
            };
            let chunk = chunk.map_err(transport_error)?;
            received_bytes = received_bytes
                .checked_add(chunk.len())
                .ok_or(CloudError::SizeLimit)?;
            if received_bytes > MAX_STREAM_BYTES {
                return Err(CloudError::SizeLimit);
            }
            for event in decoder.push(&chunk)? {
                if response_state.consume(event, on_delta)? {
                    return Ok(ResponseResult {
                        text: response_state.text,
                        response_id: response_state.response_id,
                    });
                }
            }
        }
    }
}

fn validate_request(request: &ResponseRequest) -> Result<()> {
    if safe_identifier(&request.model).is_none()
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

#[derive(Default)]
struct ResponseState {
    text: String,
    /// Full message text from `response.output_item.done`, a cross-check for the deltas.
    item_text: String,
    response_id: Option<String>,
    last_sequence_number: Option<u64>,
}

impl ResponseState {
    /// Time O(event bytes), space O(maximum output bytes).
    fn consume<F>(&mut self, event: ServerEvent, on_delta: &mut F) -> Result<bool>
    where
        F: FnMut(&str),
    {
        if event.data == "[DONE]" {
            return Err(CloudError::InterruptedStream);
        }
        let value: serde_json::Value =
            serde_json::from_str(&event.data).map_err(|_| malformed("event is not JSON", ""))?;
        let event_type = value
            .get("type")
            .and_then(|value| value.as_str())
            .ok_or_else(|| malformed("event has no type", ""))?;
        if event
            .event_type
            .as_deref()
            .is_some_and(|kind| kind != event_type && kind != "message")
        {
            return Err(malformed("event name differs from its type", event_type));
        }
        if let Some(sequence) = value
            .get("sequence_number")
            .and_then(|value| value.as_u64())
        {
            if self
                .last_sequence_number
                .is_some_and(|previous| sequence <= previous)
            {
                return Err(malformed("sequence number went backwards", event_type));
            }
            self.last_sequence_number = Some(sequence);
        }
        match event_type {
            "response.output_text.delta" => {
                let delta = value
                    .get("delta")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| malformed("delta has no text", event_type))?;
                if self.text.len().saturating_add(delta.len()) > MAX_OUTPUT_BYTES {
                    return Err(CloudError::SizeLimit);
                }
                self.text.push_str(delta);
                on_delta(delta);
            }
            "response.output_item.done" => {
                if let Some(item) = value.get("item") {
                    let text = message_text(item, event_type)?;
                    if self.item_text.len().saturating_add(text.len()) > MAX_OUTPUT_BYTES {
                        return Err(CloudError::SizeLimit);
                    }
                    self.item_text.push_str(&text);
                }
            }
            "response.completed" => {
                let response = value
                    .get("response")
                    .ok_or_else(|| malformed("completion has no response", event_type))?;
                if response.get("status").and_then(|status| status.as_str()) != Some("completed") {
                    return Err(malformed("completion status is not completed", event_type));
                }
                // The plan endpoint completes with an empty `output`; the finished items arrive
                // as `response.output_item.done`. Whichever full text is present must equal the
                // deltas, so a dropped delta is still caught.
                let final_text = completed_text(response)?;
                let reference = if final_text.is_empty() {
                    std::mem::take(&mut self.item_text)
                } else {
                    final_text
                };
                if self.text.is_empty() {
                    self.text = reference.clone();
                }
                if self.text.trim().is_empty() {
                    return Err(malformed("no text was streamed", event_type));
                }
                if !reference.is_empty() && self.text != reference {
                    return Err(malformed(
                        "completed text differs from the stream",
                        event_type,
                    ));
                }
                self.response_id = response
                    .get("id")
                    .and_then(|value| value.as_str())
                    .and_then(safe_identifier);
                return Ok(true);
            }
            "response.failed" => {
                let error = value
                    .get("response")
                    .and_then(|response| response.get("error"));
                return Err(stream_error(error));
            }
            "response.incomplete" => return Err(CloudError::IncompleteResponse),
            "error" => return Err(stream_error(value.get("error").or(Some(&value)))),
            _ => {}
        }
        Ok(false)
    }
}

/// "Text/Event-Stream ; charset=utf-8" → "text/event-stream".
fn media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// Protocol shape only (a fixed reason and the event type), never response text.
fn malformed(reason: &'static str, event_type: &str) -> CloudError {
    tracing::warn!(
        subsystem = "chatgpt",
        reason,
        event_type,
        "malformed response stream"
    );
    CloudError::MalformedResponse
}

fn stream_error(error: Option<&serde_json::Value>) -> CloudError {
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(|code| code.as_str())
        .and_then(safe_identifier);
    let parameter = error
        .and_then(|error| error.get("param"))
        .and_then(|param| param.as_str())
        .and_then(safe_identifier);
    CloudError::Remote {
        status: 200,
        code,
        parameter,
        request_id: None,
        body_shape: "stream_error".into(),
    }
}

fn completed_text(response: &serde_json::Value) -> Result<String> {
    let output = response
        .get("output")
        .and_then(|output| output.as_array())
        .ok_or_else(|| malformed("completion has no output list", "response.completed"))?;
    let mut result = String::new();
    for item in output {
        let text = message_text(item, "response.completed")?;
        if result.len().saturating_add(text.len()) > MAX_OUTPUT_BYTES {
            return Err(CloudError::SizeLimit);
        }
        result.push_str(&text);
    }
    Ok(result)
}

/// Output text of one message item; other item kinds (reasoning, tool calls) contribute none.
fn message_text(item: &serde_json::Value, event_type: &str) -> Result<String> {
    if item.get("type").and_then(|kind| kind.as_str()) != Some("message") {
        return Ok(String::new());
    }
    let content = item
        .get("content")
        .and_then(|content| content.as_array())
        .ok_or_else(|| malformed("message has no content list", event_type))?;
    let mut result = String::new();
    for block in content {
        if block.get("type").and_then(|kind| kind.as_str()) == Some("refusal") {
            return Err(CloudError::IncompleteResponse);
        }
        if block.get("type").and_then(|kind| kind.as_str()) != Some("output_text") {
            continue;
        }
        let text = block
            .get("text")
            .and_then(|text| text.as_str())
            .ok_or_else(|| malformed("output text has no text", event_type))?;
        if result.len().saturating_add(text.len()) > MAX_OUTPUT_BYTES {
            return Err(CloudError::SizeLimit);
        }
        result.push_str(text);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn event_stream_media_type_ignores_case_spacing_and_parameters() {
        assert_eq!(media_type("text/event-stream"), "text/event-stream");
        assert_eq!(
            media_type("Text/Event-Stream ; charset=utf-8"),
            "text/event-stream"
        );
        assert_eq!(
            media_type(" text/event-stream;charset=UTF-8"),
            "text/event-stream"
        );
        assert_ne!(media_type("application/json"), "text/event-stream");
    }

    use super::*;

    fn event(value: serde_json::Value) -> ServerEvent {
        ServerEvent {
            event_type: None,
            data: value.to_string(),
        }
    }

    #[test]
    fn completion_must_match_the_entire_streamed_text() {
        let mut state = ResponseState::default();
        let mut deltas = String::new();
        state.consume(event(serde_json::json!({"type":"response.output_text.delta","delta":"Hello","sequence_number":1})), &mut |delta| deltas.push_str(delta)).unwrap();
        let completed = serde_json::json!({"type":"response.completed","sequence_number":2,"response":{"id":"resp_test","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Hello"}]}]}});
        assert!(state.consume(event(completed), &mut |_| {}).unwrap());
        assert_eq!(deltas, "Hello");
        let mut mismatch = ResponseState {
            text: "Lost".into(),
            ..Default::default()
        };
        assert!(matches!(mismatch.consume(event(serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Lost delta"}]}]}})), &mut |_| {}), Err(CloudError::MalformedResponse)));
    }

    /// The ChatGPT plan endpoint finishes with `output: []` and reports items separately.
    #[test]
    fn plan_completion_with_empty_output_checks_finished_items() {
        let item = |text: &str| {
            event(
                serde_json::json!({"type":"response.output_item.done","item":{"type":"message","content":[{"type":"output_text","text":text}]}}),
            )
        };
        let completed = || {
            event(
                serde_json::json!({"type":"response.completed","response":{"id":"resp_plan","status":"completed","output":[]}}),
            )
        };
        let delta = |text: &str| {
            event(serde_json::json!({"type":"response.output_text.delta","delta":text}))
        };

        let mut state = ResponseState::default();
        state.consume(delta("Hello "), &mut |_| {}).unwrap();
        state.consume(delta("there."), &mut |_| {}).unwrap();
        state.consume(event(serde_json::json!({"type":"response.output_item.done","item":{"type":"reasoning","summary":[]}})), &mut |_| {}).unwrap();
        state.consume(item("Hello there."), &mut |_| {}).unwrap();
        assert!(state.consume(completed(), &mut |_| {}).unwrap());
        assert_eq!(state.text, "Hello there.");

        let mut lost = ResponseState::default();
        lost.consume(delta("Hello "), &mut |_| {}).unwrap();
        lost.consume(item("Hello there."), &mut |_| {}).unwrap();
        assert!(matches!(
            lost.consume(completed(), &mut |_| {}),
            Err(CloudError::MalformedResponse)
        ));

        let mut deltas_only = ResponseState::default();
        deltas_only
            .consume(delta("Only deltas."), &mut |_| {})
            .unwrap();
        assert!(deltas_only.consume(completed(), &mut |_| {}).unwrap());

        let mut items_only = ResponseState::default();
        items_only.consume(item("No deltas."), &mut |_| {}).unwrap();
        assert!(items_only.consume(completed(), &mut |_| {}).unwrap());
        assert_eq!(items_only.text, "No deltas.");

        let mut empty = ResponseState::default();
        assert!(matches!(
            empty.consume(completed(), &mut |_| {}),
            Err(CloudError::MalformedResponse)
        ));
    }

    #[test]
    fn partial_output_followed_by_usage_error_is_never_a_success() {
        let mut state = ResponseState::default();
        state
            .consume(
                event(serde_json::json!({"type":"response.output_text.delta","delta":"Partial"})),
                &mut |_| {},
            )
            .unwrap();
        let error = state.consume(event(serde_json::json!({"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}})), &mut |_| {}).unwrap_err();
        assert!(
            matches!(error, CloudError::Remote { code: Some(code), .. } if code == "subscription_sharing_usage_limit_exceeded")
        );
    }

    #[test]
    fn rejects_duplicate_events_and_done_without_completed() {
        let mut state = ResponseState::default();
        state
            .consume(
                event(serde_json::json!({"type":"response.created","sequence_number":1})),
                &mut |_| {},
            )
            .unwrap();
        assert!(matches!(state.consume(event(serde_json::json!({"type":"response.output_text.delta","sequence_number":1,"delta":"duplicated"})), &mut |_| {}), Err(CloudError::MalformedResponse)));
        assert!(matches!(
            ResponseState::default().consume(
                ServerEvent {
                    event_type: None,
                    data: "[DONE]".into()
                },
                &mut |_| {}
            ),
            Err(CloudError::InterruptedStream)
        ));
    }
}
