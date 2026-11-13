@AGENTS.md

## Claude Code notes

- If the Keel skills are installed, use `keel-build` for any change and `keel-test` when adding
  or reviewing tests; otherwise follow the rules in `AGENTS.md`, which say the same.
- Interface work: read `DESIGN.md` and `docs/frontend-handoff.md` first, and preview against the
  labelled mock with `npm run dev`.
- On Windows, run cargo from PowerShell after `. ./scripts/toolchain.ps1` (sets `LIBCLANG_PATH`).
