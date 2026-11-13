# ADR 005: Encrypted dictation history with user-set retention

Status: accepted. Date: 2026-10-23. Partly supersedes ADR 003 ("no default transcript history").

Keeping only the last result in memory made one failed insertion block the next dictation: the user had to copy or discard it before the shortcut worked again, and an older dictation was gone as soon as a new one started. People need to keep dictating and still find what they said.

Onda keeps a history of user-initiated dictations, voice edits and rewrites (not typed previews, paste retries or undo). Each entry holds the local transcript, the final text, any spoken instruction, whether it was typed, a fallback flag, an error code when it was not typed, and the target executable name. A failed operation no longer blocks new ones; its text is in History instead.

- Retention is a setting, `historyDays`: 0 (kept only until Onda quits, never written to disk) or 1–30 days, default 3.
- On disk each entry is a DPAPI blob (current Windows user, Onda-specific entropy) in SQLite. Plaintext exists only in process memory and the renderer's view. SQLite runs with `secure_delete`.
- Expired entries are removed at startup, after each write and when retention shrinks. Lowering retention to 0 deletes every stored entry. Entries that no longer decrypt are deleted. The table is capped at 2,000 rows.
- Users can delete one entry, clear everything (Privacy or History) or delete the visible result.
- History text never enters logs, diagnostics, metrics or WebView storage. Logs record only that a write failed, with the operation ID.
- Updates still refuse to restart over an untyped, uncopied failed result when history is session-only, because the restart would lose it.

Update (2026-11-13): On Linux and macOS each entry is sealed with ChaCha20-Poly1305 under a random key that the desktop keyring (Secret Service) or the macOS Keychain keeps for the user (`crates/onda-platform/src/protect.rs`). On Linux without a keyring, history stays in memory until Onda quits.
