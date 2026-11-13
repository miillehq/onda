# Architecture

## Decision and boundaries

Tauri 2 renders a small Preact interface in the system webview: WebView2 on Windows, WebKitGTK on Linux, WKWebView on macOS. Rust owns every policy and sensitive operation. Electron would add a bundled browser runtime without improving the native APIs Onda needs, and each OS's input limits are handled in the native crates rather than in renderer callbacks. See [ADR 001](docs/adr/001-runtime-and-isolation.md) and [ADR 006](docs/adr/006-framework.md).

```
ui/ (Preact: main window and dictation pill)
             │ application_call (tagged ApiRequest) / onda://event
         onda-app (Tauri shell, runtime actor, tray, windows)
             ├── onda-core      domain, policy, SQLite
             ├── onda-platform  audio, focused field, clipboard and typing, at-rest protection
             ├── onda-input  →  keyboard helper process (Windows hook, Linux portal)
             ├── onda-asr    →  speech worker processes (CPU, AVX2, Vulkan, Metal)
             └── onda-cloud     ChatGPT sign-in, AI providers, API keys
```

`onda-core` has no Tauri, microphone or network dependency. It contains the explicit state machine, settings/entities, compiled dictionary index, normalization/validation, bounded diff/learning policy, and migrations. SQLite uses WAL, FULL synchronous durability, parameterized statements, a busy timeout and an explicit schema version. A newer schema is refused instead of reset. Corrupt durable data fails visibly; it is never replaced with an empty dictionary.

`onda-platform` owns recording lifetimes and focused-field identity on each OS. Native audio callbacks (CPAL: WASAPI, ALSA, Core Audio) write to a bounded SPSC buffer and avoid inference, file I/O and locks. A worker mixes samples, resamples incrementally and collects clipping/silence diagnostics. Releasing recording stops the stream and returns mono 16 kHz floats; cancelling drops them. Default input is resolved afresh per recording. Persistent endpoint IDs support explicit selection.

`onda-input` runs the shortcut helper in its own process, away from the renderer and inference. On Windows it owns `WH_KEYBOARD_LL` and its message pump; on Linux it holds a global-shortcuts portal session. It emits only configured actions, never arbitrary keys. The parent monitors heartbeat/process death, cancels an operation on reset and bounds restart attempts. Escape is armed only while an operation is active. Configured shortcut updates replace the helper. On Windows the helper reinstalls its hook every 30 seconds while idle, because Windows can silently remove a timed-out hook. macOS has no helper process: the app registers system hotkeys through Tauri's global-shortcut plugin.

`onda-asr` in the shell is a process client, not native inference. PCM crosses anonymous pipes with bounded framing. Exactly one worker is admitted per engine; it starts and loads its model when recording starts and exits after one recording. Cancellation/timeout kills and reaps the child. CPU workers do not import optional GPU runtimes. A separate optional Vulkan binary probes the driver and falls back to CPU after failure, reserving deadline for recovery. The baseline CPU build needs only SSE2; an AVX2 build is preferred on CPUs that support it. On macOS the workers are built with Metal. Workers use at most six inference threads and run at normal priority, while the keyboard helper runs above normal. Fresh workers trade a modest model-load cost for crash isolation and low idle memory.

On Linux the focused field is read through AT-SPI, typing goes through the remote desktop portal once the user allows it, API keys live in the Secret Service, and `/proc` serves memory and process measurements. Without the shortcuts portal, a desktop shortcut runs `onda --dictate`. See [docs/linux.md](docs/linux.md).

On macOS the focused field is read through the Accessibility API and Onda presses ⌘V only after the user turns it on under Privacy & Security › Accessibility; until then dictation is copied for the user to paste. API keys and the key that seals account records and history live in the Keychain. macOS support is experimental ([ADR 007](docs/adr/007-platform-support.md)).

`onda-cloud` keeps OAuth in Rust. It also holds the other providers (`providers.rs`: an OpenAI-compatible adapter, an Anthropic Messages adapter and a loopback-only local adapter) and their key storage (`keys.rs`). Protected per-profile records use DPAPI on Windows, or a key held by the keyring or Keychain on Linux and macOS, with atomic replacement. A stable opaque host ID and issued registration IDs survive restarts. JWKS/issuer/audience/expiry/nonce checks are mandatory. A durable refresh-in-progress marker and cross-process locks handle rotating token races. Network endpoints are fixed in production; loopback mock injection is test-only. The ChatGPT client only accepts text, uses `store:false`, and requires a successful complete stream before output is eligible for insertion.

## Concurrency and sensitive lifetimes

The application's bounded action actor serializes recording start/stop, selection transforms and retry/undo admission. Processing proceeds in cancellable tasks so the actor can accept cancellation. The state machine's UUID generation prevents a stale task from completing a newer job. Native accessibility/audio/paste work runs on blocking worker threads; HTTP/IPC uses async I/O. Mutexes protect short state/database/index accesses. The immutable dictionary index is rebuilt after a committed edit. Model downloads are independently cancellable and protected by per-asset filesystem locks.

Recording start captures the target: on Windows the window handle, PID, process creation time, executable and UIA focused runtime ID; on Linux and macOS what AT-SPI or the Accessibility API exposes. The library checks password status (and on Windows the process integrity level) before reading anything. Immediately before insertion it revalidates target identity, and a transform also compares the current selection to its original source. It never brings another app forward automatically. If focus changes, the result remains available in memory and in History, and a structured error is shown.

Cloud output is provisional until the completed event passes all gates. Ordinary dictation falls back to deterministic local text. Selection transforms retain the original on failure. Text never enters metrics, SQLite, log messages or HTTP diagnostics; History is the one exception and is encrypted ([ADR 005](docs/adr/005-dictation-history.md)). Context is off by default and per-app exclusions are checked before collection. A post-insertion document read occurs only for transform undo or enabled learning. Learning compares only a uniquely identified inserted scope and stops on focus change, unrelated edits, cancellation, a new job or its twelve-second limit.

## Asset and release boundary

The manifest pins model revisions, size and SHA-256. Only trusted HTTPS hosts/redirects are accepted. Downloading, verification and atomic installation are distinct progress states. Runtime verifies model bytes before inference. Location migration verifies the source and destination, leaves old assets intact, and switches saved preferences only after success. Application upgrades preserve the user data/model directory; uninstall does not destroy user assets by default.

Each package bundles the shell and the sidecars named in `src-tauri/tauri.conf.json`: the shortcut helper and the baseline and AVX2 speech workers (ARM packages carry the baseline worker under both names). Optional GPU builds add the separate Vulkan worker. Windows ships a per-user NSIS installer, Linux a deb and an AppImage, macOS a dmg; all are unsigned ([docs/release.md](docs/release.md)). Signed updates are disabled until a distributor supplies an HTTPS endpoint and public verification key at build time. Signing secrets remain in release environment/CI secrets.

## Boundaries

| Module | Owns | May call |
|---|---|---|
| `ui/` (Preact, Vite) | screens, the store, the pill | `ui/src/api.ts` only |
| `ui/src/api.ts` | the typed adapter: `application_call` and `onda://event` in the shell, the labelled mock in a browser | Tauri `invoke`/`listen`, or `ui/src/mock/backend.ts` |
| `src-tauri/` (`onda-app`) | the contract (`ApiRequest`/`ApiResponse` in `src/api.rs`), dispatch, the runtime actor, tray and windows | every crate |
| `crates/onda-core` | domain: state machine, settings, dictionary, text validation, stats, SQLite and migrations | nothing native |
| `crates/onda-platform` | audio capture, target identity, insertion, at-rest protection, desktop integration, process reports | the OS |
| `crates/onda-input` | the shortcut helper process (Windows hook, Linux portal) and its supervisor | `onda-platform` |
| `crates/onda-asr` | model catalog, verified downloads, hardware routing, worker processes | the OS |
| `crates/onda-cloud` | ChatGPT OAuth, AI providers, API key storage | `onda-platform` |

The UI never talks to a crate directly, and crates never import Tauri. `onda-core` stays free of
microphone, network and window code so it runs in any test.

## The seam

One Tauri command, `application_call`, takes a tagged `ApiRequest` and returns an `ApiResponse`;
runtime events arrive on `onda://event`. ts-rs generates the TypeScript mirror into
`ui/src/generated/` (`npm run contracts`), and `npm run contracts:check` fails CI when it drifts.
The mock (`ui/src/mock/backend.ts`) is typed from the same generated types and loads only in
development builds outside the shell. It starts a fresh install from `DEFAULT_SETTINGS` (or
`MACOS_DEFAULT_SETTINGS` with `?platform=macos`) and checks input with `LIMITS`, all generated
from Rust beside the types, and mirrors `Settings::validate` in `ui/src/mock/validate.ts`.
`contracts/examples.ts` holds shared requests and expected answers: Vitest runs them against the
mock (`tests/unit/contract.test.ts`) and the real-shell suite runs them in the real app
(`tests/e2e/00-contract.spec.ts`), so neither side can drift alone. The mock shows a "Development
mock" badge; browser tests assert it and the real-shell suite asserts it is absent.

## State and data

Rust owns all state; the UI rebuilds from `getSnapshot` and events. Data lives in the per-user app
data directory `com.miille.onda` (`%LOCALAPPDATA%` on Windows, `~/.local/share` on Linux,
`~/Library/Application Support` on macOS): SQLite (`onda.db`, explicit `user_version`,
forward-only migrations, newer schemas refused), dictation history encrypted at rest (DPAPI on
Windows, a keyring- or Keychain-held key on Linux and macOS), account records and API keys in
protected files or the OS keyring, and models in a verified, locked directory. Migrations are
tested at L3. A profile left by the provisional `org.eagle.dictation` identifier is moved on
first launch.

## Platform integration

Each OS concern sits behind one module API, with `windows_*`, `linux_*` and `macos_*`
implementations selected by `cfg`, so the domain and the UI are testable without them:
shortcuts (`onda-input`; macOS hotkeys in `src-tauri/src/runtime.rs`), focused-field identity and
insertion (`target.rs`, `clipboard.rs`), at-rest protection (`protect.rs`,
`onda-cloud/src/keys.rs`), autostart and theme (`desktop.rs`), and process reports
(`processes.rs`). See [ADR 007](docs/adr/007-platform-support.md).

## Ownership

One maintainer with agents and outside contributors through pull requests. Contract changes
(`src-tauri/src/api.rs`, `crates/onda-core/src/contracts.rs`, `ui/src/generated/`, `contracts/`,
`ui/src/mock/`) land in their own small PR with regenerated types, the mock updated and the
contract suite green.

## Deliberately simple

- A fresh ASR worker per transcription: crash isolation and low idle memory over model-load time
  (signal to revisit: warm load above 1 s on the Fast profile at L3 benchmarks).
- One IPC command with a tagged union instead of many commands (signal: a request that needs
  streaming or binary payloads).
- No sync and no server; everything stays in the local app data directory.
