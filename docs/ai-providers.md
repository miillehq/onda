# AI providers

AI polish, rewrites and spoken edits can use one of four providers, chosen in Settings › AI polish.
Sign in with ChatGPT is described in [openai-sign-in.md](openai-sign-in.md) and
[optional-chatgpt.md](optional-chatgpt.md).

| Provider | Endpoint | Credential | Text leaves the computer |
| --- | --- | --- | --- |
| ChatGPT | Fixed OpenAI endpoints | OAuth tokens in protected storage | Yes, to OpenAI |
| OpenAI-compatible | User's base URL; HTTPS, or HTTP only to loopback | Optional bearer key | Yes, to that host |
| Anthropic | Fixed `https://api.anthropic.com/v1/` | Required API key (`x-api-key`) | Yes, to Anthropic |
| On this computer | User's base URL, loopback only | Optional bearer key | No |

## Rules every provider follows

- **One provider, no fallback.** Only the chosen provider is called. If it fails, dictation types
  the local transcript and a rewrite keeps the original text; Onda never retries with another
  provider or a billable service. The one retry anywhere is the local server's: a server that
  rejects the thinking switches gets the same request once without them, on the same computer.
- **What is sent.** The instructions, the transcript (or selection) and, if the user enabled it,
  nearby text, as JSON data in the user message. Never audio, keys in bodies, or file paths.
- **Bounded responses.** Requests up to 128 KiB of text, streams up to 8 MiB, answers up to
  512 KiB, an idle timeout (30 s remote, 120 s local, which may be loading a model) and a total
  timeout (90 s remote, 180 s local). A stream counts only after its explicit end: `finish_reason:
  "stop"` (Chat Completions) or `message_stop` after `end_turn` (Anthropic). Length limits,
  refusals and content filters keep the original text.
- **No redirects.** A redirect would forward the key or text to a host the user did not name, so
  every 3xx is an error.
- **Local means local.** The local provider accepts only 127.0.0.0/8, `::1` and `localhost`
  (resolved by Onda to 127.0.0.1, so a hosts file cannot point it elsewhere) and its client ignores
  system proxy settings, so loopback traffic cannot be routed off the computer.
- **Redacted errors.** Logs and messages keep the HTTP status, error code and request id; response
  bodies and provider messages are discarded.

## Credentials

Keys are entered once, validated (printable ASCII, no spaces, at most 512 bytes) and stored by
`onda-cloud::keys`:

- Windows: a DPAPI-protected file per provider in the app data folder (`provider-keys/*.bin`),
  bound to the Windows user, with per-provider entropy so files cannot be swapped between slots.
- Linux: the desktop Secret Service (GNOME Keyring, KWallet's Secret Service, KeePassXC).
- macOS: the Keychain.

On Linux and macOS only an empty marker file is written to disk, so checking whether a key exists
never prompts to unlock the keyring.

Keys never enter settings, the database, logs, prompts or the webview. The webview sends a key once
and then only learns whether one is saved.

## Adapters

- **OpenAI-compatible** (`/chat/completions`, `/models`): `messages` with a system and a user
  message, `stream: true`. No sampling parameters are sent to remote servers (some reasoning models
  reject them). `reasoning_content`/`reasoning` deltas are ignored and a leading `<think>…</think>`
  block is removed before any text is shown.
- **Local**: the same API plus `max_tokens: 4096`, `temperature: 0.2`, `reasoning_effort: "none"`
  and `chat_template_kwargs: {"enable_thinking": false}`, because llama.cpp and Ollama turn
  thinking on for Qwen3.5 unless asked not to. See [local-models.md](local-models.md).
- **Anthropic** (`/v1/messages`, `/v1/models`): its own adapter, not the OpenAI shape.
  `anthropic-version: 2026-10-21`, `max_tokens: 16000` (thinking counts against it on models that
  always think), no sampling parameters (that API version rejects them). Text comes only from
  `text_delta` events; thinking blocks are never collected. Models whose capabilities report
  `effort.low` are asked for low effort. Server-side refusal fallbacks are deliberately not enabled:
  they would retry on another, billable model.

## Verification status

- Covered by deterministic mock-server tests in `crates/onda-cloud/src/providers/tests.rs`: endpoint
  policy, headers, request bodies, key handling, streaming, think-block removal, truncation,
  refusals, redirects, unreachable servers, error redaction and model lists in OpenAI, Ollama and
  Anthropic shapes.
- An ignored live test polishes text through a real llama.cpp server ([local-models.md](local-models.md)).
- Real cloud services (an OpenAI-compatible service, an Anthropic key, ChatGPT) are in the manual
  matrix ([testing.md](testing.md)); [VERIFICATION.md](../VERIFICATION.md) records no real
  provider call yet.
