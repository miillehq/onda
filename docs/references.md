# References

The sources behind Onda's engineering and licensing decisions, and what each one decided. They
were read in October 2026. No source code, strings, branding or artwork from the projects below
is in Onda; reading a project is not permission to reuse its code. Licenses of what Onda ships
are in [third-party-licenses.md](third-party-licenses.md).

## Other dictation apps (design research only)

| Project | Read | What it decided for Onda |
| --- | --- | --- |
| [WhimprFlow](https://github.com/Blueturboguy07/WhimprFlow/tree/04d477a75278fd4f8e6e25f55a34d7733d9d393b) (MIT) | `src-tauri/src/win.rs`, its auto-learning, dictionary and research notes | Vocabulary and replacements as separate concepts. Its Windows paste keeps only plain text, does not check how many inputs Windows accepted and restores the clipboard without a sequence check, so Onda preserves every clipboard format and checks the sequence under the clipboard lock. [ADR 001](adr/001-runtime-and-isolation.md) rejects its large local cleanup model. |
| [Handy](https://github.com/cjpais/Handy/tree/5ec58f696354fcf64ae831102e673779e0249717) (MIT) | `paste_tx/windows.rs`, `clipboard.rs`, the recorder | Delayed clipboard rendering gives a read receipt; a lock-free ring keeps the audio callback free of allocation, logging and locks. Onda's versions are written independently and refuse a paste when any clipboard format cannot be kept. |
| [VoiceInk](https://github.com/Beingpax/VoiceInk/tree/c09cc1f677f40f2ee665843a61f07670210f012e) (GPL-3.0) | the license, dictionary replacement variants, prompt structure | Design only, no code and no GPL dependency. Replacements never cascade, and exact spelling stays separate from explicit correction. |

## Windows APIs

- [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc):
  Windows silently removes a hook that exceeds its timeout (1,000 ms since Windows 10 1709). Hence
  the hook in its own process with its own message loop, a bounded queue, and reinstallation every
  30 seconds while idle, since a heartbeat does not prove the hook is still installed.
- [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput):
  input is refused into higher-integrity windows without an identifiable error, so Onda checks the
  target's integrity first and checks the count of accepted events.
- [Clipboard formats](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats),
  [GetClipboardSequenceNumber](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber),
  [OleDuplicateData](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-oleduplicatedata)
  and [Using the clipboard](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard):
  every format is duplicated or the paste is refused; old metafile pictures are refused; the
  history and cloud-sync exclusion markers; delayed rendering through `WM_RENDERFORMAT`.
- UI Automation [connection](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-put_connectiontimeout)
  and [transaction](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-put_transactiontimeout)
  timeouts (750 ms per provider call) and [TextRange::GetText](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-gettext),
  whose length limit makes it safer than an unbounded `ValuePattern` read.
- [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects): a
  kill-on-close job ends speech workers even when Onda is killed.
- [GetAsyncKeyState](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate):
  it can report a key the hook swallowed as released, so the hook's own key-up events decide
  when a held shortcut ends; `GetAsyncKeyState` only seeds modifier state when the hook starts.

## Audio

- [CPAL 0.18.2](https://docs.rs/cpal/0.18.2/cpal/traits/trait.DeviceTrait.html): its `DeviceId`
  is the WASAPI endpoint ID, so two microphones with the same name stay distinct; 0.16 exposed
  names only. Its [error kinds](https://docs.rs/cpal/0.18.2/cpal/enum.ErrorKind.html) separate a
  capture gap (`Xrun`, recording continues) from device loss.
- [rubato 0.16](https://docs.rs/rubato/0.16.2/rubato/struct.FftFixedInOut.html) for resampling and
  [rtrb](https://docs.rs/rtrb/) for the real-time ring.

## Speech recognition

| Source | What it decided |
| --- | --- |
| [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (MIT) through [whisper-rs](https://codeberg.org/tazz4843/whisper-rs) 0.16.0 (Unlicense) | CPU inference on quantized GGML models, with Vulkan and Metal as optional accelerators ([ADR 002](adr/002-asr-profiles.md)). Native CPU tuning is off so one build runs on any x86-64 CPU. |
| [Whisper license](https://github.com/openai/whisper/blob/main/LICENSE) and the [pinned model repository](https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1) | The weights are MIT. The pinned revision, byte sizes and SHA-256 in `crates/onda-asr/src/manifest.rs` are the trust root for downloads. |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime/blob/main/LICENSE) (MIT), [DirectML](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html), [transcribe-rs](https://github.com/cjpais/transcribe-rs) and NVIDIA Parakeet [v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) and [v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) (CC BY 4.0) | Researched alternatives, not used: another runtime and model family to validate and ship. Adding one needs its own model and license review. |
| [whisper.cpp acceleration notes](https://github.com/ggml-org/whisper.cpp/blob/master/README.md#vulkan-gpu-support) | Vulkan covers NVIDIA, AMD and Intel where the driver supports it; CUDA would be NVIDIA-only. Onda probes the real worker instead of trusting the vendor name, and keeps the CPU path. |
| [JFK inaugural address](https://www.jfklibrary.org/archives/other-resources/john-f-kennedy-speeches/inaugural-address-19610120) and [whisper.cpp's sample](https://github.com/ggml-org/whisper.cpp/blob/b0a11594aec50892a02cd8d129eee2dfe93a8bb8/samples/jfk.wav) | The public-domain test clip; provenance in `tests/fixtures/asr/README.md`. |
| [Visual C++ redistribution](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170) and [deployment options](https://learn.microsoft.com/en-us/cpp/windows/deployment-in-visual-cpp?view=msvc-170) | The Windows workers need the VC++ runtime; app-local DLLs keep the install per-user, and Onda must ship serviced copies with its releases. |

## Sign in with ChatGPT

Read on 2026-10-03 from OpenAI's official documentation. Reread the relevant page before
changing OAuth scope, endpoints, revocation or inference; if an endpoint changes, update the
allowlist and the mocked tests together. Production endpoints are never user-configurable.

| Source | Used for |
| --- | --- |
| [Open-source plan usage](https://developers.openai.com/siwc/token-sharing-open-source) | public client registration, stable host identity |
| [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in) | dynamic registration, loopback redirect, PKCE, issued client IDs |
| [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions) | separate registrations, serialized rotating refresh, revocation |
| [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference) | the per-account model list, a stream that must complete |
| [Preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations) | required request fields, unsupported inputs and tools |
| [Token reference](https://developers.openai.com/siwc/token-sharing-open-source/token-reference) | token fields, expiry, replacement refresh tokens |
| [Errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery) | plan permission, limits, terminal refresh errors |
| [Website OIDC verification](https://developers.openai.com/siwc/website) and [discovery](https://auth.openai.com/.well-known/openid-configuration) | JWKS signature, issuer, audience, time and nonce checks; the exact endpoints |

Onda uses the website flow's token-validation guidance but not its confidential client, and no
private ChatGPT endpoint or embedded partner credential.

## Tauri

[Windows installer](https://v2.tauri.app/distribute/windows-installer/),
[prerequisites](https://v2.tauri.app/start/prerequisites/),
[updater](https://v2.tauri.app/plugin/updater/) and
[capabilities](https://v2.tauri.app/security/capabilities/): the per-user NSIS installer, the
WebView2 bootstrapper, the narrow IPC permission and signed updates.
