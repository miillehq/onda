# Onda on Linux

Onda builds and runs on Linux (x86-64, X11 or Wayland) with a smaller feature set than on Windows.
Windows remains the primary platform; this page says exactly what the Linux build does and how it
was checked.

## What works

| Area | Linux behaviour |
| --- | --- |
| Speech recognition | Same local Whisper workers (generic and AVX2), built with GCC by `scripts/prepare.sh`. |
| Microphone | `cpal` on ALSA (PipeWire and PulseAudio provide ALSA devices). Device choice and level test as on Windows. |
| Starting dictation | No keyboard hook. Bind a desktop shortcut to `onda --dictate`: the first press starts hands-free recording, the second finishes it. The tray menu has the same item. A second launch with `--dictate` forwards to the running instance. |
| Delivering text | Onda cannot type into other apps, so a finished dictation goes on the clipboard (X11, or XWayland on Wayland desktops) and the pill says "Copied · press Ctrl+V". The clipboard entry carries KDE's `x-kde-passwordManagerHint` so clipboard managers that honour it skip it. |
| AI polish | All providers except Sign in with ChatGPT, whose credentials need Windows DPAPI. API keys are stored in the desktop keyring (Secret Service); without one, Onda refuses to store keys and says why. |
| History | Kept in memory until Onda quits. Onda encrypts saved history with Windows DPAPI and never writes it unencrypted, so nothing is saved on Linux. |
| Statistics, profile, dictionary, snippets | As on Windows (SQLite in `~/.local/share/com.miille.onda`). |
| Launch at login | An XDG autostart entry (`~/.config/autostart/onda.desktop`); `$APPIMAGE` is used when running from an AppImage. |
| Tray icon | AppIndicator. Linux does not report the panel colour, so Automatic uses the white mark; choose Black for light panels. |
| Resource report | Measures Onda's process tree from `/proc` (private resident memory), including WebKitGTK's processes. |

## Not available yet

- Hold-to-talk, Esc to cancel, and the rewrite and spoken-edit shortcuts: these need a system-wide
  keyboard hook. X11 could support key grabs, and Wayland's GlobalShortcuts portal could support
  press/release, but neither is implemented.
- Typing into the focused field, target validation, undo and learning: these use UI Automation and
  SendInput on Windows. AT-SPI and the RemoteDesktop portal are the likely Linux routes.
- Rewriting selected text and context from nearby text (they need the focused field).
- On Wayland, the compositor may place the dictation pill itself and may ignore always-on-top.

## Build and run

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libasound2-dev cmake clang libclang-dev pkg-config libxdo-dev
# Rust from rust-toolchain.toml (rustup), Node 24
npm ci
scripts/prepare.sh            # debug speech workers into src-tauri/binaries
npm run tauri dev             # or: cargo build -p onda-app && npm run dev
scripts/build.sh --bundles deb   # release workers, notices and a .deb (AppImage also supported)
```

`.cargo/config.toml` forces MSVC compiler flags for whisper.cpp's release build; `prepare.sh`
overrides them with `--config` for GCC/Clang, the same mechanism `prepare.ps1` uses to switch on
AVX2. Debug builds are unaffected (whisper-rs-sys builds them as RelWithDebInfo).

## How it was checked (October 2026, Ubuntu 26.04 VPS, no GPU, no microphone)

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt
  --check`, `npm run contracts:check` and `npm run build` pass on Linux.
- Real-model golden tests (`cargo test -p onda-asr --test golden -- --ignored`) pass with the
  Linux CPU worker: the JFK clip transcribes, cancellation kills the worker, and a failed GPU probe
  falls back to the CPU.
- The debug app ran under Xvfb with WebKitGTK and passed all 41 checks of the native smoke harness
  (`ONDA_SMOKE_DIRECTORY`), including the new profile, provider, language, tray and resource
  checks.
- Secret Service key storage passed its round trip against gnome-keyring 50.0 on a private D-Bus
  session (the test is ignored by default; see `crates/onda-cloud/src/keys.rs`).
- `scripts/build.sh --bundles deb` produced a 11.75 MiB `.deb` with the app and both workers
  (inspected, not installed).
- Not checked: a real microphone, the clipboard handoff in a desktop session, the AppIndicator tray
  on a real panel, Wayland desktops, and the packaged app on a clean machine.
