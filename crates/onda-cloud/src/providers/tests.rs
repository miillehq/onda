use super::*;
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// One captured request: the head (request line and headers, lower-cased names) and the body.
#[derive(Clone, Debug)]
struct Captured {
    head: String,
    body: Value,
}

/// Answers each connection with the next canned response and records what was sent.
struct MockServer {
    base: String,
    requests: Arc<Mutex<Vec<Captured>>>,
}

impl MockServer {
    async fn start(responses: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        tokio::spawn(async move {
            for response in responses {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head, body) = loop {
                    let read = socket.read(&mut chunk).await.unwrap();
                    buffer.extend_from_slice(&chunk[..read]);
                    if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .map_or(0, |value| value.trim().parse::<usize>().unwrap());
                        while buffer.len() < end + 4 + length {
                            let read = socket.read(&mut chunk).await.unwrap();
                            buffer.extend_from_slice(&chunk[..read]);
                        }
                        let body = serde_json::from_slice(&buffer[end + 4..end + 4 + length])
                            .unwrap_or(Value::Null);
                        break (head, body);
                    }
                    if read == 0 {
                        return;
                    }
                };
                captured.lock().unwrap().push(Captured { head, body });
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        });
        Self { base, requests }
    }

    fn requests(&self) -> Vec<Captured> {
        self.requests.lock().unwrap().clone()
    }
}

fn sse(events: &[&str]) -> String {
    let body: String = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len())
}

fn named_sse(events: &[(&str, &str)]) -> String {
    let body: String = events
        .iter()
        .map(|(name, data)| format!("event: {name}\ndata: {data}\n\n"))
        .collect();
    format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}", body.len())
}

fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\nrequest-id: req_123\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn chunk(content: &str) -> String {
    json!({ "choices": [{ "index": 0, "delta": { "content": content }, "finish_reason": null }] })
        .to_string()
}

fn finish(reason: &str) -> String {
    json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": reason }] }).to_string()
}

fn request(model: &str) -> ProviderRequest {
    ProviderRequest {
        model: model.into(),
        instructions: "Fix punctuation. Return only the text.".into(),
        source_text: "hello world".into(),
        context_text: None,
        low_effort: false,
    }
}

async fn respond(
    endpoint: &Endpoint,
    key: Option<&ApiKey>,
    request: &ProviderRequest,
) -> (Result<ResponseResult>, Vec<String>) {
    let mut deltas = Vec::new();
    let result = ProviderClient::new()
        .unwrap()
        .respond(endpoint, key, request, CancellationToken::new(), |delta| {
            deltas.push(delta.to_owned())
        })
        .await;
    (result, deltas)
}

#[test]
fn endpoints_require_https_or_loopback_and_local_requires_loopback() {
    let remote = Endpoint::openai_compatible(" https://api.example.com/v1 ").unwrap();
    assert_eq!(remote.base_url(), "https://api.example.com/v1/");
    assert_eq!(remote.host(), "api.example.com");
    assert!(Endpoint::openai_compatible("http://127.0.0.1:8080/v1").is_ok());
    for refused in [
        "http://api.example.com/v1",
        "ftp://api.example.com",
        "https://user:pass@api.example.com/v1",
        "https://api.example.com/v1?key=1",
        "https://api.example.com/v1#x",
        "api.example.com/v1",
        "",
    ] {
        assert!(Endpoint::openai_compatible(refused).is_err(), "{refused}");
    }
    let local = Endpoint::local("http://localhost:11434/v1").unwrap();
    assert_eq!(local.host(), "127.0.0.1:11434");
    assert!(Endpoint::local("http://127.8.9.1:1234/v1").is_ok());
    assert!(Endpoint::local("http://[::1]:9931/v1").is_ok());
    for refused in [
        "http://192.168.1.20:11434/v1",
        "http://10.0.0.1:11434/v1",
        "https://api.example.com/v1",
        "http://localhost.example.com:11434/v1",
        "http://0.0.0.0:11434/v1",
    ] {
        assert!(Endpoint::local(refused).is_err(), "{refused}");
    }
}

#[tokio::test]
async fn openai_compatible_streams_text_with_bearer_key_and_no_secret_in_the_body() {
    let server = MockServer::start(vec![sse(&[
        &chunk("Hello"),
        &chunk(", world."),
        &finish("stop"),
        "[DONE]",
    ])])
    .await;
    let endpoint = Endpoint::openai_compatible(&server.base).unwrap();
    let key = ApiKey::parse("sk-test-secret").unwrap();
    let (result, deltas) = respond(&endpoint, Some(&key), &request("gpt-test")).await;
    assert_eq!(result.unwrap().text, "Hello, world.");
    assert_eq!(deltas, ["Hello", ", world."]);
    let captured = &server.requests()[0];
    assert!(captured.head.starts_with("post /v1/chat/completions "));
    assert!(captured
        .head
        .contains("authorization: bearer sk-test-secret"));
    assert!(!captured.body.to_string().contains("sk-test-secret"));
    assert_eq!(captured.body["model"], "gpt-test");
    assert_eq!(captured.body["messages"][0]["role"], "system");
    let user: Value =
        serde_json::from_str(captured.body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(user["source_text"], "hello world");
    // Remote servers get no sampling or thinking switches they might reject.
    assert!(captured.body.get("temperature").is_none());
    assert!(captured.body.get("chat_template_kwargs").is_none());
}

#[tokio::test]
async fn local_requests_turn_thinking_off_and_hide_think_blocks() {
    let server = MockServer::start(vec![sse(&[
        &chunk("<thi"),
        &chunk("nk>private reasoning</think>\n\n"),
        &chunk("Hello"),
        &chunk(" world."),
        &finish("stop"),
        "[DONE]",
    ])])
    .await;
    let endpoint = Endpoint::local(&server.base).unwrap();
    let (result, deltas) = respond(&endpoint, None, &request("qwen3.5:0.8b")).await;
    assert_eq!(result.unwrap().text, "Hello world.");
    assert_eq!(deltas.concat(), "Hello world.");
    let captured = &server.requests()[0];
    assert!(!captured.head.contains("authorization:"));
    assert_eq!(
        captured.body["chat_template_kwargs"]["enable_thinking"],
        false
    );
    assert_eq!(captured.body["reasoning_effort"], "none");
    assert_eq!(captured.body["max_tokens"], LOCAL_MAX_TOKENS);
}

#[tokio::test]
async fn local_server_that_rejects_the_thinking_switch_gets_one_plain_retry() {
    let server = MockServer::start(vec![
        json_response(
            "400 Bad Request",
            r#"{"error":{"message":"bad","type":"invalid_request","param":"reasoning_effort","code":"invalid_value"}}"#,
        ),
        sse(&[&chunk("Fine."), &finish("stop"), "[DONE]"]),
    ])
    .await;
    let endpoint = Endpoint::local(&server.base).unwrap();
    let (result, _) = respond(&endpoint, None, &request("local-model")).await;
    assert_eq!(result.unwrap().text, "Fine.");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].body.get("reasoning_effort").is_none());
    assert!(requests[1].body.get("chat_template_kwargs").is_none());
}

#[tokio::test]
async fn unfinished_truncated_and_filtered_streams_are_not_success() {
    for (events, expected) in [
        (vec![chunk("Hel")], "InterruptedStream"),
        (vec![chunk("Hel"), "[DONE]".to_owned()], "InterruptedStream"),
        (
            vec![chunk("Hel"), finish("length"), "[DONE]".to_owned()],
            "IncompleteResponse",
        ),
        (
            vec![chunk("Hel"), finish("content_filter"), "[DONE]".to_owned()],
            "Refusal",
        ),
        (
            vec![finish("stop"), "[DONE]".to_owned()],
            "MalformedResponse",
        ),
        (vec!["{not json".to_owned()], "MalformedResponse"),
    ] {
        let events: Vec<&str> = events.iter().map(String::as_str).collect();
        let server = MockServer::start(vec![sse(&events)]).await;
        let endpoint = Endpoint::local(&server.base).unwrap();
        let (result, _) = respond(&endpoint, None, &request("m")).await;
        let error = result.unwrap_err();
        assert!(
            format!("{error:?}").starts_with(expected),
            "{events:?}: {error:?}"
        );
    }
}

#[tokio::test]
async fn final_chunk_without_done_is_accepted_after_stop() {
    let server = MockServer::start(vec![sse(&[&chunk("Done."), &finish("stop")])]).await;
    let endpoint = Endpoint::local(&server.base).unwrap();
    assert_eq!(
        respond(&endpoint, None, &request("m"))
            .await
            .0
            .unwrap()
            .text,
        "Done."
    );
}

#[tokio::test]
async fn remote_errors_keep_status_and_code_but_never_the_message() {
    let server = MockServer::start(vec![json_response(
        "401 Unauthorized",
        r#"{"error":{"message":"Incorrect API key sk-leaked provided","type":"invalid_request_error","code":"invalid_api_key"}}"#,
    )])
    .await;
    let endpoint = Endpoint::openai_compatible(&server.base).unwrap();
    let (result, _) = respond(&endpoint, Some(&ApiKey::parse("k").unwrap()), &request("m")).await;
    let error = result.unwrap_err();
    let printed = format!("{error:?} {error}");
    assert!(
        matches!(error, CloudError::Remote { status: 401, ref code, .. } if code.as_deref() == Some("invalid_api_key"))
    );
    assert!(!printed.contains("sk-leaked") && !printed.contains("Incorrect"));
}

#[tokio::test]
async fn redirects_are_refused_rather_than_followed() {
    let server = MockServer::start(vec![format!(
        "HTTP/1.1 307 Temporary Redirect\r\nlocation: https://elsewhere.example/v1/chat/completions\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
    )])
    .await;
    let endpoint = Endpoint::local(&server.base).unwrap();
    let (result, _) = respond(&endpoint, None, &request("m")).await;
    assert!(matches!(
        result,
        Err(CloudError::Remote { status: 307, .. })
    ));
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn nothing_listening_locally_is_reported_as_unreachable() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let endpoint = Endpoint::local(&format!("http://{address}/v1")).unwrap();
    let models = ProviderClient::new()
        .unwrap()
        .list_models(&endpoint, None)
        .await;
    assert!(matches!(models, Err(CloudError::Unreachable)), "{models:?}");
}

#[tokio::test]
async fn anthropic_uses_its_own_headers_and_collects_only_text_blocks() {
    let server = MockServer::start(vec![named_sse(&[
        ("message_start", r#"{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}"#),
        ("content_block_start", r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#),
        ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"secret plan"}}"#),
        ("content_block_stop", r#"{"type":"content_block_stop","index":0}"#),
        ("ping", r#"{"type":"ping"}"#),
        ("content_block_start", r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#),
        ("content_block_delta", r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hello,"}}"#),
        ("content_block_delta", r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":" world."}}"#),
        ("content_block_stop", r#"{"type":"content_block_stop","index":1}"#),
        ("message_delta", r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":9}}"#),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ])])
    .await;
    let endpoint = Endpoint::anthropic_at(&server.base);
    let key = ApiKey::parse("sk-ant-test").unwrap();
    let mut polish = request("claude-opus-5-5");
    polish.low_effort = true;
    let (result, deltas) = respond(&endpoint, Some(&key), &polish).await;
    assert_eq!(result.unwrap().text, "Hello, world.");
    assert_eq!(deltas.concat(), "Hello, world.");
    let captured = &server.requests()[0];
    assert!(captured.head.starts_with("post /v1/messages "));
    assert!(captured.head.contains("x-api-key: sk-ant-test"));
    assert!(captured.head.contains("anthropic-version: 2026-10-21"));
    assert!(!captured.head.contains("authorization:"));
    assert_eq!(captured.body["max_tokens"], ANTHROPIC_MAX_TOKENS);
    assert_eq!(
        captured.body["system"],
        "Fix punctuation. Return only the text."
    );
    assert_eq!(captured.body["output_config"]["effort"], "low");
    // The 2026-10-21 API version refuses sampling parameters.
    assert!(captured.body.get("temperature").is_none());
}

#[tokio::test]
async fn anthropic_refusal_and_truncation_fail_and_a_missing_key_never_connects() {
    for (stop_reason, expected) in [("refusal", "Refusal"), ("max_tokens", "IncompleteResponse")] {
        let delta =
            format!(r#"{{"type":"message_delta","delta":{{"stop_reason":"{stop_reason}"}}}}"#);
        let server = MockServer::start(vec![named_sse(&[
            ("message_start", r#"{"type":"message_start","message":{}}"#),
            ("content_block_delta", r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial"}}"#),
            ("message_delta", &delta),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ])])
        .await;
        let endpoint = Endpoint::anthropic_at(&server.base);
        let key = ApiKey::parse("k").unwrap();
        let error = respond(&endpoint, Some(&key), &request("m"))
            .await
            .0
            .unwrap_err();
        assert!(format!("{error:?}").starts_with(expected), "{error:?}");
    }
    let (result, _) = respond(&Endpoint::anthropic(), None, &request("m")).await;
    assert!(matches!(result, Err(CloudError::MissingApiKey)));
}

#[tokio::test]
async fn model_lists_parse_openai_ollama_and_anthropic_shapes() {
    let server = MockServer::start(vec![
        json_response("200 OK", r#"{"object":"list","data":[{"id":"qwen3.5:0.8b","object":"model","owned_by":"library"},{"id":"qwen3.5:0.8b"},{"id":"llama3.2:1b"}]}"#),
        json_response("200 OK", r#"{"data":[{"id":"claude-opus-5-5","display_name":"Claude Opus 5.5","capabilities":{"effort":{"supported":true,"low":{"supported":true}}}},{"id":"claude-haiku-4-5","display_name":"Claude Haiku 4.5","capabilities":{"effort":{"supported":false,"low":{"supported":false}}}}],"has_more":false}"#),
    ])
    .await;
    let client = ProviderClient::new().unwrap();
    let local = client
        .list_models(&Endpoint::local(&server.base).unwrap(), None)
        .await
        .unwrap();
    assert_eq!(
        local
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["qwen3.5:0.8b", "llama3.2:1b"]
    );
    let key = ApiKey::parse("k").unwrap();
    let anthropic = client
        .list_models(&Endpoint::anthropic_at(&server.base), Some(&key))
        .await
        .unwrap();
    assert_eq!(anthropic[0].display_name, "Claude Opus 5.5");
    assert!(anthropic[0].low_effort && !anthropic[1].low_effort);
    assert!(server.requests()[1]
        .head
        .starts_with("get /v1/models?limit=1000 "));
}

#[test]
fn visible_text_releases_nothing_inside_an_unclosed_think_block() {
    let mut text = VisibleText::default();
    assert_eq!(text.push("<think>still").unwrap(), None);
    assert!(matches!(text.finish(), Err(CloudError::IncompleteResponse)));
    let mut plain = VisibleText::default();
    assert_eq!(
        plain.push("<b>bold</b>").unwrap().as_deref(),
        Some("<b>bold</b>")
    );
}

/// Real model, real server: set ONDA_LOCAL_LLM_URL (e.g. http://127.0.0.1:9931/v1) and
/// ONDA_LOCAL_LLM_MODEL, then run with `--ignored`. See docs/local-models.md.
#[tokio::test]
#[ignore]
async fn live_local_server_polishes_text() {
    let base = std::env::var("ONDA_LOCAL_LLM_URL").expect("ONDA_LOCAL_LLM_URL");
    let model = std::env::var("ONDA_LOCAL_LLM_MODEL").expect("ONDA_LOCAL_LLM_MODEL");
    let endpoint = Endpoint::local(&base).unwrap();
    let client = ProviderClient::new().unwrap();
    let models = client.list_models(&endpoint, None).await.unwrap();
    assert!(models.iter().any(|listed| listed.id == model), "{models:?}");
    let polish = ProviderRequest {
        model,
        instructions: "Clean up this dictated text: fix punctuation and capitalization and remove filler words. Keep the wording and meaning. Return only the cleaned text.".into(),
        source_text: "um so the meeting is moved to thursday at 3 pm and uh please bring the q3 numbers".into(),
        context_text: None,
        low_effort: false,
    };
    let started = std::time::Instant::now();
    let (result, deltas) = respond(&endpoint, None, &polish).await;
    let text = result.unwrap().text;
    eprintln!(
        "{:.1} s, {} deltas: {text}",
        started.elapsed().as_secs_f64(),
        deltas.len()
    );
    assert!(text.to_lowercase().contains("thursday"));
    assert!(!text.contains("<think>"));
}
