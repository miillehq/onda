# Sign in with ChatGPT

Checked against official OpenAI documentation on **2026-10-03**. This implementation uses the public open-source ChatGPT plan flow. It requires an eligible account and user consent; it does not use an embedded client secret, a partner API key, or a copied token from another application.

## Native API

`onda-cloud` owns authorization, credentials, account selection, model discovery, and streamed text inference. The Tauri layer may serialize only its public account/model/result/error types. `ChatGptService` and `PendingSignIn` deliberately implement neither `Serialize` nor `Debug`.

```rust
use onda_cloud::{ChatGptService, CloudConfig, CancellationToken, ResponseRequest};

let service = ChatGptService::new(CloudConfig {
    data_directory: absolute_application_data_directory.join("chatgpt"),
    app_name: application_brand_name.to_owned(),
})?;
let account = service.sign_in(None, CancellationToken::new()).await?;
let models = service.list_models(Some(account.id.clone())).await?;
// The application presents these account-specific choices and persists a slug.
```

`begin_sign_in` binds the listener; `PendingSignIn::open_browser` opens the system browser; `complete_sign_in` consumes the transaction. `sign_in` combines those operations. The browser, not the renderer, performs authentication and consent. An explicit `enable_plan_usage` action requests renewed consent for an existing registration; ordinary sign-in does not force consent.

`list_profiles`, `select_profile`, `set_preferred_model`, and `sign_out` manage registrations. Each profile receives a local UUID and distinct stable label. An email match never merges registrations. `respond` accepts a model slug, trusted transformation instructions, untrusted source text, optional context text, cancellation, and a delta callback. It returns completed text only after successful stream validation; the caller decides whether text is safe to insert.

## Authorization and identity

The service persists one UUID URN host identifier. It starts an ephemeral `127.0.0.1` HTTP listener at `/auth/callback`, then generates independent cryptographic state, OIDC nonce, and PKCE verifier. First authorization uses `dynamic_agent_client`; code exchange uses the callback's issued client ID. Subsequent sign-in reuses that registration and host identifier. The exact callback URI is reused during exchange. [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in).

Callback validation rejects incorrect hosts, duplicate parameters, mismatched state, missing registration, changed returning client IDs, oversized headers, and non-GET requests. Browser access is limited to the local listener. JWT verification requires RS256, a published JWKS key, the exact issuer, client audience, subject, expiration, issuance time, and original nonce. Unknown keys refresh the bounded JWKS cache. Returning authorization must preserve the verified subject. [Website identity verification](https://developers.openai.com/siwc/website).

## Credential durability

Every registration has a Windows-user-scoped DPAPI record. Access, rotating refresh, and retained ID tokens never enter renderer storage, logs, URLs exposed to the UI, or public DTOs. Temporary plaintext serialization is zeroized. Atomic replacement uses a flushed temporary file and Windows write-through rename. Host identity and selected local profile UUID are stored separately without tokens.

An OS file lock serializes each account's refresh across processes. A durable pending marker precedes refresh submission. A received replacement is stored as one record before its token can be used. A crash or ambiguous connection failure preserves credentials but prevents reuse of an uncertain rotating refresh token; reauthorization recovers using the issued client ID. Terminal refresh errors clear unusable tokens. Explicit server/network infrastructure failures do not discard unrelated registrations. Sign-out cancels in-process requests, attempts revocation with bounded retries, clears local tokens, and retains the registration and host identity. `remote_revocation_confirmed: false` must be shown as an actionable notice linking to ChatGPT settings. [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions).

The optional `earliest_refresh_at` hint has no fixed JSON type in the current reference. Recognizable epoch values delay renewal; unfamiliar shapes do not reject otherwise valid tokens. Standard expiry remains the renewal contract. Cancellation applies during discovery, callback, token/identity validation, model discovery, and streaming, including preventing a cancelled sign-in from opening the browser. [Token reference](https://developers.openai.com/siwc/token-sharing-open-source/token-reference).

Identity can remain signed in without plan permission. Such a profile is marked `plan_usage_enabled: false`; no inference or model-catalog request proceeds. The UI offers an explicit consent action instead of silently changing billing. [Errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery).

## Inference and failure policy

The account catalog supplies display names and model slugs in server order. No model is hard-coded. The app persists a selected slug per registration and refreshes choices after account switching. A removed model is surfaced for reselection. Successful inference, rather than catalog membership alone, establishes that a request can run. [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

Requests use `POST https://api.openai.com/v1/responses`, `instructions`, text-only `input` arrays, `store: false`, and `stream: true`. Source/context are serialized as JSON data separate from instructions. Unsupported parameters and tools are absent from the request type. Audio remains local. [Preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).

The SSE decoder accepts UTF-8 split across arbitrary chunks, LF/CRLF/CR, multiline data, and comments. It bounds event, stream, output, request, and catalog sizes. A missing Content-Type is accepted, because the plan endpoint streams without one; a stated non-stream type is refused. The plan endpoint also completes with an empty `output`, so the accumulated deltas are cross-checked against `response.output_item.done` message text when the completion carries none. It rejects malformed events, duplicate/reordered sequence numbers, terminal or finished-item text differing from accumulated deltas, refusal, premature EOF, incomplete responses, explicit failure, and idle/overall timeouts. Cancellation drops the HTTP stream. A usage error arriving after text deltas remains a failure. Error DTOs retain HTTP status, safe error code/parameter/request ID, and body shape; remote prose and credentials are redacted. The application must fall back to local text for failed dictation enhancement and must not insert partial cloud text.

## Verification and release checks

`cargo test -p onda-cloud` covers protected storage, callback attacks, JWT signature/claims/nonce, permission-only identities, account mismatch, registration reuse, account models, actual mocked HTTP/SSE inference, sign-out, concurrent rotating refresh, and crash recovery. Tests use a generated test-only RSA key and a loopback fixture. Production code exposes no network endpoint override and refuses redirects; fixture endpoint validation exists only under `cfg(test)`.

Release validation still requires a human to complete real browser consent with an eligible account, verify live account-specific models, perform inference, switch workspace registrations, and disconnect/reconnect through ChatGPT settings. Mocked tests cannot establish live entitlement, serving-region policy, or account limits.
