# Work log: MY-29 to MY-37 (branch `onda/my-29-37`)

A resumable record of the October 2026 batch: Linux build, profile and statistics, model
languages, custom AI providers, local polish models, tray icon choice, branded installer and
WebView2 attribution. No secrets belong here.

## Environment

- Linux VPS (Ubuntu 26.04, 4 vCPU, ~8 GB RAM, no GPU). Windows, WebView2, NSIS installation,
  microphones, GPU acceleration and real provider accounts cannot be proven here.
- Toolchain installed for this work: rustup with the pinned Rust 1.95.0 (`rust-toolchain.toml`),
  Node 24.22.2 from nodejs.org (checksum verified) in `~/.local/opt`, and apt packages for
  Tauri on Linux (WebKitGTK 4.1, GTK 3, ayatana-appindicator, librsvg, ALSA, OpenSSL), CMake,
  clang/libclang, ImageMagick, rsvg, poppler, NSIS and Xvfb.
- Builds use `CARGO_BUILD_JOBS=3` because other work shares the machine.

## Starting point

- `master` at `dd7a10c`; the only untracked path was `.claude/` (left untouched).
- No other agent was working in this repository when the batch started.
- Both "Make a linux compatible version" and "Windows Task Manager and WebView2 process
  attribution" arrived labelled MY-37; they are tracked here as separate items.

## Plan and order

1. Linux baseline (shared dependency: every Rust check below needs the workspace to build here).
2. Settings and contract additions shared by several items (tray icon style, AI provider,
   profile), then each feature on top.
3. Docs, notices, Windows/GPU/provider/installer checklists.

## Progress

(Updated as work lands; newest last.)

### 2026-10-29, session 1

- Baseline on Linux: `npm ci`, `npm run build` pass. `cargo test --workspace --exclude onda-app`
  passes (core 38, ASR 13, input 11, platform 10, cloud 9). `onda-app` failed to build: Tauri
  needs Linux sidecars (`binaries/onda-input-x86_64-unknown-linux-gnu`) that only
  `prepare.ps1` (Windows) produced.
- Linux build groundwork: `scripts/prepare.sh` (Linux counterpart of `prepare.ps1`; overrides
  the MSVC-only whisper.cpp release flags from `.cargo/config.toml` with `--config`), and
  `src-tauri/tauri.linux.conf.json` (Linux sidecars, deb/AppImage). Fixed a real portability
  bug: the worker's whisper log callback took `c_int`, but ggml's enum is unsigned under GCC;
  it now uses bindgen's `ggml_log_level` (`whisper-rs` feature `raw-api`).
- Research (sources recorded in the docs written for each item):
  - Model languages: the catalog's six models predate large-v3, so none has the Cantonese
    (`yue`) token; passing `yue` to them silently selects `<|translate|>` in whisper.cpp 1.8.3.
  - Local LLM: Qwen3.5-0.8B (Apache-2.0) exists; llama-server and Ollama turn thinking on unless
    the request turns it off.
  - WebView2 grouping: no supported API groups WebView2 processes under the host in Task
    Manager (WebView2Feedback #5628 open, unanswered); hosts can measure via GetProcessInfos.
- Core: settings for tray icon style and AI provider; migration 004 (profile table, lifetime
  usage totals backfilled from daily rows); `stats.rs` (streaks, pace), `profile.rs` (name rules,
  picture re-encoding that drops metadata). 51 core tests pass.
- Cloud: `providers.rs` (OpenAI-compatible, Anthropic Messages, Local loopback-only adapters;
  no redirects; local bypasses proxies; thinking switched off for local), `keys.rs` (DPAPI on
  Windows, Secret Service on Linux). 23 tests pass with mock servers.
- Real CPU check: llama.cpp b11960 (sha256-verified release) + Qwen3.5-0.8B Q4_K_M
  (sha256 bd2587…dc517) on this VPS. The ignored live test passed three times, 2.3–4.0 s per
  short sentence, ~960 MB RSS. `llama-bench -t 3`: prompt 92 tok/s, generation 13.7 tok/s.
- Commit `101c3c5`: Linux build, providers, keys, statistics, profile, tray style, process report.
- Commit `f4ca27b`: interface (Stats and share image, General profile and tray, AI polish
  providers, language notes, resource report, Linux variants); Cantonese and other unknown codes
  refused in settings, engine and worker.
- Commit `ebd6664`: Linux startup fix (DLL hardening was "unsupported"), golden tests use the
  platform executable suffix, branded NSIS bitmaps (`scripts/installer-art.sh`), extended smoke.
- Verified on Linux: real-model golden tests (3/3), native smoke under Xvfb/WebKitGTK (41/41),
  Secret Service round trip with an isolated gnome-keyring 50.0 (unpacked to the scratchpad, not
  installed), Tauri schema validation, `makensis` compile of the branded pages.
- Real Linux bugs found and fixed by running: worker log callback enum type, MSVC-only release
  flags, model-folder validation demanded a Windows drive, memory read as 0 (every profile fell
  back to Fast), local dates were UTC, DLL hardening failed startup.
- Decisions: local polish goes through a loopback OpenAI-compatible server (local-models.md);
  WebView2 attribution is reported, not altered (windows-integration.md); Anthropic server-side
  refusal fallbacks stay off (they would be an implicit billable fallback).

- cargo-deny 0.19.7 (release binary, checksum verified): licenses and sources pass for both the
  Windows and the Linux graph (`deny.toml` now lists both targets). Advisories fail on crates
  already pinned on `master`: rustls 0.23.45, rustls-webpki 0.103.15, tokio 1.53.1, webpki-roots
  1.0.9 (patch releases fix them) and zip 4.6.1 (pulled in by tauri-plugin-updater's default `zip`
  feature; the fix needs a newer zip major, so it stays until upstream moves or the feature is
  dropped after checking that updates never use v1-compatible archives).
- Interface wording made provider-neutral (error titles, "AI version", rewrites), and polish
  prefetch now contacts ChatGPT only when ChatGPT is the chosen provider.
- Notices regenerated on Linux: the Rust side gains the new crates, and the npm side lists Linux
  build-tool binaries instead of Windows ones (build.ps1 regenerates them on Windows).

- Linux release `.deb` built (11.75 MiB); added ALSA and D-Bus to its dependencies after `ldd`.
- Patch updates applied (rustls 0.23.46, rustls-webpki 0.103.16, tokio 1.54.1, webpki-roots
  1.0.11). Final pass: fmt, clippy (Linux, and Windows target for platform/input), 142 tests,
  contracts, frontend build, cargo-deny licenses/sources, golden 3/3, native smoke 41/41.
- Linux CI job added to `.github/workflows/ci.yml` (not run; nothing was pushed).

### Remaining (needs the owner)

- Everything in testing.md › "Your checklist": Windows build and smoke, installer, GPU, real
  provider keys, a real Linux desktop session.
- zip 4.6.1 advisories via tauri-plugin-updater (see testing.md).

### 2026-10-29, Windows review (Claude, desktop session)

Ran the branch on Windows 11 for the first time: dev build in an isolated `ONDA_DEV_PROFILE`,
CABLE Output (VB-Audio Virtual Cable) as the microphone fed by a radio stream. Onboarding, the
microphone test, a Fast model download and three dictations into Onda's practice box worked end
to end; Stats counted them (85 words, 3 dictations; the failed attempt was not counted), and the
process report, profile name, share image and provider forms worked. Providers and local models
were reviewed in code only (no keys or local server here).

Fixed:
- Privacy still said context is "Only sent to ChatGPT" with another provider chosen; both
  Privacy lines now name the chosen provider.
- Activity grid labelled a month at the week holding its 1st ("Nov" over a week that is still
  October) and needed a sideways scrollbar at 1120 px; months now start at their first Monday and
  the year fits the card. The share image showed "—" for an unknown pace; Active days stands in.
- Anthropic had no default model and its hint pointed at Claude Opus 5.5; the hint now suggests
  Claude Haiku and offers it in one click once the account's models are listed.
- The greeting example couldn't show a name; it now previews it.
- Copy trimmed where it was long or carried test-machine detail: AI polish (lede, provider,
  servers, the local-model table without the VPS timing), Stats (lede, definitions behind a
  closed "How these are counted"), General, Updates and help, Speech (Cantonese only under
  Chinese), onboarding. "AI version" reads "polished version".

Tooling found by running on Windows: the cloud tests' key fixture (`fixtures/test-private-key.pem`)
was never committed because `.gitignore` excludes `*.pem`, so no fresh checkout could build
those tests or clippy's all-targets pass; it now has an exception. Both contract drift checks
compared bytes, so a checkout with `core.autocrlf` always failed; they compare text now. After
that, every check passes on Windows (fmt, contracts, build, clippy twice, 128 tests, quality).

Checked and left as is: provider routing and the no-fallback rule, Anthropic request shape
(`anthropic-version: 2026-10-21`, no sampling parameters, `output_config.effort` only where the
Models API reports it), stream and error handling, key storage, picture decoding limits, stats
maths and the lifetime totals, language refusal.

Open: "Automatic" tray icon updates only on app theme changes (a taskbar-only change waits for
a restart); migration 004 is one-way, so 0.1.4 can't open a database this branch has upgraded;
dictation that starts while a Chromium app reports its window pane instead of the text box still
fails as a focus change (seen here in Onda's own window), the fix the owner deferred on master.

### 2026-11-02, Linux parity and lifetime streaks (Claude, desktop session)

Stats: active days and the longest streak were counted from the daily rows Onda reads (the
latest 366 active days), so both stopped growing after a year of use. They are running totals
now, like words; migration 005 backfills them from the existing rows.

Linux, to match Windows (details in [linux](linux.md)):
- Shortcuts: `onda-input` now builds for Linux and binds every shortcut through the global
  shortcuts portal; the supervisor and events are unchanged. Cancel is Esc with the dictation
  modifiers. Without the portal the app falls back to `onda --dictate`; a dismissed approval can
  be asked again from Shortcuts.
- Typing: an "Allow" in Shortcuts starts a remote desktop portal session (keyboard only, restore
  token for later launches) and dictation pastes as on Windows. Without it, fields that accept
  AT-SPI edits get the text directly; elsewhere it goes on the clipboard.
- The focused field: AT-SPI identifies it, refuses password fields, and reads it for spacing,
  selections, context, undo and learning from corrections.
- ChatGPT sign-in and History are encrypted under a key in the desktop keyring.
- The app runs through XWayland on Wayland desktops, and the pill is an override-redirect window
  made before its first show; the tray shows the state beside the icon.
- The Linux bundle adds the shortcut helper and the Vulkan worker.

Found while testing in WSL: clipped input rang past ±1 after resampling and the speech workers
refused the recording (Windows too; clamped now); the newest month label could widen the
activity grid past the year (Windows too); process names were cut to 15 characters; about a
dozen strings said "PC" on Linux.

Not verified here: the GNOME portals themselves, the tray on a real panel, browsers and Electron
apps with accessibility on, ChatGPT sign-in end to end, Vulkan on a real GPU, KDE.
