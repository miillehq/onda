use super::*;

pub(super) enum Step {
    Delta(String),
    Continue,
    Done,
}

/// OpenAI Chat Completions chunks: `choices[0].delta.content`, a `finish_reason`, then `[DONE]`.
#[derive(Default)]
pub(super) struct ChatStream {
    pub(super) finish: Option<String>,
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
    pub(super) fn unfinished(&self) -> CloudError {
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
pub(super) struct AnthropicStream {
    pub(super) started: bool,
    pub(super) stop_reason: Option<String>,
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
pub(super) struct VisibleText {
    pub(super) raw: String,
    pub(super) emitted: usize,
}

impl VisibleText {
    const OPEN: &'static str = "<think>";
    const CLOSE: &'static str = "</think>";

    pub(super) fn push(&mut self, delta: &str) -> Result<Option<String>> {
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
    pub(super) fn visible(raw: &str) -> Option<&str> {
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

    pub(super) fn finish(self) -> Result<ResponseResult> {
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
