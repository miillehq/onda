# Architecture

## Decision and boundaries

Tauri v2 renders a small TypeScript view using the installed WebView2 runtime. Rust owns every policy and sensitive operation. Electron would add a bundled JavaScript runtime without improving the required native APIs; a fully native UI would delay the separate frontend team's work. Tauri's Windows input limitations are handled in the native crates rather than in renderer callbacks. See [ADR 001](docs/adr/001-runtime-and-isolation.md).

```
Diagnostics frontend / future polished frontend
             │ generated tagged IPC / events
         onda-app (Tauri)
             │
         onda-core (policy + SQLite)
             ├── onda-platform (WASAPI / UIA / clipboard / SendInput)
             ├── onda-input → isolated keyboard helper
             ├── onda-asr → isolated CPU or Vulkan worker
             └── onda-cloud → official OAuth / text-only Responses
```

`onda-core` has no Tauri, microphone or network dependency. It contains the explicit state machine, settings/entities, compiled dictionary index, normalization/validation, bounded diff/learning policy, and migrations. SQLite uses WAL, FULL synchronous durability, parameterized statements, a busy timeout and an explicit schema version. A newer schema is refused instead of reset. Corrupt durable data fails visibly; it is never replaced with an empty dictionary.

`onda-platform` owns recording lifetimes and Windows target identity. Native audio callbacks write to a bounded SPSC buffer and avoid inference, file I/O and locks. A worker mixes samples, resamples incrementally and collects clipping/silence diagnostics. Releasing recording stops the stream and returns mono 16 kHz floats; cancelling drops them. Default input is resolved afresh per recording. Persistent endpoint IDs support explicit selection.

`onda-input` isolates `WH_KEYBOARD_LL` and its message pump from both the renderer and inference. It emits only configured actions, never arbitrary keys. The parent monitors heartbeat/process death, cancels an operation on reset and bounds restart attempts. Escape is armed only while an operation is active. Configured shortcut updates replace the helper. The helper checks stale held-key state and periodically reinstalls its hook because Windows can silently remove a timed-out hook.

`onda-asr` in the shell is a process client, not native inference. PCM crosses anonymous pipes with bounded framing. Exactly one worker is admitted per engine. Cancellation/timeout kills and reaps the child. CPU workers do not import optional GPU runtimes. A separate optional Vulkan binary probes the driver and falls back to CPU after failure, reserving deadline for recovery. The generic CPU build does not require AVX or AVX2. Six inference threads maximum and BELOW_NORMAL process priority reserve responsiveness. Fresh workers intentionally trade a modest model-load cost for crash isolation and low idle memory.

Linux uses the same crates with smaller platform implementations: `cpal` capture on ALSA, an
unidentified target and a clipboard handoff instead of typing, the Secret Service for API keys,
`/proc` for memory and process measurements, and no keyboard helper (desktop shortcuts run
`onda --dictate`). See [docs/linux.md](docs/linux.md).

`onda-cloud` keeps OAuth in Rust. It also holds the other providers (`providers.rs`: an
OpenAI-compatible adapter, an Anthropic Messages adapter and a loopback-only local adapter) and
their key storage (`keys.rs`). Protected per-profile records use Windows DPAPI and atomic replacement. A stable opaque host ID and issued registration IDs survive restarts. JWKS/issuer/audience/expiry/nonce checks are mandatory. A durable refresh-in-progress marker and cross-process locks handle rotating token races. Network endpoints are fixed in production; loopback mock injection is test-only. The client only accepts text, uses `store:false`, and requires a successful complete stream before output is eligible for insertion.

## Concurrency and sensitive lifetimes

The application's bounded action actor serializes recording start/stop, selection transforms and retry/undo admission. Processing proceeds in cancellable tasks so the actor can accept cancellation. The state machine's UUID generation prevents a stale task from completing a newer job. Native UIA/audio/paste work runs on blocking worker threads; HTTP/IPC uses async I/O. Mutexes protect short state/database/index accesses. The immutable dictionary index is rebuilt after a committed edit. Model downloads are independently cancellable and protected by per-asset filesystem locks.

Recording start captures window handle, PID, process creation time, executable and UIA focused runtime ID. The library checks integrity and password status before reading anything. Immediately before insertion it revalidates target identity, and a transform also compares the current selection to its original source. It never brings another app forward automatically. If focus changes, the result remains available in memory and in History, and a structured error is shown.

Cloud output is provisional until the completed event passes all gates. Ordinary dictation falls back to deterministic local text. Selection transforms retain the original on failure. Text never enters metrics, SQLite, log messages or HTTP diagnostics. Context is off by default and per-app exclusions are checked before collection. A post-insertion document read occurs only for transform undo or enabled learning. Learning compares only a uniquely identified inserted scope and stops on focus change, unrelated edits, cancellation, a new job or its twelve-second limit.

## Asset and release boundary

The manifest pins model revisions, size and SHA-256. Only trusted HTTPS hosts/redirects are accepted. Downloading, verification and atomic installation are distinct progress states. Runtime verifies model bytes before inference. Location migration verifies the source and destination, leaves old assets intact, and switches saved preferences only after success. Application upgrades preserve the user data/model directory; uninstall does not destroy user assets by default.

The per-user NSIS bundle contains the shell, CPU worker and shortcut helper; optional GPU bundles add the separate Vulkan worker. Signed updates are disabled until a distributor supplies an HTTPS endpoint and public verification key at build time. Signing secrets remain in release environment/CI secrets. ARM64 is structurally feasible but not a V1 release target or certified architecture.

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
development builds outside the shell. Planned in [PLAN.md](PLAN.md): shared request and response
examples in `contracts/`, a contract suite (L2) that runs them against the mock and the real
dispatcher, a real-shell suite (L6) that asserts at least one call against the same examples, and
a visible mock badge so no browser test can pass against the mock by mistake.

## State and data

Rust owns all state; the UI rebuilds from `getSnapshot` and events. Data lives in the per-user app
directory (`com.miille.onda`): SQLite (`onda.db`, explicit `user_version`, forward-only
migrations, newer schemas refused), dictation history encrypted at rest (DPAPI on Windows, a
keyring-sealed key on Linux, the Keychain on macOS once ported), account records and API keys in protected
files or the OS keyring, and models in a verified, locked directory. Migrations are tested at L3.

## Platform integration

Each OS concern sits behind one module API, with `windows_*`, `linux_*` and (planned) `macos_*`
implementations selected by `cfg`, so the domain and the UI are testable without them:
shortcuts (`onda-input`), focused-field identity and insertion (`target.rs`, `clipboard.rs`),
at-rest protection (`protect.rs`, `onda-cloud/src/keys.rs`), autostart and theme (`desktop.rs`),
and process reports (`processes.rs`). macOS adds an event-tap shortcut helper, Accessibility-based
target checks with Cmd+V paste, Keychain storage, launch at login and a Metal speech worker; ARM
builds drop the x86-only AVX2 worker. See [ADR 007](docs/adr/007-platform-support.md).

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
- No sync and no server; the encrypted SQLite file is the only store.
