# WP-02 — Toolchain and CI skeleton

**Objective:** make a broken change impossible to land unnoticed, before there
is any new code to break.

**Depends on:** nothing. Runs in parallel with WP-01.

## Work

### devops

Create the GitHub Actions workflow on a `windows-latest` runner. Rust checks:
`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`. Frontend
checks: typecheck, `svelte-check`, `vitest run`, build. Cache the Cargo
registry and target directory.

Fix versioning: `Cargo.toml` says `0.1.0-1`, `package.json` has no version, and
neither agrees with `tauri.conf.json`. The three disagreeing is the defect —
`0.1.0-1` is itself valid semver. Pick one source of truth and enforce agreement
in CI.

Add `cargo audit` and `npm audit`.

Prove each check can fail before declaring it works.

**Run the SQLCipher spike.** Time-boxed, and it gates the crate choice in
[ADR-0005](../architecture/adr/0005-sqlite-store.md). On `windows-latest` in
CI: open an encrypted database with a key, write a row, close, reopen with the
key, read it back, and confirm reopening with the wrong key fails. Try the
architect's preferred crate first; if it cannot do this in the time box,
`rusqlite` with `bundled-sqlcipher` is the fallback.

Report the build time and binary size impact. A bundled C compile is the
slowest thing in this pipeline and needs caching from the start.

### devops — the log sink

Added after [review 005](../reviews/005-wp-05-crud.md). Install the log sink
[ADR-0012](../architecture/adr/0012-logging.md) specifies: `tauri-plugin-log`,
stdout plus a rotating file in `~/.fast-clip/`, **webview target off**. Pin the
version and record it, as ADR-0005 set the rule.

This is not tidiness. **Four accepted design decisions are load-bearing on the
log existing** — the absorbed `update_clips` emission, post-commit conversion
cleanup, the startup sweep and stray-`keyfile` deletes, and `lock`'s checkpoint
and close. Each absorbs a failure on the stated ground that it is *recorded*
rather than lost. With no sink installed, all four read "absorbed silently",
which is what each was written to prevent. A released FastClip has no console
attached, which is why a file and not stdout alone.

**The content rule is the load-bearing half.** A file sink creates a new file in
`~/.fast-clip/` and therefore a new way to breach spec §8 criterion 6, which no
care in the storage layer prevents. A `log::` call formatting a clip `label` or
`value` is a `BLOCK`-level finding. `log::error!("{error}")` on a `ClipError` is
safe by construction, because no variant carries clip text — a property of that
type worth keeping.

### test-engineer

Install and configure the runners so the workflow has something to call:
Vitest with `@testing-library/svelte` and the Tauri mock API, and a `cargo test`
target. One trivial passing test per side, purely to prove the wiring.

### Others

No work.

## Definition of done

- A pull request with a clippy warning fails CI.
- A pull request with a failing test fails CI.
- The three version declarations agree, and CI fails if they diverge.
- The SQLCipher spike has run, and the crate choice is recorded in
  [ADR-0005](../architecture/adr/0005-sqlite-store.md) with evidence.
- A log sink is installed per [ADR-0012](../architecture/adr/0012-logging.md),
  and a test creates, copies, edits and deletes a clip carrying a distinctive
  value, then greps the log file and stdout for it and finds nothing. Spec §8
  criterion 6 is otherwise tested only by inspection.

## Risks

Rust CI without caching is slow enough that people stop waiting for it, which
turns the gate into decoration. A bundled SQLCipher build makes that worse, so
cache before the spike rather than after.
