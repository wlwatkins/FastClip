# Conventions

## Writing

The binding style rule is in `CLAUDE.md` at the repository root: precise and
direct, not terse and not padded. It applies to every file here and to every
message an agent returns.

British spelling in user-facing strings, identifiers and documentation. The
code already uses `colour`. The [contract](../architecture/contract.md) is the
authority on field names.

## Commits

Conventional commits, scoped to the owning agent's area:

```text
feat(frontend): drag-to-reorder clip list
fix(backend): write store atomically via temp file and rename
docs(contract): close identity-minting question
chore(ci): cache cargo registry between runs
test(backend): cover truncated database file on load
```

One logical change per commit. A commit touching both `src/` and `src-tauri/`
means something bypassed the contract.

## Documentation

- This book is the specification. Code comments explain why, never what.
- A behaviour change updates the relevant page in the same commit.
- New ADRs are numbered sequentially and added to
  [the index](../architecture/adr/index.md) and `SUMMARY.md`.
- `mdbook build docs` must succeed. A broken link fails the build.

## Code

- No `console.log` or `println!` in landed code. Use a logging facility with
  levels. A clip's `value` never enters a log line
  ([ADR-0002](../architecture/adr/0002-threat-model.md)).
- No `unwrap()` or `expect()` on any path reachable after startup.
- No unchecked casts across the IPC boundary. Validate the shape and fail
  loudly.
- Dead code does not land. If you compute a value, use it.
- List keys are UUIDs, never array indices.

## Tests

- Show the test failing before it passes.
- No arbitrary `setTimeout` waits. Wait on conditions.
- Tests never touch the real `%LOCALAPPDATA%` store.
- Coverage is a diagnostic, not a target.

## Versioning

One source of truth, enforced in CI. `Cargo.toml`, `package.json` and
`tauri.conf.json` currently disagree, and `0.1.0-1` is not valid semver.
`devops` owns the fix.
