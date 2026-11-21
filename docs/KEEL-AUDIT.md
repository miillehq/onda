# Onda Keel audit, November 2026

The independent release review ran against the default branch at `522b528` and again against the first integrated product candidate at `3938d04`. The second review restored all 20,000 supported dictionary entries into a clean database, exercised the real editors and ran the Windows speech worker. Its 20 fixed probes produced 16 passes, three failures and one inapplicable case. Two failures exposed implementation defects and one reflected Onda’s documented duplicate-trigger rule. The follow-up changes and their tests are listed below.

An independent rerun on `c24fa12` found 18 passes, the documented duplicate-trigger difference and one inapplicable probe. It also found three cases where the source checker assigned the wrong owner or counted test-only code. The gate now checks qualified implementations separately from homonymous types and limits production roots and members to the shipped configurations. Each checker defect has a regression fixture.

## Fixed release probes

| # | Probe | Observed behavior and disposition |
|---:|---|---|
| 1 | Desktop arguments | All 69 native request variants and the development mock reject five malformed argument classes. Real webviews return the application error for representative malformed envelopes. The complete-settings validator also protects nested persistent data. See `src-tauri/tests/request_shapes.rs` and `tests/e2e/00-contract.spec.ts`. |
| 2 | Missing production bridge | The built main renderer shows its recovery action in Chromium and WebKit. The mock badge is absent. The asset check excludes the smoke fixture and development data. |
| 3 | Shipped content policy | The distributed CSP contains only the app’s required origins. Development allowances have a separate policy. A harmless one-pixel data image is blocked by the real webview. |
| 4 | Navigation | Both windows use the same application-origin policy. External HTTPS new-window requests are delegated to the system browser. The decision table is tested in `src-tauri/src/navigation.rs`. |
| 5 | Test hooks | Source and production assets were inspected. Profile overrides require a debug build, WebDriver requires the E2E feature, and Vite removes the smoke runner from distributed assets. |
| 6 | Invalid UTF-8 | A damaged SQLite payload returned a storage error and kept its bytes. The dictionary file picker now reports invalid UTF-8 before import. |
| 7 | A newer database | A schema-99 database in DELETE journal mode stayed byte-identical when a schema-6 build refused it. `crates/onda-core/tests/profile_storage.rs` verifies both bytes and mode. |
| 8 | Export to app data | The stats writer refused an `onda.db` destination and retained its contents. The writer uses the exact user-selected filename. |
| 9 | Locked startup | An exclusively opened Windows fixture produced a storage error. Reopening it after releasing the lock recovered its original entry. |
| 10 | Interrupted save | A rejected PNG replacement preserves its destination. Review found a remaining in-place Linux permission-token write. The replacement uses a private file in the same directory. An injected partial-write failure now leaves the previous token intact. |
| 11 | Concurrent instances | Source inspection found the production single-instance guard and file locks for model downloads and account updates. Concurrent synthetic account clients also share a refresh lock. |
| 12 | Malformed numbers and dates | Invalid durations and malformed calendar dates are rejected, including February 30. The calendar test also covers out-of-order valid dates. |
| 13 | Record width and location | Extra or missing fields in a dictionary entry give the source location and preserve the existing dictionary. |
| 14 | Missing or unexpected header | A bare array, absent version and unsupported version are refused. The first record stays a record. |
| 15 | Duplicate records | **Product-specific exception.** Keel’s generic probe keeps duplicates within one input. Onda permits one rule per normalized trigger. An archive with `Alpha` and `ALPHA` inserts one rule and importing it again inserts zero. This behavior keeps dictation deterministic. |
| 16 | Round trip | Quotes, delimiters, Unicode and embedded newlines survive export and import. A backup containing 20,000 valid entries restored completely. |
| 17 | Rejected batch | A 100,000-row invalid input reports the number of rejected records. Import commits atomically and the original data survives. |
| 18 | Negative monetary entries | Inapplicable. Onda records nonnegative dictation counters and has no financial ledger, refunds or reversal entry surface. |
| 19 | Same rules at each entry point | Direct and imported dictionary entries share normalization, length and uniqueness checks. The native and mock contract examples exercise both. |
| 20 | Untouched fields | The candidate preserved dictionary text but trimmed snippet triggers and rewrite instructions. The corrected editors change only the edited value. Both browser engines exercise multiline instructions and surrounding spaces. |

## Independent free read

| Affected behavior | Correction and evidence |
|---|---|
| History on a locked keyring, retention changes and racing clears | Unreadable rows remain encrypted on disk. A revision check prevents an older write from restoring a cleared entry. Retention and deletion survive a failed transaction. [Data protection changes](https://github.com/miillehq/onda/pull/42) |
| Native selection and clipboard handling | A direct Linux replacement refuses to delete the source selection before an independent insertion. A same-length replacement checks the affected range. Borrowed clipboards retain ownership and rich data. A cancelled queued handoff cannot write. [Delivery changes](https://github.com/miillehq/onda/pull/46) |
| Unconfirmed pastes and permission persistence | Linux and macOS require observable field state before sending a paste shortcut. Linux persists the next portal grant by atomic replacement. [Follow-up](https://github.com/miillehq/onda/pull/51) |
| Delivered counts, streaks and retry outcomes | One receipt credits a successful dictation across recovery paths. Spoken edits receive no credit. Real SQLite tests join late dates, preserve lifetime streaks beyond 400 daily rows, and reject impossible dates. Completed unchanged previews remain in History. Delayed learning reads respect the absolute deadline. [Outcome changes](https://github.com/miillehq/onda/pull/47) |
| Worker selection and spacing | CPU-only disables Metal even in a Metal-capable worker. Real Mac workers exercised the pinned speech fixture on both architectures. East Asian punctuation joins without a spurious space. [Speech changes](https://github.com/miillehq/onda/pull/45) |
| Stats sharing and editor fidelity | Shared PNGs encode 1200×630. Privacy selections apply when saving. Rewrite-specific model choices and lossless dictionary edits are covered in the browser. [Editor changes](https://github.com/miillehq/onda/pull/44) |
| Model acquisition and native credentials | Clicking the already-selected missing Fast model starts acquisition. Disabling an obsolete OS guard allowed real macOS account construction and Keychain-backed synthetic sign-in, refresh and sign-out. [Shipped verification](https://github.com/miillehq/onda/pull/50) |
| Default rewrite, retained destination and Unicode names | Deletion repairs the default atomically and startup repairs older profiles. Retry retains provider, server, account and resolved model. The General editor accepts forty flag graphemes. A controlled restart and browser regressions cover the affected paths. [Rewrite changes](https://github.com/miillehq/onda/pull/52) |

## Product promises

| Commitment | Evidence |
|---|---|
| Single operation, 180-second default and 600-second maximum | Core state-machine, settings, shortcut matcher and bounded native audio cases. The native shell checks missing models before it opens the microphone. |
| Local ASR, CPU-only and original text on failure | Real workers transcribe a pinned public-domain clip, exercise accelerator recovery and cancellation. Provider adapters use synthetic local HTTP. The native preview checks retention of source text on failure. |
| Two characters of caret context, a short hold and bounded input | Core text and audio cases cover opening and closing punctuation, Unicode, short taps and over-limit source text. OS focus, driver loss and hot-plug have separate manual checks. |
| One provider and a three-second focus-return opportunity | The dispatcher selects one provider. A changed retry destination is refused through IPC. Native completion revalidates the original target after the focus delay. Live-provider authentication and real application latency belong to the manual release matrix. |
| Learning disabled by default, three samples within twelve seconds | Default contracts and `src-tauri/src/runtime/learning.rs` check the observation cap, absolute deadline and source eligibility. |
| Forty-grapheme names, ten-mebibyte images, 256-pixel avatar | Native profile tests exercise both sides of the size limits. The General editor accepts forty flags through real browser text input. |
| Delivered-only usage, 400 detailed days and one-minute pace threshold | `usage_calendar.rs`, the SQLite schema migration, the dictation receipt tests and the core summary cases retain these values across restarts and late dates. |
| 1200×630 share image, identity options off | The real canvas encoder and the share-dialog tests check dimensions and selected privacy options. |
| Three-day default history and session-only at zero | Native settings and retention tests cover cutoff, idle reads, locked credentials, stale writes and the zero-duration transaction. |

Installed artifacts at `5ca06cd` passed six candidate install cycles, six public-to-candidate upgrades with a seeded multiline snippet, and Debian, Fedora and Arch GUI launches. That run used two 0.3.0 builds with different binaries and database schemas. The ledger and PR checks identify subsequent revisions. [Installer evidence](https://github.com/miillehq/onda/actions/runs/44593069718)

The original structural review found 24 oversized files and 42 oversized functions or callbacks. The original `Runtime` measured 3,566 lines and `ChatGptService` measured 728. The measured source limits and compatibility exceptions are recorded in [ADR 008](adr/008-source-budgets.md). Physical devices, third-party target applications, live accounts, platform signing and assistive-technology sessions retain their own entries in the [manual release matrix](testing.md).

The final inventory extended the Rust and TypeScript measurement to the shipped stylesheets, Vite HTML entries and compiled SQL migrations. It found three further files above 400 code lines. Controls, shell and page styles are now ordered feature imports. Before and after ordinary production builds on native Windows emitted 19 files with identical names and SHA-256 digests. The complete product source check covers 219 files.

An independent check at `5a2d25b` observed all 30 first-party CSS inputs in the production build and confirmed the 19 artifact digests. Its two additional checker fixtures are now enforced. A development-only stylesheet stays outside Vite’s emitted modules, and comment-only HTML and SQL lines stay outside the code budget.

The full Cargo audit also covered conditional packages outside desktop builds. It found a certificate-list advisory in the locked WebAssembly dependency and a patchable `rustix` soundness warning. Updating `webpki-root-certs` to 1.0.12 and `rustix` to 1.1.6 cleared both. The remaining GTK 3 warning and maintenance notices are recorded in the [dependency policy](third-party-licenses.md).
