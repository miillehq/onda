# Architecture

## Decision and boundaries

Tauri v2 renders a small TypeScript view using the installed WebView2 runtime. Rust owns every policy and sensitive operation. Electron would add a bundled JavaScript runtime without improving the required native APIs; a fully native UI would delay the separate frontend team's work. Tauri's Windows input limitations are handled in the native crates rather than in renderer callbacks. See ADR 001.

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
`onda --dictate`). See linux.md.

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
