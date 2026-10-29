# Onda

Onda by miille: Windows voice dictation that types wherever your cursor is. Speech recognition runs locally. Optional text cleanup and selection rewrites use the AI provider you choose: a connected ChatGPT plan, an OpenAI-compatible server, Anthropic, or a model on this computer. A Linux build with fewer features is also available ([linux](docs/linux.md)). Apache-2.0.

The repository contains the native application, the Onda interface (main window and a non-activating dictation pill), CPU and optional Vulkan ASR workers, a Windows shortcut helper, installer scripts, CI, and failure tests. Design decisions live in [DESIGN.md](DESIGN.md). This is an engineering candidate: consult [testing](docs/testing.md) for the measured results and the interactive release matrix that has not yet been certified.

## Run and build

Windows 11 x64 is the primary platform. Windows 10 22H2 x64 (build 19045) is the minimum project support target; that baseline needs verification on a clean machine. The ordinary build needs neither a GPU, API key nor administrator rights. A model download and Microsoft WebView2 are required. Default profile is Fast, with automatic language detection. Choose English explicitly to use the smaller English-only path.

Install Rust 1.95.0, Node 24, Visual Studio 2022 C++ Build Tools, CMake and LLVM. Set `LIBCLANG_PATH` to the directory containing `libclang.dll`; these are developer dependencies, not runtime requirements. Then:

```powershell
npm ci
npm run desktop
```

The script builds the helper executables before starting Tauri. First-run setup checks the microphone, downloads a speech model (or keeps one you already have) and lets you try dictation in a practice field. Then focus an editable field in any app, hold **Ctrl + Win + Space**, speak, and release. Closing the main window keeps Onda in the tray; use the tray's Quit action to exit. The first launch moves data from the provisional `org.eagle.dictation` folder to `com.miille.onda`.

For interface work without the native shell, `npm run dev` serves the UI against a clearly labelled development mock (`ui/src/mock/backend.ts`, never bundled); add `?mock=empty|onboarding|error`, `?theme=dark`, a route such as `#settings/speech`, or open `/pill.html?pill=listening|handsfree|writing|polishing|done|error`. In debug builds, `ONDA_DEV_PROFILE=<absolute folder>` runs an isolated profile beside an installed copy (no global shortcuts, registry changes or single-instance lock).

```powershell
npm run release                       # unsigned per-user NSIS installer
./scripts/build.ps1 -WithVulkan        # optional separate GPU worker; needs Vulkan SDK
```

See [release instructions](docs/release.md) for signed updates, Authenticode and complete installation prerequisites. Large models are downloaded to the user's local data directory and never committed or bundled into Git.

## Capabilities

- Push-to-talk, optional toggle recording, cancellation, microphone selection/level test, bounded capture and band-limited resampling.
- CPU Whisper ASR on baseline x64 hardware, an optional Vulkan worker for NVIDIA/AMD/Intel drivers, process isolation and CPU fallback.
- Checked resumable model downloads, model profiles, explicit selection, uninstall, and verified location migration.
- Separate vocabulary and replacement rules, Unicode boundaries, indexed relevant-term retrieval, snippets, import/export and immediate updates.
- Optional scoped correction proposals: three focused-field samples over twelve seconds, only for text this app inserted, and only with learning enabled.
- Official browser OAuth/PKCE, DPAPI credentials, account/workspace registrations, serialized refresh, account model discovery, bounded streaming, and local dictation fallback.
- OpenAI-compatible, Anthropic and loopback-only local model providers with protected API keys, no cross-provider fallback, and per-machine model guidance.
- Local profile and statistics (words, pace, streaks, a year of activity) from content-free counts, with an Onda-branded PNG to share.
- A report of Onda's whole process tree, including the WebView2 processes Task Manager lists separately.
- Selected-text transforms, custom instructions/shortcuts, spoken editing commands, original/result/diff/copy, retry insertion and guarded undo.
- Target/window/process/control validation, password/UIPI refusal, delayed-render clipboard transactions and sequence-guarded restoration.
- Migrations, content-free logs and performance metrics, redacted diagnostics, restrictive IPC capabilities and signed-update hooks.

No audio uploads, analytics, screenshot capture, continuous app scraping or hidden billable fallback are implemented. Dictations are kept in History for 3 days by default (0–30 days, or only until Onda quits), encrypted with the user's Windows account; see `docs/decisions/005-dictation-history.md`.

## Validate

```powershell
./scripts/prepare.ps1
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run contracts:check
npm run build
npm run quality
./scripts/asr-benchmark.ps1
```

Real desktop clipboard tests are explicitly opt-in because they take foreground focus; see [testing](docs/testing.md). Real ChatGPT authorization needs an eligible account's consent, and is not claimed from mocked tests.

## Continue development

Read [architecture](docs/architecture.md), [frontend handoff](docs/frontend-handoff.md), [product behavior](docs/product-behavior.md), [security](docs/security.md), [OpenAI sign-in](docs/openai-sign-in.md), [AI providers](docs/ai-providers.md), [local models](docs/local-models.md) and [Linux](docs/linux.md). [Benchmarks](docs/benchmarks.md) describe measured hardware and cold/warm conditions. [References](docs/references.md) records pinned source inspections and engineering decisions.
