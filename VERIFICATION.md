# Verification ledger

Append-only. One row per claim; a claim names the level, the platform and the run (a CI run link
or the exact local command and commit). Anything not here is "built", not "tested". Records from
before Keel are in [docs/testing.md](docs/testing.md) and were not re-verified.

Commits: the 2026-11-12 runs used `852860d`, which the attribution cleanup rewrote to `60eb6d6`
with an identical tree.

| Date | Level | Claim | Platforms | Evidence |
|---|---|---|---|---|
| 2026-11-12 | L0 | rustfmt and clippy (workspace, warnings as errors) are clean | windows-x64, local | `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings` |
| 2026-11-12 | L1, L3 | workspace suite: 166 passed, 0 failed, 7 ignored | windows-x64, local | `cargo test --locked --workspace` |
| 2026-11-12 | L2 | generated TypeScript contracts match the Rust types | windows-x64, local | `npm run contracts:check` |
| 2026-11-12 | L1 | quality fixtures pass; 10,000-entry index builds in 68 ms | windows-x64, local | `npm run quality` |
| 2026-11-12 | probe | 17 planted bugs: 6 caught, 11 missed (A1, A3, C1, C2, K3, K4, K5, K6, P1, P3, I1); K3 is caught by `npm run quality` only | windows-x64, local | `cargo test -p <crate>` per bug in a copy of `852860d`; list in `docs/TEST-AUDIT.md` |
| 2026-11-13 | L0 to L3 | after moving `target/` to the D: build drive and a cold rebuild: fmt, both clippy runs, 166 passed and 7 ignored, contracts, TypeScript and Vite build, quality | windows-x64, local | `scripts/prepare.ps1` then the commands above on `60eb6d6` |
| 2026-11-13 | L6 | real app: a dictionary word and the dark theme survive a restart (2 spec files, 5 tests); stable in 8 of 8 runs, about 7 s each | windows-x64, local | `npm run build:e2e`; `npm run test:e2e` |
| 2026-11-13 | L6 probe | the restart spec fails when settings saves are dropped (`ON CONFLICT DO NOTHING`): "Expected: dark, Received: system" in `02-restart.spec.ts` | windows-x64, local | planted in `crates/onda-core/src/persistence.rs`, rebuilt, run red, restored, run green |
| 2026-11-13 | L2 | the development mock answers all 15 contract examples (fresh-install defaults, 9 settings refusals, 5 profile-name cases) | windows-x64, local | `npm run test:unit` |
| 2026-11-13 | L2 probe | each audit drift turns the mock red: code-point name counting fails "the name limit counts graphemes"; demo settings on a fresh install fail "a fresh profile starts from Rust's default settings"; skipping validation fails all 9 refusals | windows-x64, local | the old behaviour restored one at a time in `ui/src/mock/`, `npx vitest run` |
| 2026-11-13 | L6 | the real app answers the same 15 examples on a fresh profile and shows no mock badge; all 3 spec files pass | windows-x64, local | `npm run build:e2e`; `npm run test:e2e` |
| 2026-11-13 | L6 probe | a changed Rust message fails `00-contract.spec.ts` ("History can be kept for 30 days at most." vs the example) | windows-x64, local | planted in `crates/onda-core/src/contracts.rs`, rebuilt, run red, restored |
| 2026-11-13 | L1 | the interface logic in `ui/src/lib` (word diff, shortcut keys, activity grid, formatting, microphone level, error routes): 20 tests | windows-x64, local | `npm run test:unit` |
| 2026-11-13 | L1 probe | one planted bug per module turns its tests red: diff ties, Esc accepted, Sunday-first weeks, the MB threshold, an error action to a missing route, swapped attack and release | windows-x64, local | each planted in `ui/src/lib/`, `npx vitest run tests/unit/lib.test.ts`, restored |
| 2026-11-13 | L1, L3 | probes A1 (model checksum before inference), A3 (CPU-only never probes a GPU), C1 (PKCE verifier, code and redirect checked by the token mock), C2 (the 256 KiB cap is per event) now caught; the OAuth callback, JWT and pre-cancel tests run beyond Windows; the Job Object kill test runs unignored; `audio_context` is tested in the library. onda-cloud 42 passed and 1 ignored, onda-asr 18 passed | windows-x64, local | `cargo test --locked -p onda-cloud -p onda-asr` |
| 2026-11-13 | probe | every new, strengthened or moved test above failed with its bug planted (checksum skipped; CPU-only dropped; nonce, state or a wrong redirect sent; the frame counter not reset; state and nonce checks removed; kill-on-close removed; the context margin removed) and passed once restored | windows-x64, local | planted one at a time in a worktree |
| 2026-11-13 | L1 | probes P1 (paste waits while Onda still owns the unread clipboard), P3 (silence, speech and clipping from `RecordingBuffer::finish`) and I1 (an auto-repeat never starts a shortcut) now caught; strengthened: the paste chord's KEYUP flags and marker, the 16 KiB helper line cap, the local day key and `civil_from_days`. onda-platform 28 passed and 1 ignored, onda-input 13 and 1 passed | windows-x64, local | `cargo test --locked -p onda-platform -p onda-input` |
| 2026-11-13 | probe | each of those failed with its bug planted (owner check inverted, KEYUP dropped, marker zeroed, silence threshold inverted, energy not summed, clipping without `abs`, repeat filter dropped, size check removed or off by one, year and month offsets, leap-day arithmetic) and passed once restored | windows-x64, local | planted one at a time in a worktree |
| 2026-11-13 | L4, L5 | browser suite against the labelled mock: onboarding, dictionary add and cancel, theme switch, error display on Home and in the pill; tier 1 ARIA and tier 2 geometry baselines shared by both engines; 26 passed, 8 pixel checks skipped (no runner baselines yet); stable in 3 of 3 runs | windows-x64, local, chromium and webkit | `npm run test:ui` |
| 2026-11-13 | L4, L5 probe | a renamed button fails the onboarding behaviour and its tier 1 check; a wider sidebar fails both tier 2 checks; a dictionary save that does nothing fails "adding a word"; removing the mock badge fails all 17 tests | windows-x64, local, chromium | planted one at a time, `npx playwright test --project=chromium`, restored |
| 2026-11-13 | L1, L3 | probes K3 (a strict cleanup may not shrink a long dictation below half), K4 (the 400-day prune keeps the newest days), K5 (the history cap keeps the newest 2,000 dictations) and K6 (a finished or cancelled operation ignores late results) now caught; strengthened: the prompt budget, atomic import, the newer-schema refusal, picture centre crop and size limits, the rewrite diff's exact segments, provider routes, key slots and models. onda-core 57 passed, onda-app 20 passed | windows-x64, local | `cargo test --locked -p onda-core -p onda-app` |
| 2026-11-13 | probe | 39 planted bugs, one at a time, each failing only its intended test (for example the prune and history cap turned ascending, `matches()` without `is_active()`, the crop origin at 0, shared key slots, a missing model sent as an empty string) and passing once restored | windows-x64, local | planted in a worktree, restored and checked by SHA-256 |
| 2026-11-13 | L1 | replacements before retirement: the engine language test now refuses zh-HK and cmn and accepts haw; the account sent to the interface has exactly its seven public fields | windows-x64, local | `cargo test --locked -p onda-asr --lib engine::`; `cargo test --locked -p onda-app --lib api::` |
| 2026-11-13 | L1 probe | accepting `zh-*` and `cmn` fails the engine test ("zh-HK reached the worker"); dropping AccountInfo's camelCase renaming fails the key-set test, while the old string search stays green | windows-x64, local | planted one at a time, run red, restored |
| 2026-11-13 | L2, L6 | dictionary entries: a saved word, the same word in another case and spacing refused, a phrase without a word refused, a vocabulary word with a replacement refused; the mock (Vitest, 20 contract examples) and the real app (00-contract.spec.ts, 21 tests) agree | windows-x64, local | `npm run test:unit`; `npm run build:e2e`; `npm run test:e2e` |
| 2026-11-13 | L2 probe | removing the mock's duplicate check fails "the same word in another case and spacing is refused" | windows-x64, local | planted in `ui/src/mock/backend.ts`, `npx vitest run`, restored |
| 2026-11-13 | L1 | retired after their replacements: `languages.rs` `the_catalog_language_table_matches_whisper_order_and_excludes_cantonese` (now `engine.rs` `languages_the_models_lack_never_reach_the_worker`) and `api.rs` `public_account_dto_contains_no_credentials` (now `the_account_sent_to_the_interface_has_only_its_public_fields`); both replacements pass | windows-x64, local | `cargo test --locked -p onda-asr -p onda-app --lib` |

## Unverified

- No GitHub Actions run has passed yet. The first (run 43344083850) failed on Linux before any
  test: `scripts/prepare.sh` was committed without its executable bit.
- Linux: nothing run since 2026-10-29 (VPS, `docs/testing.md`), including the Linux-only tests.
- macOS: no code yet; nothing built.
- Windows arm64 and Linux arm64: nothing built.
- L5 tier 3: no pixel baselines until the snapshots workflow runs on the runners. L6: the WebView2
  smoke (`scripts/smoke.ps1`) has not run since 0.1.x.
- L7: the NSIS install test has not passed in CI yet; the Linux deb and AppImage test
  (`scripts/installer-test.sh`) is written but runs first in the installers workflow on `master`.
- Real-model golden tests: not run since 2026-10-29.
- Dictation into real applications, physical microphones and real provider accounts on any OS:
  manual matrix, not run for 0.2.0.
- Linux only, written on Windows and first run by Linux CI: the strengthened `/proc` identity test, the Linux day key, and the keyring seal framing test (checked only in an extracted copy with an in-memory keyring).
- Observed once, not reproduced: in 1 of 12 local real-shell runs (the first after a rebuild), the
  second spec's session failed to start and the restart spec then failed. 8 further runs were green.
  If CI repeats it, it is quarantined with an issue (TESTING.md, flaky tests).
