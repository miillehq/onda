# Onda on Linux

Onda builds and runs on Linux (x86-64, X11 or Wayland). On Wayland, apps may not watch the
keyboard or type into other apps on their own, so Onda asks the desktop through its portals,
and reads the focused field through the accessibility bus (AT-SPI). This page says what each
feature uses, what the user approves, and how it was checked.

## What works, and how

| Area | Linux behaviour |
| --- | --- |
| Speech recognition | Same local Whisper workers (generic, AVX2 and Vulkan), built with GCC by `scripts/prepare.sh`. |
| Microphone | `cpal` on ALSA (PipeWire and PulseAudio provide ALSA devices). Device choice and level test as on Windows. |
| Shortcuts | The `onda-input` helper opens a global shortcuts portal session and binds every Onda shortcut. The desktop asks the user once and may let them pick other keys; it then reports each press and release, so hold-to-talk, hands-free, rewrite and voice edit work as on Windows. Cancel is Esc together with the dictation modifiers (Ctrl+Super+Esc by default), because a bound Esc alone would be taken from every app. If the desktop reports only presses, a quick release keeps recording until the next press. |
| Without the shortcuts portal | Bind a desktop shortcut to `onda --dictate`: the first press starts hands-free recording, the second finishes it. The tray menu has the same item. Settings › Shortcuts shows where to add it. |
| Typing into apps | After the user allows it once (Settings › Shortcuts › Typing into apps), Onda holds a remote desktop portal session with keyboard access and pastes like on Windows: dictation on the clipboard, then Ctrl+V (Ctrl+Shift+V in terminals), then the previous clipboard text back. The portal's restore token, kept in `typing-permission` in the data folder, restarts the session at the next launch without a dialog. GNOME shows a remote-access icon while it runs; stopping it there turns typing off. |
| Without that permission | A field that accepts edits through AT-SPI (GTK, Qt, LibreOffice) gets the text inserted directly. Elsewhere the dictation goes on the clipboard and the pill says "Copied · press Ctrl+V". The clipboard entry carries KDE's `x-kde-passwordManagerHint` so clipboard managers that honour it skip it. |
| The focused field | AT-SPI identifies it (app, window, role, position), refuses password fields, and reads the text before the caret, the selection and, for learning from corrections, the field after a dictation. Onda listens for focus changes and checks the reported field live, so an app without accessibility that took focus is not mistaken for the last one. Browsers and Electron apps expose their fields only when the desktop's accessibility support is on; there Onda pastes without checking the field. |
| AI polish | All providers, including Sign in with ChatGPT. The account record is encrypted (ChaCha20-Poly1305) under a key kept in the desktop keyring (Secret Service); API keys are stored in the keyring directly. Without a keyring, Onda refuses to store them and says why. |
| History | Encrypted the same way and kept for the chosen number of days. Without a keyring, history stays in memory until Onda quits. |
| Statistics, profile, dictionary, snippets | As on Windows (SQLite in `~/.local/share/com.miille.onda`). |
| Launch at login | An XDG autostart entry (`~/.config/autostart/onda.desktop`); `$APPIMAGE` is used when running from an AppImage. |
| Pill | On a Wayland desktop Onda runs through XWayland (`GDK_BACKEND=x11` unless already set), because Wayland lets the compositor place windows, ignore "always on top" and focus a window as it appears. The pill is an override-redirect window, made so before it is first shown: outside window management, it stays above the dock and never takes focus. |
| Tray icon | AppIndicator. Linux does not report the panel colour, so Automatic uses the white mark; choose Black for light panels. While Onda listens or works, the state ("Listening", "Polishing" …) shows beside the icon, since GNOME and KDE show no tray tooltips. |
| Resource report | Measures Onda's process tree from `/proc` (private resident memory), including WebKitGTK's processes. |

## Not available

- Elevated-target checks: Linux has no integrity levels; every app runs as the user.
- Fields in apps without accessibility (most Electron apps unless accessibility is on, games,
  some terminals) cannot be checked before pasting, read for context, or watched for corrections.

## Build and run

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libasound2-dev cmake clang libclang-dev pkg-config libxdo-dev \
  libvulkan-dev glslc   # the last two only for the Vulkan worker
# Rust from rust-toolchain.toml (rustup), Node 24
npm ci
scripts/prepare.sh            # debug speech workers and the shortcut helper into src-tauri/binaries
npm run tauri dev             # or: cargo build -p onda-app && npm run dev
scripts/build.sh --with-vulkan --bundles deb   # release build and a .deb (AppImage also supported)
```

`.cargo/config.toml` forces MSVC compiler flags for whisper.cpp's release build; `prepare.sh`
overrides them with `--config` for GCC/Clang, the same mechanism `prepare.ps1` uses to switch on
AVX2. Debug builds are unaffected (whisper-rs-sys builds them as RelWithDebInfo).

The keyring tests are ignored by default. Run them inside `dbus-run-session` with an unlocked
gnome-keyring that stays running, so its login collection is the default one:

```bash
dbus-run-session -- sh -c 'printf test | gnome-keyring-daemon --replace --unlock --foreground --components=secrets &
  sleep 2; cargo test -p onda-platform keyring -- --ignored; cargo test -p onda-cloud secret_service -- --ignored'
```

## How it was checked (November 2026)

In WSL (Ubuntu 24.04, WSLg) on Windows 11, with a private D-Bus session, an unlocked test
keyring and a radio playing into a VB-Audio virtual cable, which WSLg passes to Linux as the
default microphone:

- `cargo fmt --check`, `npm run contracts:check`, `cargo clippy --workspace --all-targets -D
  warnings` and `cargo test --workspace` pass on Linux; so do the two keyring tests above.
- `scripts/build.sh --with-vulkan --bundles deb` produced a 22 MB `.deb` with the app, the
  shortcut helper and the three speech workers; it was installed with `dpkg -i` and used for all
  checks below.
- Dictation (`onda --dictate`, Balanced, base.en on the CPU, about 0.3 s per 8 s of speech) was
  typed into a GTK 4 editor through AT-SPI's EditableText, spaced after the earlier text; later
  dictations appended. History saved them encrypted; Stats counted words, days, streak and pace.
- A capitalised one-word correction made within seconds of a dictation produced the "Add to your
  dictionary?" proposal on Home. Proposals reach the main window only while it is open.
- Under mutter 46 (GNOME's window manager, X11 on Xvfb) the pill showed bottom-centre with its
  transparent edges, never took focus, and the editor received the text while the pill was up.
  WSLg's own window system does give the override-redirect pill keyboard focus, so in WSLg the
  dictation falls back to the clipboard while the pill shows; that is specific to WSLg.
- Without the portals (WSLg has neither), Home and Settings › Shortcuts show the `onda --dictate`
  setup, Rewrites says its shortcuts need the portal, and Typing into apps says the desktop cannot
  let apps type.

Found and fixed during these runs: a loud, clipped microphone could ring past full scale after
resampling, and the speech workers then refused the recording (Windows too); the pill could take
focus; the newest month label could push the activity grid wider than the year (Windows too);
process names were cut to 15 characters.

Not checked: GNOME's global shortcuts and remote desktop portals (no GNOME session in WSL, and
Ubuntu 24.04's GNOME 46 has no shortcuts portal), the AppIndicator tray on a real panel,
browsers and Electron apps with accessibility on, Sign in with ChatGPT end to end, the Vulkan
worker on a real GPU, and KDE Plasma.
