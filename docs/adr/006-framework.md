# ADR 006: Framework and its test consequences

Status: accepted · Date: 2026-11-13 · Decided by: procedure step 3 (testable on hosted runners), confirming [ADR 001](001-runtime-and-isolation.md)

## Context

Onda is an existing Tauri 2 app: a Preact and Vite UI over six Rust crates (ADR 001). The
maintainer now wants Windows, Linux and macOS on x64 and ARM, proven on GitHub's hosted runners,
and the repository is public, so those runners are free. The question is not whether to switch
frameworks but what the framework means for testing on every platform.

## Decision

Keep Tauri 2 with the Preact and Vite UI and the Rust workspace. Keel profile `tauri-web`.

## Rejected

- Electron: one browser engine would make pixel baselines simpler, but a bundled Chromium for an
  always-on tray app costs about 200 MB and double the memory, and the native input work stays in
  Rust either way.
- Native UIs per OS (WinUI, AppKit, GTK): three UIs to build and no shared browser tests.
- Flutter: no DOM, so no structure or geometry baselines, and nothing gained for native input.

## Consequences

- Testing: `cargo test` (L1, L3) on every runner; Vitest for UI logic (L1); a contract suite
  that runs the same requests against the labelled mock and the real dispatcher (L2); browser UI
  tests against the labelled mock with Playwright on chromium and webkit (L4, L5); a real-shell
  suite with WebdriverIO's embedded provider driving the debug app (L6); installer tests (L7) for
  NSIS on Windows, deb and AppImage on Linux and dmg on macOS. Three webviews (WebView2,
  WebKitGTK, WKWebView), so pixel baselines are per OS and the ARIA and geometry tiers carry
  cross-platform consistency.
- What hosted runners can drive, one line per platform:
  - windows-x64 on `windows-2022`: the GUI and IPC, L4 to L7. Typing into other apps needs
    foreground rights the runner does not reliably grant, so it stays in the manual matrix.
  - linux-x64 on `ubuntu-24.04` under Xvfb: the GUI and IPC. Portal shortcuts and typing need a
    real GNOME or KDE session: manual.
  - macos-arm64 on `macos-latest`: the GUI and IPC once ported. Accessibility and Input
    Monitoring cannot be granted on hosted runners, so dictation into other apps stays unverified
    until someone checks it on a real Mac.
  - windows-arm64 (`windows-11-arm`) and linux-arm64 (`ubuntu-24.04-arm`): as their x64 twins.
- Native work stays in the crates, one module per OS concern; ADR 007 lists the macOS pieces.
- Packaging and signing: NSIS, deb, AppImage and dmg. Apple Developer ID signing and notarization
  are required before the first macOS release (no account yet). Windows Authenticode hooks exist
  and stay optional; Linux packages are unsigned.
- Speech: the whisper.cpp CPU worker everywhere (NEON on ARM, where the AVX2 variant does not
  exist), the optional Vulkan worker on Windows and Linux, Metal on macOS.
- Agents: Tauri 2's APIs differ from Tauri 1; the profile's recipes give the current shapes.
