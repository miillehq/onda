# ADR 003: Ephemeral text and bounded correction proposals

Status: accepted; transcript history superseded by ADR 005 (2026-10-23). Date: 2026-10-03.

Dictation and selection text are sensitive. Convenience does not justify default transcript history, continuous UIA reads or learning from arbitrary typing.

Persist only settings/dictionary/snippets/transforms/content-free counters. Keep audio in capture memory/pipes and results in bounded operation memory. Keep tokens in DPAPI protected records, never SQLite or renderer storage. Context is disabled by default and only read from a user-triggered focused operation after security/per-app checks.

Learning is an explicit opt-in and produces proposals. Read at most three bounded focused-field samples in twelve seconds; compare only the uniquely identifiable inserted scope while requiring unchanged surrounding text. Reject ordinary rewrites, protected identifiers, common words and ambiguous matches. The user chooses whether to add a proposed correction. This is a deliberately narrower capability than continuous auto-learning and does not promise support in inaccessible controls.
