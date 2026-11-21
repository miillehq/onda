# Testing strategy

Strategy only: what each level proves, the tool, the command and where it runs. Evidence goes in
[VERIFICATION.md](VERIFICATION.md). The audit behind this plan is [docs/TEST-AUDIT.md](docs/TEST-AUDIT.md);
the manual release matrix and the opt-in tests are in [docs/testing.md](docs/testing.md).

## Levels

Every check runs at the cheapest level that can catch the bug it guards.

| Level | Proves | Tool | Command | Runs | Today |
|---|---|---|---|---|---|
| L0 | Source builds, fits its limits and is reachable | Rustfmt, Clippy, TypeScript, Biome, Knip and the parsed Rust module graph | `npm run lint` | every PR, native plus one cheap source job | all desktop OS branches enter the source measurement |
| L1 | a function or module does what it says | `cargo test`, Vitest | `cargo test --locked --workspace`; `npm run test:unit` | every push | Rust; Vitest harness |
| L2 | the mock and the real backend agree | ts-rs drift check; `contracts/examples.ts` run against the mock (Vitest) and the real app (L6) | `npm run contracts:check`; `npm run test:unit` | every push | types, defaults, validation, profile names |
| L3 | modules work against real I/O: SQLite files, temp dirs, localhost servers, child processes, DPAPI or keyring | `cargo test`; golden real-model tests | `cargo test --locked --workspace` | every push | yes |
| L4 | a user can do the journey, against the labelled mock | Playwright, chromium and webkit (`tests/ui/`) | `npm run test:ui` | every PR, 3 OSes | onboarding, dictionary, settings, error display, History, the Mac wording (`?platform=macos`) |
| L5 | structure, layout and pixels have not drifted | ARIA snapshot, geometry snapshot, per-OS pixels | with L4 | with L4 | tiers 1 to 3; tier 3 baselines for chromium and webkit on Windows, Linux and macOS, from the snapshots workflow (PR #19) |
| L6 | Desktop startup, real IPC, target capabilities and persistence through process restarts | WebdriverIO embedded provider and native hidden smoke | `npm run build:e2e` then `npm run test:e2e` | six native targets | contract errors, content policy, changed theme, dictionary, multiline snippet and screenshot evidence |
| L7 | Installed binary, release workers, in-place replacement, data migration and removal | NSIS, deb, AppImage and DMG on fresh runners | `installers.yml` with `upgrade_from` | release, master or branch dispatch | candidate and public artifacts carry verified digests |
| L8 | Additional distributions resolve libraries and launch their packaged UI as an ordinary user | Debian, Fedora and Arch under Xvfb | `installers.yml` | hosted distribution jobs | assistive technology and physical hardware are measured separately |

Rules for the mock: it implements the same `Backend` interface from the same generated types,
shows a visible badge that the mock-based browser journeys assert, and is checked against the real
dispatcher by the contract suite before any test depends on it.

## Critical journeys

| Journey | Proven at |
|---|---|
| Hold, speak, release and type | `crates/onda-input/src/matcher.rs` covers held keys, repeated events and releases. `crates/onda-asr/tests/golden.rs` sends a public WAV through the real worker. `tests/e2e/01-change.spec.ts` checks native typing permission. The Windows WebView2 smoke can drive `replayDebugFixture` into its own edit control. Third-party foreground behavior and physical devices are recorded separately in the manual matrix. |
| AI polish, protected text and a failed rewrite | `crates/onda-cloud/src/providers/tests.rs` uses local protocol fixtures for HTTP, response and stream failures. `crates/onda-core/src/text.rs` checks protected-output rules. `tests/smoke/desktop.ts` verifies that a failed preview and retry retain the input. `tests/e2e/00-contract.spec.ts` refuses a retry after its provider endpoint changes. Manual live-account evidence is tracked separately. |
| ChatGPT sign-in, refresh and sign-out | L3 synthetic provider registration, responses, sign-out and concurrent refresh in `crates/onda-cloud/src/oauth/tests.rs`. Windows uses DPAPI. Hosted Linux and macOS exercise a private Secret Service session or Keychain through `scripts/credential-fixtures.sh`. Real accounts belong to the manual matrix. |
| Download, verify and select a model | L3 resume, checksum and cancellation in `crates/onda-asr/src/models.rs` and its `models/` submodules. L4 clicks the already-selected, missing Fast model and checks the installed state in `tests/ui/speech-models.spec.ts`. |
| Dictionary, snippets and settings survive a restart | L3 SQLite in `crates/onda-core/src/persistence.rs`. L6 creates a word and a multiline snippet through their editors, changes theme and checks all three after the process restarts in `tests/e2e/01-change.spec.ts` and `02-restart.spec.ts`. L1 settings replacement in `src-tauri/tests/settings_requests.rs`. L2/L6 malformed inputs in `contracts/settings-inputs.ts`, exercised by `tests/unit/contract.test.ts` and `tests/e2e/00-contract.spec.ts`. |
| Production startup and native policy | `npm run test:production-ui` loads the distributable renderer without IPC in Chromium and WebKit, checks its visible error and scans the emitted assets for the mock and smoke fixture. `src-tauri/src/navigation.rs` tests allowed application origins. `tests/e2e/00-contract.spec.ts` checks the content policy through a harmless image in each real webview. |
| History retention and statistics | L3 retention rollback and future schema refusal in `crates/onda-core/tests/profile_storage.rs`. Temporary unlock failures, expiry and delayed writes in `src-tauri/src/runtime/history/tests.rs`. Atomic PNG saves in `src-tauri/src/runtime/stats_export.rs`. Browser coverage in `tests/ui/history.spec.ts` and `tests/ui/stats.spec.ts`. |
| Install, upgrade, uninstall | L3 migrations in `crates/onda-core/src/persistence/tests.rs`. L7 native install and launch in `scripts/installer-smoke.ps1` or `scripts/installer-test.sh`. `scripts/installer-upgrade.mjs` checks the same installed profile across builds. `published-installers.yml` exercises downloaded assets. |
| Restore a dictionary backup | L3 large round trips, malformed rows and oversized inputs in `crates/onda-core/tests/dictionary_archives.rs`. L2/L6 import and duplicate handling in `contracts/examples.ts`, executed by `tests/unit/contract.test.ts` and `tests/e2e/00-contract.spec.ts`. |
| Edit and share | `tests/ui/dictionary.spec.ts` checks imported text, UTF-8 failures, editing focus, Enter and Shift+Enter in a replacement, and that a typed word loses the spaces around it. `tests/ui/rewrites.spec.ts` saves and clears a per-rewrite model. `tests/ui/stats.spec.ts` checks the encoded PNG dimensions. `tests/unit/dictionary-file.test.ts` checks the size before reading a file. |
| Speech worker selection and punctuation | `crates/onda-asr/tests/worker_policy.rs` checks CPU and Metal selection. `crates/onda-asr/tests/golden.rs` drives the built workers using `scripts/asr-golden.mjs`. `crates/onda-core/src/text.rs` checks East Asian opening and closing punctuation at the caret, and the text that follows full-width punctuation. |
| Native clipboard and accessible text | `crates/onda-platform/src/text_clipboard.rs` covers paste preconditions and rich content. `linux_edits.rs` checks a same-length selection and UTF-8 byte/character boundaries. `linux_clipboard.rs` exercises a private Xvfb display. `macos_clipboard.rs` uses a private pasteboard. `src-tauri/src/runtime/delivery.rs` checks cancellation before writing. The Linux permission file uses the interrupted-write case in `crates/onda-platform/src/private_file.rs`. |
| Delivered usage, late dates and correction observations | `src-tauri/src/runtime/dictation_usage.rs` covers one credit across recovery paths. `crates/onda-core/tests/usage_calendar.rs` uses a real SQLite store for streaks and invalid dates. `src-tauri/src/runtime/history/tests.rs` covers unchanged previews and `runtime/learning.rs` bounds observation time. |
| IPC argument shapes | `src-tauri/tests/request_shapes.rs` checks every registered command before dispatch, command and provider names written as anything but a string, and every nullable argument left out. `tests/unit/contract.test.ts` checks the mock against the generated schema and on the JSON the bridge would carry. `tests/e2e/00-contract.spec.ts` sends representative malformed envelopes, and a call without a request, through the native bridge. |
| Default rewrites, edit fidelity and Unicode names | `crates/onda-core/tests/default_rewrite.rs` checks the delete itself, then reopens a real profile. `tests/ui/rewrites.spec.ts` changes one field and verifies the untouched value. `tests/ui/settings.spec.ts` types forty flag graphemes. `src-tauri/src/runtime/retry_route.rs` pins a model, checks a changed recipient and refuses a missing previously resolved model. |

Coverage is tracked per journey in this table, never as a percentage. A change that removes a
level from a journey needs a replacement that proves the same behaviour first.

## Visual tiers

- Tier 1, structure: ARIA snapshots, one baseline shared by every OS and engine.
- Tier 2, geometry: rounded boxes of elements marked `data-geo`, one JSON baseline, 8 px tolerance.
- Tier 3, pixels: full-page PNG per engine per OS, `maxDiffPixelRatio` 0.002 (0.001 for the pill),
  animations disabled. Baselines come only from the snapshots workflow on the runners
  (`.github/workflows/snapshots.yml`). A missing image fails CI. Local runs report a missing
  platform image as a skip.
- Every browser test fixes the clock (2026-11-13 10:00) so greetings and dates never move a
  baseline. Refresh tiers 1 and 2 after an intended change with
  `UPDATE_SNAPSHOTS=1 npx playwright test -g "tier 1|tier 2" --update-snapshots=all`.

## Proving a test

A new or rewritten test is trusted only after it has failed: break the code it guards, run it
red, restore, run it green, and record the red message in the PR. The audit's 17 planted bugs
(A1 to I2 in `docs/TEST-AUDIT.md`) are the first probes; each missed one gets a test.

## Flaky tests

A test that fails without a code change is reproduced first (a loop around `cargo test`, or
`--repeat-each=30` in Playwright, on the failing platform), then quarantined with `#[ignore]` or
`test.fixme` and an issue that records the rate and the message, and kept running in an advisory
job. It returns only after a fix that is shown red without it. Retries are allowed only in L6 and
L7 jobs, once; below that a retry hides a bug. The WebView2 smoke uses its retry after a timeout
only, and each retry is added to [issue #25](https://github.com/miillehq/onda/issues/25).

The Windows installer check retains a 30-second monotonic startup deadline. It reads only log bytes appended by the launch and checks the emitting process when the package supplies a PID. Its first report is retained. One L7 retry is available for the specific startup observation timeout. A missing worker, installation error or second timeout fails. The separate first-launch sample uses one immutable installer on thirty fresh hosted Windows machines with one attempt each. [Windows L7 tracker](https://github.com/miillehq/onda/issues/63)

## Budgets

- L0 to L3 on one runner: under 3 minutes with a warm cache (2 minutes on Windows on 2026-11-12).
- Whole PR wall clock: 20 minutes. The NSIS build and installer test run on `master` and releases.
- L4 and L5 per OS: under 5 minutes. L6 per platform: under 15 minutes including the debug build.
- L7 per OS aims for 25 minutes including the release build. The distribution and public-upgrade
  jobs report their own durations.

Intel macOS builds its three genuine debug sidecars once. The ordinary app tests and real
WKWebView journeys consume that archive in parallel. Native crate and private Keychain cases run
independently. The release gate checks each lane and its named steps. For timing investigations,
record both Actions wall time and time waiting for a hosted runner.

Ubuntu runners cache only Debian archives. The action refreshes signed Ubuntu indexes, uses Ubuntu’s `apt-cache` to check restored files against their indexed version, architecture and SHA-256, and runs the ordinary APT install. Cache keys include the Ubuntu release, CPU architecture, package group and week. The browser and native test suites keep their own timing measurements.

## Deliberately not automated

Physical microphones and hot-plug, typing into third-party apps (Notepad, browsers, Electron,
terminals), elevated and RDP sessions, suspend and resume, real ChatGPT accounts and real
provider keys, GPU vendors beyond the measured ones, and macOS Accessibility prompts. These stay
in the manual matrix in `docs/testing.md`, recorded per release in `VERIFICATION.md`.
