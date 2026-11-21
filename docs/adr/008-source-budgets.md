# ADR 008. Source ownership and size limits

Accepted on 2026-11-19 for the desktop Rust workspace and Preact renderer.

The initial audit measured the native `Runtime` at 3,566 lines across its implementations. Moving a method to another file kept its locks and dependencies on the same type. The runtime now composes services that own recording, recognition, delivery, results, retention, accounts, model management and platform shortcuts. A single operation actor admits and cancels work. SQLite continues to own one connection so a cross-table change can commit atomically.

## Enforced limits

| Source unit | Limit | Measurement |
|---|---:|---|
| Production source file | 400 | Nonblank physical source lines containing code |
| Function, closure or callback | 80 | Its body and signature, including nested logic |
| Class, component or Rust struct with its implementations | 400 | All production parts in the crate; splitting an implementation preserves its total |
| Public members | 25 | Declared public fields and callable members |
| Preact view returning JSX | 80 executable lines, 160 including markup | The larger allowance keeps an accessible label and its control together. Embedded expressions and handlers retain the executable-line limit. |
| `Settings` public members | 40 | Thirty deployed settings fields share a flat wire and persistence format. There are 34 measured members. Changing that shape requires a compatibility decision. |
| `Store` public members | 32 | The existing typed facade has 27 members over one SQLite connection. Domain modules borrow it and transactions own all associated writes. |
| `Runtime::dispatch` | 180 lines | One exhaustive table maps the 69 stable IPC alternatives to their owners. A loop, condition or nested match inside any route arm fails the check. |

Rustfmt and Clippy check code compiled for the host. The source scanner follows production library and binary roots across OS configurations, resolves Rust module paths and combines implementations with their declaring type. Test-only methods, test modules, debug-only items, E2E fixtures, examples, benchmarks and build scripts are outside the shipped source measurements. A module referenced solely from any of these paths cannot establish production reachability. The callable native library API includes the documented synchronous clipboard conveniences. Cancellable desktop workflows use the guarded methods. [Windows integration](../windows-integration.md)

Biome checks renderer and tooling source. Knip checks the full tree and the production entry points separately. The production pass exposed test-only exports and duplicate shortcut and calendar formatting. The renderer now uses the same functions the tests exercise. Generated mirrors, fixtures and test bodies have their own compile and test checks.

The file limit also applies to styles, Vite’s configured HTML entries and SQLite migrations. PostCSS parses the same source that Vite bundles. A write-free production build identifies emitted CSS after Vite folds development guards. The check follows its local imports and matches SQL resources to native compile-time includes. HTML and SQL comment-only lines are excluded without treating comment markers inside literals as comments. Feature styles retain their original cascade order through the three style entry files.

`node scripts/quality/architecture.mjs --stats` prints the measurements. An additional wire field, view branch or operation phase faces the same checked limits on the next PR. A higher limit requires an ADR with the compatibility or implementation reason.
