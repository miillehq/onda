# Testing and release verification

Use Windows x64 with the pinned Rust 1.95 toolchain, Node 24, MSVC build tools, CMake and LLVM/libclang. Run `npm ci` and `scripts/prepare.ps1` first: Tauri embeds real worker sidecars at build time. `scripts/toolchain.ps1` detects the installed CMake/libclang paths and fails explicitly if absent. Do not substitute fake executable placeholders to make the desktop host compile.

## Reproducible automated checks

```powershell
npm ci
./scripts/prepare.ps1
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p onda-asr --features native-asr --all-targets -- -D warnings
cargo test --locked --workspace
npm run contracts:check
npm run build
npm run quality
cargo deny --locked check licenses advisories sources
npm audit
```

The core tests cover state transitions, stale operation IDs, dictionary boundaries/longest-match/Unicode, snippets, conservative correction proposals, text validation and SQLite migrations. The quality harness asserts fixture outputs/validation decisions and times a 10,000-entry retrieval index; see `benchmarks.md` for measured results. Generated Rust/TypeScript contracts are checked from the backend schema, and drift fails CI.

The cloud tests use local mock HTTP servers and test signing keys to exercise OAuth state/nonce/PKCE/identity checks, refresh-token rotation and crash recovery, Windows DPAPI storage, account/model discovery, HTTP errors, split/oversized SSE, malformed/partial streams, connection failure, cancellation and timeout. They do not authorize a real account or assert live plan eligibility. Never use real tokens as fixture data.

ASR/model tests cover trust-manifest selection, hardware routing, URL/content-range validation, checksum rejection, truncated/corrupted downloads, disk/path/lock/cancellation behavior and worker protocol. Ordinary tests do not silently download large files. CI explicitly downloads the 77.7 MB pinned English tiny model using the same verified manager as the application, then runs the ignored golden tests:

```powershell
$installationJson = (& cargo run --locked -p onda-asr --bin onda-asr-benchmark -- download '.artifacts/ci-models' whisper-tiny-en) -join "`n"
$installation = $installationJson | ConvertFrom-Json
$env:ONDA_ASR_TEST_MODEL = $installation.path
$env:ONDA_ASR_WORKER_DIR = Join-Path $PWD 'target/debug'
cargo test --locked -p onda-asr --test golden -- --ignored --test-threads=1
```

The public-domain JFK WAV test checks both recognized speech clauses. The GPU-probe test deliberately supplies a CPU worker as an unavailable accelerator and requires a real CPU transcript plus fallback metadata. The cancellation test cancels a real worker operation and requires `Cancelled`. These are actual transcription/process tests, not stub success tests. Download/cache contents are reverified against the manifest; downloaded model binaries are never committed or embedded in installers.

## Native desktop tests

`onda-platform` verifies batch/incremental resampling duration/frequency/alias attenuation, non-finite rejection, clipping/silence, identity changes/PID reuse/password/unknown identity, unsupported clipboard formats, sequence guards and chord release ordering. `onda-input` verifies repeated-key behavior, injected filtering, modifier release ordering, custom transform bindings, protocol bounds and idle/active Escape cancellation while PTT remains held.

Simulated capture-worker/channel tests verify immediate disconnect/crash reporting, repeated completion polling followed by Stop retaining the original result, Stop finalizing a still-running worker, and a time-limit callback retaining exactly the valid bounded audio prefix with `limitReached`. These tests need no physical microphone and do not substitute for real USB/default-device/hot-plug tests.

The opt-in real desktop harness opens only controlled Win32 Edit windows and preserves the previous clipboard before using fixture content. It exercises Unicode/emoji/Japanese insertion, delayed-render receipt, registered rich-format restoration, focused-field reading, selected-span replacement, unchanged-document undo, stale-document refusal, clipboard changes during paste and password rejection. Do not run concurrently with user clipboard/focus activity:

```powershell
$env:ONDA_DESKTOP_TESTS = '1'
cargo test -p onda-platform real_edit_unicode -- --ignored --test-threads=1
```

This harness passed on the implementation host. It is ignored in hosted CI because foreground activation and desktop access are environmental prerequisites. A separate real helper-process probe passed Ready, 500 ms heartbeat and clean shutdown. Actual keyboard activation, session transitions and CPU-starvation behavior still require the matrix below.

Debug builds also expose `onda-platform::debug_fixture::OwnedEditWindow` for an application-level known-WAV replay through the same Runtime transcription/normalization/insertion pipeline. It creates/focuses one owned empty edit control, reads only that control, and restores a still-valid previous window only if focus remains on the fixture. It never reads previous user fields. The entire helper is guarded by `cfg(all(windows, debug_assertions))` and is absent from release code/resources; it is not a general window-automation endpoint.

The opt-in full Runtime replay attempt was refused on the implementation host before processing began: ordinary foreground activation and the owned UIA SetFocus request could not establish an accepted accessible target. The harness keeps a bounded explicit refusal rather than bypassing Windows foreground/security policy. This check needs a foreground-permitted interactive session and is **not** counted as a successful end-to-end dictation proof. Separately, all thirteen ordinary real WebView2/native IPC checks passed in both healthy and deliberately unavailable-cloud-storage debug profiles; controlled Win32 insertion and real CPU/GPU recognition have their own positive evidence.

## Packaging CI

`.github/workflows/ci.yml` runs on Windows Server 2022 with immutable action pins. It prepares the CPU workers, checks formatting/clippy/contracts/frontend/quality, runs ordinary and explicit real-model tests, checks current dependencies, builds unsigned per-user NSIS, performs silent install/uninstall with application/helper layout checks, and uploads installer/SHA-256/notices/reviewed MPL source. `.github/workflows/release.yml` builds tag/manual candidates, with optional separately compiled Vulkan worker and explicit updater-signing configuration. It uploads candidates; it does not publish a release. Workflow definitions have been locally linted; an actual hosted GitHub run is a separate validation record.

The optional Vulkan path maps the repository to a short drive path and uses a separate target directory so native shader/toolchain paths do not hit Windows MAX_PATH. SDK checksum/Authenticode are verified before installation. Hosted runners usually lack usable GPUs; compiling the worker does not establish driver compatibility or accelerated inference correctness.

### Observed installer smoke on the implementation host

`scripts/installer-smoke.ps1 -WithVulkan` passed against the final `Onda_0.1.0_x64-setup.exe` candidate (12,333,269 bytes; SHA-256 `5aa1cff5cf4ba5a00f8a0de2301c38a3739cc94626e2410dec49dfa65777bc1f`). The test refused preexisting installer/process/shortcut/autostart namespaces, installed silently into a fresh owned `.artifacts/installed-smoke/<GUID>` directory, and verified the shell, shortcut helper, CPU worker and optional Vulkan worker. All ten app-local CRT DLL hashes, Microsoft/license resources, corresponding MPL source ZIP and recursive installed import closure passed.

The installed CPU worker loaded/probed successfully with zero Vulkan devices; the installed Vulkan worker loaded/probed successfully with one device. The release shell launched hidden with a PATH containing only Windows/System32, emitted its current startup log, and kept its owned shortcut helper alive. During a 5.015-second idle sample, native shell private/working-set memory was approximately 13.01/38.74 MiB and helper memory 3.59/8.05 MiB; both sampled zero CPU time. These are short native-process measurements, excluding WebView2 child/model totals and not a hardware-spectrum benchmark.

Closing only the test-owned shell caused the helper to exit through inherited-pipe EOF. Silent uninstallation returned zero, removed the owned installation, and preserved every settled application-data file hash compared immediately before/after uninstall. No application data was deleted or seeded by the script, and no unrelated processes were stopped. The local evidence report is `.artifacts/installed-smoke/107c6fecb1a340b08e16a141e9c17063.json`; it records the exact candidate, checks and hash snapshots. Initial test-only startup-log filtering was corrected to use each entry's UTC timestamp because Windows may defer a live file's LastWriteTime.

This proves the packaged candidate on this development host, with its existing WebView2/Windows runtime and GPU driver. It is not certification of a fresh Windows installation, Windows 10 support, update/upgrade migration, physical shortcuts/microphones, every target application, or real account/certificate-backed signing.

## Manual matrix before public release

| Scenario | Required evidence | Current status |
| --- | --- | --- |
| Clean Windows 11 x64 install, launch/tray, uninstall, reinstall and upgrade | Installer logs, all three required executables, preserved settings/models, no developer tooling needed | Local installer/build and CI path available; clean-machine matrix must be recorded |
| Windows 10 actual minimum/WebView2 bootstrap | Install/launch/upgrade on documented minimum with runtime bootstrap behavior | Unverified |
| PTT/toggle/cancel/custom transform, rapid requests and conflicts | Exactly one start/stop per physical chord; idle Escape works; busy/cancel errors visible | Matcher/helper protocol tested; physical shortcut matrix unverified |
| Microphone absent/unplug/default switch, sample formats, silence/clipping | Structured errors, no hangs/leaks or hidden microphone switch, usable level diagnostics | Logic/transport tested; device matrix unverified |
| Notepad, browser, Electron/VS Code, WPF/WinUI and Terminal | Correct original target/control, selected transforms/undo, configured paste chord, rich clipboard unchanged | Controlled Win32 Edit passed; named application matrix unverified |
| Password/elevated fields and focus/selection changes mid-operation | Refusal before unsafe insertion; original/result retained for copy; no silent success | Password/identity/guard logic tested; elevated application matrix unverified |
| RDP, suspend/resume, user session switch, saturated CPU | Release/reset/recovery behavior, bounded helper restart, no stuck recording | Unverified |
| Real ChatGPT sign-in/account switching/quota/offline/revocation | Current official flow, DPAPI record, catalog by account, streamed response and local fallback | Mocked protocol/storage tested; real account requires user-driven verification |
| NVIDIA/AMD/Intel Vulkan and CPU-only older x64 hardware | Real probe/inference/fallback evidence, cancellation and timings per hardware/profile | RTX 5060 Vulkan and CPU/failure fallback verified; AMD/Intel/older-CPU matrix unverified |
| Authenticode and signed updater | Valid publisher/timestamp on app/helpers/installer, signature rejection for tampered payload, approved endpoint/public key | Hooks/configuration exist; real signer/secrets not provided |

Record each release's actual evidence and failures instead of relabeling unverified scenarios as supported. Run a final content-leak review of logs/diagnostic export, cancel during every processing stage, simulate corruption/download loss, and compare raw/enhanced fixture outputs before promotion.

## MY-29 to MY-37 batch (29 October 2026, Linux VPS)

Done on an Ubuntu 26.04 VPS (4 vCPU Haswell, 8 GB, no GPU, no microphone, no Windows). Nothing
below proves Windows, WebView2, NSIS installation, a microphone, a GPU or a real cloud provider.

### Run here, and passed

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (core 51, ASR 16, cloud 23 plus 2 ignored live tests, app 19, platform
  22, input 11; 142 in total), `npm run contracts:check`, `npm run build` (TypeScript and Vite).
- Windows-only Rust type-checked from Linux: `cargo clippy --target x86_64-pc-windows-msvc
  -p onda-platform -p onda-input --all-targets -- -D warnings`. This covers the new process
  measurement code; it was compiled, not run. The app crate and onda-cloud cannot be checked for
  Windows here, because their C dependencies need MSVC.
- Real-model golden tests on Linux (pinned whisper tiny.en, verified download): JFK transcription,
  cancellation, and CPU fallback after a failed GPU probe (a CPU worker stands in for the GPU).
- Native smoke under Xvfb with WebKitGTK: all 41 checks of `ui/src/smoke.ts` pass, 14 of them new. One run while the
  release build saturated the CPU failed at startup (WebKit fell back from its IPC protocol); the
  same run passed on an idle machine. Watch for this under load in CI.
- Secret Service key round trip against gnome-keyring 50.0 on a private D-Bus session (ignored
  test `keys::tests::secret_service_round_trip`).
- Live local polish: llama.cpp b11960 serving Qwen3.5-0.8B Q4_K_M, ignored test
  `providers::tests::live_local_server_polishes_text`, 3 runs (see local-models.md).
- Installer config validated against Tauri CLI 2.12.1's schema; NSIS 3.10 compiled the branded
  MUI2 pages. The real installer was not built.
- Linux release package: `scripts/build.sh --bundles deb` built `Onda_0.1.4_amd64.deb` (11.75 MiB,
  12.5 min on 3 jobs). It contains `onda`, both speech workers, the desktop entry, icons and
  notices. The binary links ALSA and D-Bus, so the package now declares them. The package was
  inspected, not installed.
- `cargo deny check licenses sources` passes for both the Windows and the Linux graph. `check
  advisories` flagged crates already pinned on `master`. rustls, rustls-webpki, tokio and
  webpki-roots are updated to their fixed patch releases. **zip 4.6.1 remains**
  (RUSTSEC-2026-0367/0368/0369): it comes from tauri-plugin-updater's default `zip` feature, and
  its fix needs a newer zip major. Either wait for the plugin, or turn off its default features
  (keeping `rustls-tls` and `system-proxy`) once you confirm updates never use v1-compatible
  archives; Onda builds NSIS updater artifacts. `npm audit` reports 0 vulnerabilities.
- Interface reviewed in Chromium against the development mock (Stats, share preview, General,
  AI polish), plus the Linux variants via `?platform=linux`.

### Your checklist

**Windows (build from this branch with `npm ci; ./scripts/build.ps1`)**

1. `cargo test --workspace`, the strict clippy commands and `npm run quality` pass on Windows. This
   branch changed the platform, cloud and app crates, and none of them has been compiled for
   Windows.
2. `scripts/smoke.ps1`: all 41 checks pass in WebView2, including "resource report includes the web
   view processes".
3. Upgrade an existing 0.1.4 profile. The database migrates to schema 5, Stats shows lifetime
   totals, active days and streaks equal to the old daily rows, and History, the dictionary and accounts are intact.
   Downgrading is refused by design.
4. Settings › General › Tray icon: Automatic, White and Black each change the notification-area
   mark at once. Switch Windows between light and dark: Automatic follows within a second, even with
   the main window closed. The setting survives a restart.
5. Settings › Updates and help › What Onda is using: compare the total with Task Manager's Onda
   entry plus "WebView2 Manager" (Memory column). Expect them within a few MB. Run once as
   administrator: the page should say no WebView2 processes were found under Onda.
6. Profile: choose a large JPEG from a phone (with GPS EXIF). It appears cropped. Export Stats with
   and without name and picture, and check the saved PNGs. Confirm the stored picture holds no EXIF:
   `sqlite3 onda.db "select hex(substr(picture,1,64)) from profile"` should start with a PNG
   signature, and the file should not contain the original's GPS bytes.
7. Dictate in Notepad with each provider set but polish off: no network traffic (Resource Monitor).

**Installer**

1. `./scripts/build.ps1` and `./scripts/build.ps1 -WithVulkan` finish. `installer-smoke.ps1`
   passes.
2. Open the setup .exe at 100%, 150% and 200% scaling. The Welcome and Finish pages show the dark
   icon and white wordmark on graphite, inner pages show the black lockup top-left, and neither is
   cropped or blurred beyond normal MUI2 stretching. Also try a high-contrast theme.
3. Fresh install, then upgrade over 0.1.4, then uninstall from Settings › Apps. The uninstaller
   pages are branded too. Data in `%LOCALAPPDATA%\com.miille.onda` survives uninstall.
4. Signed build (when a certificate exists): installer, uninstaller and helpers all validate.

**GPU:** follow the GPU procedure in local-models.md and the existing Vulkan whisper rows below.

**Real providers (needs your keys; nothing was sent to a provider from here)**

1. Anthropic: save a key, Check connection lists models, polish a dictation with
   `claude-opus-5-5` and with a lighter model. Remove the key: polish then says to add one.
2. OpenAI-compatible: a real HTTPS service (OpenAI itself, OpenRouter or similar), with and without
   a key. Try a model that rejects `max_tokens`/sampling; Onda sends none of those to remote servers.
3. Confirm each failure keeps the original text and no other provider is called: a wrong key
   (401), a wrong model (404), rate limiting (429) and offline.
4. Ollama and LM Studio on Windows with `qwen3.5:0.8b`. Note whether LM Studio rejects the thinking
   switch (Onda should retry once without it).

**Linux desktop (beyond this VPS)**

1. A real session on GNOME Wayland and on KDE or X11. Bind `onda --dictate` and dictate with a real
   microphone. The words land on the clipboard and Ctrl+V pastes them into another app.
2. The tray mark appears, Black and White switch, and launch at login starts Onda in the background.
3. `scripts/build.sh --bundles deb,appimage`, then install the .deb on a clean machine.

### Not done, and why

- Hold-to-talk, typing into apps, selection rewrites and context on Linux need a keyboard hook and
  an accessibility route; see linux.md.
- WebView2 `GetProcessInfos` and a low memory target for the hidden pill need COM code that could
  not be compiled here; see windows-integration.md.
- An Onda-managed embedded LLM runtime; see local-models.md for why a local server came first.

## Final local validation record (3 October 2026)

### Held shortcut and empty recognition follow-up (0.1.3)

The old heartbeat queried `GetAsyncKeyState` for the suppressed trigger, interpreted an up/unavailable result as lost state, cancelled and cleared the matcher latch every 500 ms. Subsequent auto-repeat restarted recording. The patch removes this query; genuine hook key-up is authoritative. Boolean `UOI_IO` desktop metadata replaces it for once-per-desktop-loss cancellation, preserving consumed key latches across return. Idle hook rearm excludes held trigger/cancel keys and active inference. Readiness is emitted on desktop restoration and the UI handles shortcut availability without reverting to a stale operation phase. See [Microsoft's asynchronous key-state contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate).

Empty ASR output now has the distinct `noSpeech` error before cleanup/cloud/insertion. Supported empty native selections retain final focus revalidation and report `selectionRequired`; inaccessible controls remain unsupported. Core source validation distinguishes empty text, the UTF-8 byte limit and null characters. Tests cover hundreds of heartbeat/repeat ticks with one start/release, Escape while held, desktop loss/return, long inference rearm refusal, empty recognition/selection, and byte-boundary/null validation. A real Windows metadata test queries the current desktop without installing a hook or changing focus. Physical hotkeys, uncommon RDP/suspend transitions and rare missed key-up still require interactive qualification; no injected input was used to claim those checks passed.

The installed Small Q5 model and installed CPU worker recognized both expected clauses in the public-domain JFK fixture. This was a correctness check under concurrent development activity: 120,893 ms inference and 348 ms model loading, not a representative latency benchmark. No user microphone audio or transcript was read or persisted in that model check.

Final affected-crate checks pass: application 11, core 28 and input 12 tests; platform ordinary tests separately pass 18 with the unrelated interactive clipboard test ignored. Strict Clippy, formatting, TypeScript/Vite and generated contract checks pass. Twenty actual hidden WebView2/native IPC checks pass for the final patch, including the empty-preview recovery message.

The optional-Vulkan `Onda_0.1.3_x64-setup.exe` built successfully at 12,325,883 bytes with ProductVersion `0.1.3` and SHA-256 `603b3ebe2887e022c94fc2afbf1f9e545830076a59647bc72fba43156bb1e0c1`. The packaged helper sidecar hash matches the corrected release helper. The running user installation was not replaced during these checks.

### Microphone callback follow-up (0.1.2)

The prior callback converted every CPAL notification into a disconnect. CPAL 0.18.2 WASAPI emits `Xrun` for a capture discontinuity and continues delivering frames; the corrected callback counts these notifications and continues recording. Real device loss, route invalidation, permission failure and sanitized backend failure remain distinct, and startup preserves typed errors. The first fatal event survives later callbacks. The notification path takes no lock and allocates no memory. Device-route changes still stop recording under the frozen-endpoint policy. See [CPAL error categories](https://docs.rs/cpal/0.18.2/cpal/enum.ErrorKind.html).

All 55 affected application/core/platform tests pass (10/27/18); one unrelated interactive clipboard test remains ignored. Regression scenarios cover a discontinuity followed by valid samples and Stop, a real disconnect through Poll and Stop, first-fatal preservation, distinct route/permission/backend errors, and startup category preservation without backend text. Strict affected-crate Clippy, TypeScript/Vite build and generated contract checks pass. The opt-in `cargo run -p onda-platform --example microphone_level -- level` uses the actual capture implementation, prints only scalar diagnostics and discards audio. A real 3-second check of default SteelSeries Sonar completed at 48 kHz stereo with one discontinuity; it received zero signal during that sample and does not establish speech or virtual-device routing quality.

Nineteen actual hidden WebView2/native IPC checks also pass for 0.1.2. The optional-Vulkan installer built successfully at 12,335,823 bytes, with ProductVersion `0.1.2` and SHA-256 `389ed357a19ff8609f3ac927b420d70b6f1e43fbf970edf9875d639c27955ba6`. It was not installed over the user's running 0.1.1 process.

### Model-selection follow-up (0.1.1)

An installed Small Q5 asset with untouched Fast defaults reproduced a missing-model failure: Fast resolves to multilingual Tiny, and installing another model does not select it. The patch exposes the resolved native `selectedModelId`, distinguishes installed/selected models, names the missing asset in the operation status, and refreshes the model panel after settings changes. Regression tests verify default selection with only Small installed, explicit Small selection across a real SQLite restart, and clearing only the model-configuration error while retaining other errors.

All 36 application/core tests, strict Clippy for both affected crates, generated contract checks and the TypeScript/Vite build pass. Nineteen actual hidden WebView2/native IPC checks pass in each fresh and deliberately unavailable-cloud-storage profile, including the visible error and settings-to-model-display refresh. Isolated debug smoke now bypasses the singleton and omits global keyboard hooks so it can run beside an installed application; release builds always retain both. The 0.1.1 installer is built separately; the recorded install/uninstall evidence applies specifically to 0.1.0, and the live user installation was left running during this follow-up.

The optional-Vulkan `Onda_0.1.1_x64-setup.exe` build completed successfully at 12,324,515 bytes, with application ProductVersion `0.1.1` and SHA-256 `9b4435faa5311d5c303d6ddca817a2f26ead49cf59780d49098f8b5e60bea06f`. It was not installed over the running user application during validation.

### Original candidate

The final ordinary workspace suite contains 88 passing tests: application 7, core 26, cloud 23, ASR/model 12, shortcut 6, native platform 14. Strict workspace Clippy, Rust formatting, generated TypeScript contract checks, TypeScript/Vite production build, deterministic quality fixtures, dependency license/source/advisory policy and npm audit pass. Six environment-dependent tests are deliberately ignored by the ordinary suite and have separate positive runs: three real CPU ASR semantic/fallback/cancellation gates, one real Vulkan gate on RTX 5060, one controlled Win32 clipboard/selection/undo/password/race harness, and one real owned-worker Job Object termination test.

Real hidden WebView2 IPC smoke passed thirteen checks in both fresh and deliberately unavailable protected-cloud-storage profiles. The unavailable account directory sentinel remained unchanged, and local controls remained usable. `scripts/smoke.ps1 -WithDictationFixture` additionally supports a real application pipeline replay into a debug-only owned Win32 target. This optional replay currently refuses before inference because Windows denies foreground activation on this host, even after ordinary bounded Win32/UIA focus requests. The refusal is recorded rather than bypassing another app's input queue, injecting keys or weakening target validation. This is not counted as a passed end-to-end dictation test. Run it on an interactive desktop granting the controlled test window foreground access; `FixtureModelDirectory` must contain the checksum-verified tiny.en asset installed by the ASR benchmark.

The final engineering pass corrected stale-generation result/cancellation races, idle Escape interception, post-completion learning cancellation, unconditional default field reads, duration-limit audio loss, unplugged-device reporting, rotating-token crash safety, Unicode matching/protected URL bugs, optional cloud startup, unsigned updater initialization, per-user CRT closure, updater admission/shutdown, worker lifetime containment and uncopied failed-dictation overwrite. The repository contains no user-triggered todo/unimplemented stubs or audio persistence; transcript history is the encrypted, retention-bound store of ADR 005. Remaining release gates are the interactive matrix, real account consent, publisher signing/channel configuration and representative hardware qualification.
