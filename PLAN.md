# Plan

Keel retrofit of Onda, started 2026-11-13. Each slice is one pull request (or a short series)
to `master`, merged when Tier 1 CI is green. Probe IDs (A1, K4, P3, ...) refer to the planted bugs
in [docs/TEST-AUDIT.md](docs/TEST-AUDIT.md); a slice that adds tests is done only when each new
test has been seen red with its probe.

| # | Slice | Done when | Levels | Status |
|---|---|---|---|---|
| 0 | Publish and first CI | cleaned history on `miillehq/onda`; the existing `ci.yml` green on `windows-2022` and `ubuntu-24.04`, linked in `VERIFICATION.md` | L0 to L3, L6 smoke, L7 NSIS | done: run 43349184270 (PR #1) |
| 1 | Keel footprint | `keel.json`, `AGENTS.md`, `CLAUDE.md`, `ARCHITECTURE.md`, `TESTING.md`, `PLAN.md`, `VERIFICATION.md`, ADR 006 and 007 merged with Tier 1 green | none | done (PR #2) |
| 2 | CI in Keel's shape | fast Tier 1 PR jobs (lint, unit, contracts) on Windows and Linux; NSIS build and install test moved to `master`; advisory Tier 2 jobs on `macos-latest`, `macos-15-intel`, `windows-11-arm`, `ubuntu-24.04-arm` that report build status without blocking; `master` requires the Tier 1 checks | L0, L1 | done (PR #3) |
| 3 | First real-shell journey | WebdriverIO embedded provider in debug builds only; `npm run test:e2e` proves "settings and a dictionary entry survive a restart" on Windows and Linux, seen red by skipping the save | L6 | done (PR #4) |
| 4 | Close the probe gaps | tests that catch A1, A3, C1, C2, K3 (in `cargo test`), K4, K5, K6, P1, P3 and I1 | L1, L3 | done (PRs #7, #8, #10) |
| 5 | Strengthen and move | the 16 strengthen and 4 move verdicts of the audit, each seen red (left-edge crop, dropped KEYUP flag, missing 16 KiB cap, ...) | L1, L3 | done (PRs #7, #8, #10) |
| 6 | Contract suite and labelled mock | `contracts/` examples; Vitest harness (`npm run test:unit`); the mock and the real dispatcher answer the examples the same; mock badge; the three drifts (settings defaults, name graphemes, missing validation) caught red, then fixed | L1, L2 | done (PRs #5, #14) |
| 7 | UI logic unit tests | Vitest over `ui/src/lib` (diff, format, keys, router, activity, levels) | L1 | done (PR #6) |
| 8 | Browser UI and visual | Playwright against the mock on three OSes, chromium and webkit: onboarding, settings, dictionary, history, polish errors, pill states; tiers 1 to 3; snapshots workflow | L4, L5 | partly (PR #9): history is untested; tier 3 waits for the snapshots workflow |
| 9 | Installers | install, launch and uninstall NSIS (Windows), deb and AppImage (Linux) on `master` | L7 | partly (PRs #3, #12): the NSIS test does not launch the app |
| 10 | Retire replaced tests | `languages.rs` constant test (replaced by the engine language test), `public_account_dto_contains_no_credentials` (replaced by a key-set test), `ui/src/smoke.ts` checks covered by L6 | L1, L6 | done for both Rust tests (PRs #13, #15); `smoke.ts` stays by ruling |
| 11 | macOS builds | the workspace compiles and `cargo test` passes on `macos-latest` with macOS modules that report "unsupported"; the CPU worker builds; the app launches under the L6 suite | L0 to L3, L6 | in PR #11 |
| 12 | macOS storage and desktop | Keychain for API keys, accounts and the history key; launch at login; menu-bar icon; microphone usage text | L1, L3 on macOS | in PR #11 |
| 13 | macOS input | event-tap shortcut helper; Accessibility focused-field identity, password refusal and Cmd+V paste; matcher tests shared with Windows and Linux | L1, L3; real typing unverified | in PR #11 |
| 14 | macOS speech | Metal worker beside the CPU worker; golden real-model test on `macos-latest` | L3 | in PR #11 |
| 15 | macOS packaging | dmg built and installer-tested on `macos-latest`; signing and notarization hooks that stay off without an Apple account | L7 | in PR #11 |
| 16 | ARM | sidecars per target triple, no AVX2 worker on ARM, L1 to L6 green on `windows-11-arm` and `ubuntu-24.04-arm`, packages built | L1 to L7 | in PR #11 |
| 17 | Distributions | deb and AppImage install checks in Fedora, Debian and Arch containers, advisory | L8 | on `master`; first run pending |

Rulings:

- Slice 2: Ruling: Tier 2 runs on every PR as advisory jobs, not only nightly — the macOS and ARM
  ports need feedback per change — cost if wrong: about 4 extra jobs per PR, free on a public
  repository.
- Slice 3: Ruling: the first L6 journey uses settings and the dictionary, not dictation — hosted
  runners have no microphone or foreground rights — cost if wrong: dictation's real-shell path
  waits for the debug WAV replay fixture to be wired into L6.
- Slice 3: Ruling: `npm audit` gates shipped dependencies only (`--omit=dev`) and reports the
  rest — WebdriverIO 9, which `@wdio/tauri-service` 1.5.1 requires, carries high advisories in
  test tooling (`basic-ftp`, `extract-zip`, `serialize-javascript`) that never ships and is never
  fed FTP proxies, untrusted archives or serialized input — cost if wrong: a compromised test
  tool on a CI runner with a read-only token.
- Slice 3: Ruling: the WebdriverIO plugins and their capability exist only in `--features e2e`
  debug builds, through a config overlay, and a release build with the feature fails to compile
  — the shipped app keeps its two-command IPC surface — cost if wrong: none for users; tests
  need the e2e build.
- Slice 6: Ruling: the mock mirrors `Settings::validate` and `normalize_name` by hand rather than
  calling Rust (no WebAssembly build) — the shared examples hold every message to Rust's on both
  sides — cost if wrong: a rule without an example can drift until one is added.
- Slice 6: Ruling: only the fresh-install scenario must equal Rust's defaults; the other mock
  scenarios stand for a returning user and keep demo data — cost if wrong: none, they are labelled.
- Slice 8: Ruling: a pixel check without a baseline skips with a message, locally and in CI, until
  the snapshots workflow commits one — CI would otherwise be red until a runner captured it —
  cost if wrong: a deleted baseline skips rather than fails; tiers 1 and 2 still run.
- Slice 8: Ruling: the browser suite on macOS is advisory with the rest of Tier 2 — cost if wrong:
  a webkit regression on macOS waits for a release check.
- Slice 8, follow-up done in its own contract PR: the mock refuses a duplicate dictionary entry as
  Rust does, held by four new contract examples.
- Slice 10: replacements first (this PR), retirements later, each named:
  `languages.rs` `the_catalog_language_table_matches_whisper_order_and_excludes_cantonese` is
  replaced by `engine.rs` `languages_the_models_lack_never_reach_the_worker` (now also zh-HK, cmn
  refused and haw accepted); `api.rs` `public_account_dto_contains_no_credentials` is replaced by
  `the_account_sent_to_the_interface_has_only_its_public_fields`.
- Slice 10: Ruling: `ui/src/smoke.ts` stays until every check in it maps to an L6 test by name —
  the smoke is the only real-shell coverage today — cost if wrong: two suites run for a while.
- Slice 11: Ruling: macOS lands first as compile-only stubs, then features — a build that fails
  on every PR hides everything else — cost if wrong: stubs to remove later.
- Slice 13: Ruling: macOS typing into other apps is recorded as unverified until checked on a real
  Mac — runners cannot grant Accessibility or Input Monitoring — cost if wrong: a bug only users
  find.
- Slice 16: Ruling: ARM follows macOS — the sidecar naming work overlaps — cost if wrong: ARM
  users wait longer.
