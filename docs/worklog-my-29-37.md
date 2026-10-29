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
