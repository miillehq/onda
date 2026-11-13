# Test audit: Onda

Date 2026-11-12 · Keel audit · scope: audit, widened to retrofit on 2026-11-13 ([PLAN.md](../PLAN.md))
Inventory: 227 files, about 37,200 source lines; Tauri 2, Preact, Vite, a Rust workspace of six crates;
191 test functions in 42 files, all Rust; CI: `ci.yml` (windows-2022, ubuntu-24.04) and `release.yml`
(windows-2022); runs: none, because the repository had no remote (published as `miillehq/onda` on 2026-11-13).

## Summary
The Rust side is tested well: 191 tests, most of them asserting real behaviour, with strong coverage of shortcut matching, text validation, dictionary matching, OAuth, model downloads and migrations, and all 166 that run on Windows pass today.
Nothing tests the TypeScript UI below the in-app smoke run, the mock the UI is built against has drifted from the Rust defaults, and 11 of 17 planted bugs got past the suite, mostly in pruning, state and platform glue.
The change that would help most is a check runner that actually executes what `ci.yml` describes, because neither workflow has ever run.

## Levels
| Level | Today | Command | Needed |
|---|---|---|---|
| L0 lint/types | yes; green on Windows today | `cargo fmt --all --check`, `cargo clippy ... -D warnings` (×2), `tsc` in `npm run build`, `cargo deny`, `npm audit` | keep; run it on every change |
| L1 unit | Rust only, about 125 tests | `cargo test --workspace` | add Vitest for `ui/src/lib` (33 exported functions, no tests) |
| L2 contract | types only | `npm run contracts:check` (generated `ApiRequest`/`ApiResponse`, green today) | add behaviour: the hand-written mock has drifted |
| L3 integration | about 60 tests: SQLite files, localhost mock HTTP, DPAPI, file locks, child processes; 3 real-model tests | `cargo test --workspace`; golden needs `-- --ignored` | keep; un-gate the OAuth tests on Linux |
| L4 UI, L5 visual | none; a labelled mock exists (`ui/src/mock/backend.ts`) | | add Playwright against the mock, with visual tiers 1 and 2 |
| L6 real shell | partial: `ui/src/smoke.ts`, 41 in-app checks through real IPC | Linux CI step under Xvfb; Windows `scripts/smoke.ps1`, local only | run it on Windows too |
| L7 installers, L8 distribution | Windows silent install/uninstall in `ci.yml`; `scripts/installer-smoke.ps1` local; manual matrix in `docs/testing.md` | | the Linux .deb is never installed by a script; keep the matrix as the release checklist |

## Critical journeys
| Journey | Covered today | Should be covered at |
|---|---|---|
| Hold the shortcut, speak, release: text typed into the focused field (Windows hook; Linux portal and clipboard paste) | L1 matcher, `linux_portal`, audio buffer, state; L3 golden ASR; Windows insertion only by an ignored desktop test; L6 smoke on Linux without dictation | L1, L3, L6 on both OSes, manual matrix |
| AI polish with a chosen provider; a failure keeps the original text | L1 text validation, L3 localhost mocks (only 401 asserted), quality fixtures | L1, L2 error mapping, L3, L4 error display |
| ChatGPT sign-in, refresh, sign-out | L3, Windows only | L3 on both OSes |
| Download, verify and select a model | L3 resume and checksum; the checksum check before use is untested (A1) | L3 |
| Dictionary, snippets and settings survive a restart | L1 dictionary, L3 persistence, L6 smoke saves | L2 mock defaults, L3, L4 |
| History kept N days and encrypted; stats and streaks | L3 persistence and DPAPI; row cap and day prune unguarded (K4, K5) | L3 |
| Install, upgrade, uninstall | L7 Windows (CI step never run, local smoke passed for 0.1.0); L3 migrations | L7 on both OSes |

## Per-file classification
Files whose tests are all kept are grouped per crate; every other file has its own row.

| File | Tests | Level | keep / strengthen / move / retire |
|---|---|---|---|
| onda-core: `text.rs` 14, `stats.rs` 6, `state.rs` 4, `learning.rs` 2 | 26 | L1 | 26 / 0 / 0 / 0 |
| `crates/onda-core/src/dictionary.rs` | 11 | L1 | 10 / 1 / 0 / 0 |
| `crates/onda-core/src/persistence.rs` | 10 | L3 (SQLite) | 7 / 3 / 0 / 0 |
| `crates/onda-core/src/profile.rs` | 5 | L1 | 3 / 2 / 0 / 0 |
| `crates/onda-core/src/transforms.rs` | 1 | L1 | 0 / 1 / 0 / 0 |
| src-tauri: `runtime.rs` 8, `errors.rs` 4, `runtime/profile.rs` 2, `runtime/commands.rs` 2 (one per OS) | 16 | L1, one L3 | 16 / 0 / 0 / 0 |
| `src-tauri/src/runtime/providers.rs` | 3 | L1 | 1 / 2 / 0 / 0 |
| `src-tauri/src/api.rs` | 2 | L1; one L0 in disguise | 1 / 0 / 0 / 1 |
| onda-cloud: `providers/tests.rs` 14 (1 ignored), `oauth/availability_tests.rs` 4, `keys.rs` 3 (1 ignored), `storage.rs` 1, `responses.rs` 5, `sse.rs` 3 | 30 | L3 localhost and DPAPI; `responses`, `sse` L1 | 30 / 0 / 0 / 0 |
| `crates/onda-cloud/src/oauth/tests.rs` | 13 | L3, 12 Windows-only | 12 / 0 / 1 / 0 |
| onda-asr: `models.rs` 8, `tests/golden.rs` 3 (ignored), `tests/gpu_golden.rs` 1 (ignored), `engine.rs` 2, `manifest.rs` 1 | 15 | L3 models and real model; L1 engine, manifest | 15 / 0 / 0 / 0 |
| `crates/onda-asr/src/hardware.rs` | 4 | L1; one L3 Linux | 2 / 2 / 0 / 0 |
| onda-asr: `worker_job.rs` 1 (L3, ignored), `bin/worker.rs` 1 (L1, never built), `languages.rs` 1 (L0 in disguise) | 3 | as listed | 0 / 0 / 2 / 1 |
| onda-platform: `target.rs` 3, `processes.rs` 2, `desktop_tests.rs` 1 (ignored) | 6 | L1; L3 real desktop and Linux `/proc` | 6 / 0 / 0 / 0 |
| `crates/onda-platform/src/audio.rs` | 14 | L1 | 13 / 1 / 0 / 0 |
| `crates/onda-platform/src/desktop.rs` | 4 | L1; one L3 Linux | 3 / 1 / 0 / 0 |
| `crates/onda-platform/src/linux_target.rs` | 3 | L1; one L3 Linux | 2 / 1 / 0 / 0 |
| `crates/onda-platform/src/clipboard.rs` | 3 | L1, Windows | 2 / 1 / 0 / 0 |
| `crates/onda-platform/src/protect.rs` | 2 | L3 DPAPI; L3 keyring, ignored | 1 / 0 / 1 / 0 |
| onda-input: `matcher.rs` 10, `linux_portal.rs` 8 (Linux), `windows_hook.rs` 1 (L3 Win32) | 19 | L1, one L3 | 19 / 0 / 0 / 0 |
| `crates/onda-input/src/supervisor.rs` | 1 | L1 | 0 / 1 / 0 / 0 |

Totals: keep 169, strengthen 16, move 4, retire 2. Quarantine: none, because there is no CI history.

## Verdicts
### strengthen
| File and test | Guards | Reason |
|---|---|---|
| `dictionary.rs` `prompt_terms_respect_the_prompt_budget` | the speech prompt's size | every fixture term is 9 characters, so the character-budget assert cannot fail |
| `persistence.rs` `normalized_duplicates_fail_and_invalid_import_is_atomic` | a failed import adds nothing | the bad import fails while parsing JSON, before any write; mix valid and invalid entries |
| `persistence.rs` `lifetime_totals_start_on_the_first_dictation_and_outlive_pruned_days` | the 400-day prune keeps the newest days | counts rows only; probe K4 passes |
| `persistence.rs` `newer_schema_is_not_destroyed` | a newer database is refused and left as is | `is_err()` only; an empty v99 database errors anyway |
| `profile.rs` `pictures_are_cropped_resized_and_reencoded_as_png` | centre crop of the profile picture | symmetric fixture; a left-edge crop passes |
| `profile.rs` `unsupported_empty_oversized_and_corrupt_files_are_refused` | 10 MB and empty-file refusal | zero-filled inputs fail as unreadable even without the size checks |
| `transforms.rs` `unicode_diff_reconstructs_both_versions` | the rewrite diff view | only rebuilds both texts; `[Removed(all), Added(all)]` passes |
| `runtime/providers.rs` `each_provider_has_its_own_route_and_key_slot` | a key never goes to the wrong provider | checks 2 of 4 slots; a shared local/OpenAI slot passes |
| `runtime/providers.rs` `a_missing_model_or_address_asks_for_it` | the missing-model message | every model field is `None`, so swapped arms pass |
| `hardware.rs` `linux_reports_installed_and_available_memory` | RAM-based profile choice on Linux | `> 0` passes if the kB-to-bytes factor is dropped |
| `hardware.rs` `optional_acceleration_never_overrides_cpu_only` | CPU-only never probes the GPU | the fixture GPU has 0 VRAM; probe A3 passes |
| `audio.rs` `silence_and_clipping_diagnostics_are_content_free` | "no speech" detection | tests `diagnostics()`, not `RecordingBuffer::finish`, which production uses; probe P3 passes |
| `desktop.rs` `local_date_is_a_day_key` | the daily usage key | checks the shape only; "0126-10-12" passes |
| `linux_target.rs` `a_process_is_identified_by_its_start_time_and_name` | PID-reuse identity on Linux | `> 0` passes for the wrong `/proc` field |
| `clipboard.rs` `shortcut_releases_are_in_reverse_order` | the paste chord leaves no key held | checks `wVk` only; dropping `KEYEVENTF_KEYUP` passes |
| `supervisor.rs` `rejects_oversized_and_unknown_messages` | the 16 KiB helper line cap | `is_err()` only; passes without the cap (EOF error) |

### move and retire
| Verdict | File and test | Guards | Reason |
|---|---|---|---|
| move | `oauth/tests.rs` `callback_rejects_csrf_client_swaps_duplicates_host_and_missing_registration` | OAuth state and CSRF checks | pure `parse_callback`, but `cfg(windows)`; Linux never runs it |
| move | `worker_job.rs` `closing_job_kills_owned_worker_even_with_stdin_open` | no orphaned ASR workers | ignored and never run; any stdin-blocking system process would do |
| move | `bin/worker.rs` `short_clips_encode_only_their_own_length` | audio context sizing | in a `required-features` binary that `cargo test --workspace` never builds; move `audio_context` into the library |
| move | `protect.rs` `keyring_sealed_bytes_round_trip_and_stay_apart_by_purpose` | Linux at-rest encryption of history | seal/open are pure but reachable only through the real keyring (the production entry); inject a key at L1 |
| retire | `languages.rs` `the_catalog_language_table_matches_whisper_order_and_excludes_cantonese` | nothing beyond `engine.rs` `languages_the_models_lack_never_reach_the_worker` | asserts a constant's order that no code relies on |
| retire | `api.rs` `public_account_dto_contains_no_credentials` | nothing the compiler does not | searches for `access_token` in camelCase JSON, so it cannot fail |

## Probes
Planted one at a time in a copy of `HEAD` on Windows, running `cargo test -p <crate>`. The suite caught the six controls: A2 (checksum compare inverted), C3 (any finish reason is success), K1 (shorter phrase wins), K2 (every dictation adds an active day), P2 (password refused only when both snapshots agree) and I2 (bare Esc ignored while the chord is held). Missed:

| # | Planted bug | Effect for a user |
|---|---|---|
| A1 | `models.rs:146` checksum not verified before use | a corrupted model is loaded |
| A3 | `hardware.rs:107` CPU-only preference ignored | the GPU is used against the setting |
| C1 | `oauth.rs:284` nonce sent as the PKCE `code_verifier` | sign-in fails at the real server; the mock never checks it |
| C2 | `sse.rs:74` event size counter not reset | long cleanups fail at 256 KiB per stream |
| K3 | `text.rs:8` length ratio 0.50 to 0.05 | polish may drop most of the text (`npm run quality` catches it, `cargo test` does not) |
| K4 | `persistence.rs:323` prune keeps the oldest days | after 400 days each new day is deleted on write |
| K5 | `persistence.rs:350` history cap keeps the oldest rows | new dictations vanish once 2,000 are stored |
| K6 | `state.rs:94` finished operations still match their ID | a late result can change a finished operation |
| P1 | `clipboard.rs:488` owner check inverted | the old clipboard is pasted instead of the dictation (only an ignored test covers it) |
| P3 | `audio.rs:421` silence threshold inverted | every recording reports no speech |
| I1 | `matcher.rs:159` auto-repeat not filtered | a held key's repeat starts dictation |

## CI findings
- The repository has no git remote, so `ci.yml` and `release.yml` have never run; `docs/testing.md` says the same. Every CI step is a definition, not evidence. What it describes is thorough. Windows: fmt, contracts, tsc/Vite, clippy ×2, tests, quality, 3 golden real-model tests, cargo deny, npm audit, an NSIS build, silent install/uninstall. Linux: the same L0 to L3 plus WebKitGTK smoke under Xvfb.
- Never run by any workflow: 6 ignored tests (worker job, Vulkan golden, live local LLM, Secret Service keys, Linux keyring seal, real Win32 paste), the worker-binary test, `scripts/smoke.ps1` (WebView2) and `scripts/installer-smoke.ps1`.
- Linux has no OAuth or account-storage coverage: 18 onda-cloud tests are `cfg(windows)`, though the callback, JWT and pre-cancel tests need no DPAPI.
- Run today on Windows with a warm cache: fmt 1 s, clippy 33 s, tests 42 s (166 passed, 7 ignored), contracts 6 s, quality 34 s, `npm run build` 3 s. All green; L0 to L3 fits the 3-minute budget.
- Timing risks found by reading: a bind-drop-reuse port race in `nothing_listening_locally_is_reported_as_unreachable` and a 2 s deadline in `cancellation_closes_a_live_stream_after_first_delta`.

## Drift
- `ui/src/mock/backend.ts:23` starts from settings that differ from `Settings::default()` (`crates/onda-core/src/contracts.rs:131`): language `"en"` vs auto, and learning, hands-free toggle, launch at login and polish all on vs off. `?mock=onboarding`, which stands for a fresh install, inherits all five. Already drifted.
- The name limit in the mock counts code points (`backend.ts:472`); Rust counts graphemes (`profile.rs:27`). The app accepts 40 flag emoji and the mock refuses them. Already drifted.
- The mock's `updateSettings` (`backend.ts:387`) skips `Settings::validate`, so invalid-settings errors never appear against it. Its `respond()` ends in `default: return { type: "unit" }` (`:539`), so a new command silently succeeds in the mock.
- No type drift: one Tauri command (`application_call`) takes a generated `ApiRequest` union, and `contracts:check` passes.

## Remediation slices (superseded on 2026-11-13 by [PLAN.md](../PLAN.md), run through GitHub Actions)
| # | Slice | Done when | Cost |
|---|---|---|---|
| 1 | Local check runner: `npm run check` (Windows) and `scripts/check.sh` (WSL) run the `ci.yml` L0 to L3 steps; each run appends a `VERIFICATION.md` row | green on both, two rows recorded | an hour |
| 2 | Tests for the 11 missed probes (new tests, or strengthened ones where a test exists), one PR per crate | each new test seen red with its probe, then green | a day |
| 3 | Strengthen the remaining weak tests above (planted bugs: left-edge crop, dropped KEYUP flag, missing size cap) | each seen red | a day |
| 4 | The four moves: un-gate the OAuth callback/JWT tests, move `audio_context` into the lib, use a system process for the worker job, inject a key for keyring seal | the four run in `cargo test --workspace` on both OSes | an hour |
| 5 | Vitest for `ui/src/lib` (diff, format, keys, router, activity) with one passing example first | in `npm run check`; each test seen red | an hour |
| 6 | L2 behaviour contract: the same requests against the mock and the real dispatcher, comparing defaults and validation errors | the three drifted pairs each caught red, then fixed | a day |
| 7 | L4/L5: Playwright on the labelled mock for onboarding, settings, dictionary and polish errors; ARIA and geometry baselines | green on Windows and WSL for a week | a day |
| 8 | L6 on Windows: `scripts/smoke.ps1` in the Windows check | 41 checks green in WebView2 | minutes |
| 9 | Retire `languages.rs`'s constant test (replaced by the engine language test plus zh-HK and cmn) and `public_account_dto_contains_no_credentials` (replaced by an exact key-set test through `From<AccountProfile>`) | after slices 3 and 4 land | minutes |

## Rulings
- Ruling: journeys taken from the README and the manual matrix in `docs/testing.md`. Why: not yet asked. Cost if wrong: slices 6 and 7 aim at the wrong flows.
- Ruling (superseded 2026-11-13, the repository is now public with Actions): slice 1 is a local runner, and the existing workflows are left as they are. Why: the project had no remote. Cost if wrong: none.
- Ruling: tier 1 platforms are windows-x64 and linux-x64, with no macOS. Why: the README names Windows as primary and Linux as supported. Cost if wrong: macOS work is unplanned.
- Ruling: verdicts come from three parallel reviewers who read every test and the code it calls, and all 17 probe predictions matched. Why: 191 tests. Cost if wrong: a misjudged keep.
- Ruling: tests against SQLite files, localhost servers, DPAPI, file locks and child processes count as L3; `smoke.ts` counts as partial L6 (real IPC, no external driver); the profile is `tauri-web` although the UI is Preact.

## Not verified
- Nothing was run on Linux today; per `docs/testing.md`, the Linux-gated tests last ran on 29 October 2026 on a VPS. Probes ran on Windows only.
- Not run: the ignored tests (golden needs the 77.7 MB model; Vulkan, desktop insertion, worker job, live LLM), `scripts/smoke.ps1`, `scripts/installer-smoke.ps1` and the NSIS build.
- There is no CI history, so flakiness was judged by reading the code only.
- The probe copy shared `target/`; its outputs were relinked from the real sources afterwards, and `npm run quality` passes.
