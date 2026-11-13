# Onda on Linux

Onda runs on Linux with X11 or Wayland. x86-64 is a supported platform; ARM64 builds and passes
its tests in CI but is not yet supported ([ADR 007](adr/007-platform-support.md)). On Wayland,
apps may not watch the keyboard or type into other apps on their own, so Onda asks the desktop
through its portals, and reads the focused field through the accessibility bus (AT-SPI). This
page says what each feature uses, what the user approves, and what has not been checked.

## What works, and how

| Area | Linux behaviour |
| --- | --- |
| Speech recognition | The same local Whisper CPU workers (baseline and, on x86-64, AVX2), built with GCC by `scripts/prepare.sh`. A Vulkan worker can be built (`--with-vulkan`), but the engine does not try it on Linux yet, because Linux does not enumerate GPU adapters ([asr.md](asr.md)). |
| Microphone | `cpal` on ALSA (PipeWire and PulseAudio provide ALSA devices). Device choice and level test as on Windows. |
| Shortcuts | The `onda-input` helper opens a global shortcuts portal session and binds every Onda shortcut. The desktop asks the user once and may let them pick other keys; it then reports each press and release, so hold-to-talk, hands-free, rewrite and voice edit work as on Windows. Cancel is Esc together with the dictation modifiers (Ctrl+Super+Esc by default), because a bound Esc alone would be taken from every app. GNOME reports a held shortcut's keyboard repeats as more presses; Onda ignores a press that comes within 0.7 s of the last one with no release between. A quick release keeps recording until the next press, and so does a release the desktop never reports (GNOME may not, if a modifier is let go before the key). Only hold-to-talk and voice edit are held; rewrite, hands-free and cancel act once per press, so a lost release never blocks the others. |
| Without the shortcuts portal | Bind a desktop shortcut to `onda --dictate`: the first press starts hands-free recording, the second finishes it. The tray menu has the same item. Settings › Shortcuts shows where to add it. |
| Typing into apps | After the user allows it once (Settings › Shortcuts › Typing into apps), Onda holds a remote desktop portal session with keyboard access and pastes like on Windows: dictation on the clipboard, then Ctrl+V (Ctrl+Shift+V in terminals), then the previous clipboard text back. The portal's restore token, kept in `typing-permission` in the data folder, restarts the session at the next launch without a dialog. GNOME shows a remote-access icon while it runs; stopping it there turns typing off. |
| Without that permission | A field that accepts edits through AT-SPI (GTK, Qt, LibreOffice) gets the text inserted directly. Elsewhere the dictation goes on the clipboard and the pill says "Copied · press Ctrl+V". The clipboard entry carries KDE's `x-kde-passwordManagerHint` so clipboard managers that honour it skip it. |
| The focused field | AT-SPI identifies it (app, window, role, position), refuses password fields, and reads the text before the caret, the selection and, for learning from corrections, the field after a dictation. Onda listens for focus changes and checks the reported field live, so an app without accessibility that took focus is not mistaken for the last one. Browsers and Electron apps expose their fields only when the desktop's accessibility support is on; there Onda pastes without checking the field, and a rewrite copies the selection with Ctrl+Insert through the typing permission (Ctrl+C would interrupt a terminal program) and puts the clipboard back; before pasting the result it copies the selection again and stops if it changed. Voice edit reads the selection while its keys are held, so in those apps it needs accessibility on. |
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
scripts/build.sh --bundles deb,appimage        # release build, a .deb and an AppImage
scripts/installer-test.sh                      # install, launch and remove them (L7)
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

## What is checked, and what is not

CI runs the Rust tests, the browser suite, the real-shell journeys and the WebKitGTK native smoke
on Ubuntu 24.04 under Xvfb, and installs, launches and removes the deb and the AppImage
([VERIFICATION.md](../VERIFICATION.md)). Runners have no desktop session, so the following are
not checked automatically and are in the manual matrix ([testing.md](testing.md)): GNOME's and
KDE's global shortcuts and remote desktop portals (Ubuntu 24.04's GNOME 46 has no shortcuts
portal), typing into real apps, the AppIndicator tray on a real panel, browsers and Electron apps
with accessibility on, Sign in with ChatGPT end to end, and the keyring round trips above.

Known limits: correction proposals reach the main window only while it is open. Under WSLg the
override-redirect pill takes keyboard focus, so dictation there falls back to the clipboard while
the pill shows; WSLg also has neither portal.
