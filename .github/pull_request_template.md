## What changes

<!-- In plain words: what a user or developer will notice. Link the issue or the PLAN.md slice. -->

## Evidence

<!-- "Tested" names the level, the platform and the run (a CI run link or the exact local
     command). Anything less is "built". Levels are in TESTING.md. -->

| Level | Platforms | Run or local command |
|---|---|---|
|  |  |  |

## Failure message of each new or rewritten test

<!-- Break the code the test guards, run it, paste the failure, then restore the code. -->

```
```

## Checklist

- [ ] `VERIFICATION.md` has an appended row for each claim above.
- [ ] No test that this pull request replaces is deleted here; a later one retires it.
- [ ] The contract (`src-tauri/src/api.rs`, `crates/onda-core/src/contracts.rs`,
      `ui/src/generated/`, the mock, `contracts/`) is untouched, or this pull request changes only
      the contract.
- [ ] No dictated text, audio, keys or real tokens in code, logs, fixtures or screenshots.
- [ ] Pixel baselines, if any changed, came from the snapshots workflow.

## Rulings

<!-- Decisions made along the way: "Ruling: <what> — <why> — <cost if wrong>". Delete if none. -->
