# Manual release checks and opt-in tests

[TESTING.md](../TESTING.md) says what each automated level proves and where it runs. This page
holds what CI cannot do: the manual matrix run before a release, and the tests that are ignored
by default because they need a model, a desktop session or a real device. Record every result in
[VERIFICATION.md](../VERIFICATION.md) with the platform, the build and the date; a scenario that
was not run stays unverified.

## Manual matrix

0.3.0 ships unsigned packages for Windows x64 and ARM64 (NSIS), Linux x64 and ARM64 (deb and
AppImage) and macOS on Apple Silicon and Intel (dmg). Run each row on every package it names.

| Scenario | Packages | What to check |
| --- | --- | --- |
| Install, first launch, uninstall | all | Windows: SmartScreen warns, the per-user install needs no administrator, uninstall keeps `%LOCALAPPDATA%\com.miille.onda`. Linux: the deb installs and removes, the AppImage starts. macOS: the first launch is refused until Open Anyway in System Settings › Privacy & Security. Onboarding downloads a model and the practice field works. |
| Upgrade | where an earlier build (0.2.0 or older) exists | Settings, dictionary, snippets, History, accounts and models survive; a downgrade is refused. |
| Oldest supported systems | Windows x64 | Windows 10 22H2 with no WebView2: the bootstrapper installs it and Onda starts. |
| Shortcuts | all | Hold to talk, hands-free, cancel, rewrite and voice edit start and stop once per physical press. Windows: the keyboard helper. Linux: the global shortcuts portal on GNOME and KDE, and `onda --dictate` where there is none. macOS: Control+Shift+Space, and a clear message for a chord macOS already uses. |
| Typing into apps | all | Windows: Notepad, a browser, VS Code or another Electron app, a WPF or WinUI app, Windows Terminal. Linux, with typing allowed: a GTK app, a Qt app, LibreOffice, a browser, a terminal. macOS, with Accessibility on: TextEdit, Notes, Safari, a Chromium or Electron app, Terminal; without it, dictation is copied for ⌘V. After an update of the unsigned app, check whether the Accessibility switch must be turned off and on again. |
| Refusals | all | Password fields, Terminal's Secure Keyboard Entry (macOS) and elevated windows (Windows) are refused; a focus change during transcription keeps the text in History instead of typing it elsewhere. |
| Microphones | all | No microphone, unplugging during a recording, switching the default device; the macOS microphone prompt. Errors are clear and nothing hangs. |
| Session changes | Windows, Linux | Suspend and resume, a locked session, Remote Desktop (Windows), a saturated CPU: no stuck recording, the shortcut helper recovers. |
| AI providers | any one per OS | Real ChatGPT sign-in, model list, polish, sign-out; an OpenAI-compatible cloud service; an Anthropic key; Ollama, LM Studio and llama.cpp ([local-models.md](local-models.md)). A wrong key, a wrong model, rate limiting and going offline each keep the original text and call no other provider. |
| Speech hardware | all | CPU on an older x64 machine and on ARM64; Vulkan on NVIDIA, AMD and Intel (Windows, Linux); Metal on Apple Silicon. Note first and later dictation times. |
| Installer pages | Windows x64 | The branded NSIS pages at 100 %, 150 % and 200 % scaling and in a high-contrast theme. |
| Keyboard layouts | macOS | ⌘V on a layout that moves V (Dvorak). |
| Leaks | any | Logs and the diagnostics export hold no dictated text, selections, keys or tokens. |

## Real-model speech tests

The golden tests transcribe the public-domain JFK clip in `tests/fixtures/asr/` with a real
model and the real worker, check a failed GPU probe falls back to the CPU, and check cancellation
kills the worker. CI runs them on Windows. To run them anywhere, after `scripts/prepare.ps1` or
`scripts/prepare.sh` has built the debug workers, download the pinned tiny English model through
Onda's own verified downloader:

```powershell
$installation = (cargo run --locked -p onda-asr --bin onda-asr-benchmark -- download .artifacts/models whisper-tiny-en) -join "`n" | ConvertFrom-Json
$env:ONDA_ASR_TEST_MODEL = $installation.path
$env:ONDA_ASR_WORKER_DIR = Join-Path $PWD 'target/debug'
cargo test --locked -p onda-asr --test golden -- --ignored --test-threads=1
```

```bash
export ONDA_ASR_TEST_MODEL=$(cargo run --locked -q -p onda-asr --bin onda-asr-benchmark -- download .artifacts/models whisper-tiny-en | jq -r .path)
export ONDA_ASR_WORKER_DIR="$PWD/target/debug"
cargo test --locked -p onda-asr --test golden -- --ignored --test-threads=1
```

`gpu_golden.rs` needs a Vulkan GPU and the Vulkan worker; `scripts/asr-benchmark.ps1 -CheckVulkan`
builds and runs it ([asr.md](asr.md)). Speed measurements are in [benchmarks.md](benchmarks.md).

## Other opt-in tests

- Windows desktop harness: opens its own edit windows and checks Unicode paste, every clipboard
  format put back, selection replacement, undo and password refusal. It changes the clipboard
  and takes focus, so run it on an idle desktop:

  ```powershell
  $env:ONDA_DESKTOP_TESTS = '1'
  cargo test -p onda-platform real_edit_unicode -- --ignored --test-threads=1
  ```

- `scripts/smoke.ps1 -WithDictationFixture` replays a known recording through the whole debug
  pipeline into an owned edit window. Windows refuses it focus on many machines; a refusal is not
  a pass.
- Linux keyring and Secret Service round trips: see [linux.md](linux.md).
- The live local-model polish test: see [local-models.md](local-models.md).
