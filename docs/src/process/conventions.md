# Conventions

## Writing

The binding style rule is in `CLAUDE.md` at the repository root: precise and
direct, not terse and not padded. It applies to every file here and to every
message an agent returns.

British spelling in user-facing strings, identifiers and documentation. The
code already uses `colour`. The [contract](../architecture/contract.md) is the
authority on field names.

## Branches

| Branch | Role |
| ------ | ---- |
| `refactor` | The working branch for this rewrite. All work happens here. |
| `main` | The destination, once the rewrite is finished. |

**Never switch branches while an agent is running.** A checkout landing
mid-review caused a `critic` run to read two different revisions of the tree.
It detected the change and refused to blend them, which is the behaviour we
want — but it wasted a ten-minute review.

**`main` is not a reference.** It contains divergent work that this refactor
does not know about — a `tray.rs`, a `password.rs`, React 19 — and unresolved
`<<<<<<<` conflict markers in `Cargo.toml`, so it does not build. Do not read
it to learn how a feature "should" work. The [specification](../product/spec.md)
is the authority, not any branch.

If `git status` shows a branch other than `refactor`, stop and report rather
than continuing.

### Open: the endgame merge

`main` is not a clean destination. Its `tray.rs` and `password.rs` overlap with
[WP-08](../work/wp-08-tray.md) and [WP-07](../work/wp-07-encryption.md), which
build those features from scratch. Someone must decide, **before
[WP-12](../work/wp-12-release.md)**, whether the finished `refactor` replaces
`main` outright or is merged into it. Deciding at merge time means resolving
conflicts between two implementations of the same feature under pressure.

This is an owner decision, not an architect one.

## One fact, one home

**An invariant binding more than one module is written in the book, and the code
points at it.** Not restated in a doc comment, not repeated in a second module,
not spelled twice in agreement.

This is the most common defect in the project — eight instances, catalogued in
[recurring defect classes](../reference/defect-classes.md). Every one of them was
correct on the day it was written and invisible to `cargo test`, `clippy`, `tsc`
and `vitest`, because both copies agreed.

When a fact needs to appear in a second place, take the first of these that
applies:

| Remedy | Example |
| ------ | ------- |
| Derive the second from the first | `command_handler!` — one handler list, expanded in both `lib.rs` and `tests/ipc.rs` |
| Make a test fail on divergence | `CASES.len() == 16` in `tests/ipc.rs`, which forces a row for any new command |
| Delete the second and leave a pointer | The mutex acquisition order, which lives in [storage](../architecture/storage.md) and is named — not restated — from `store.rs` |
| Record the duplication as accepted, with the reason | The fallback. It preserves the fault, but an inherited surprise is cheaper than an unrecorded one |

The rule binds the **rule**, not the reasoning that justifies it. The race
analysis for `Store::begin_conversion` stays beside that function; the book links
to it rather than copying it.

## Commits

**No agent commits on this project.** The owner writes every commit. The block is
enforced in four layers — `.claude/settings.json` deny rules for both `Bash(git …)`
and `PowerShell(git …)`, a `PreToolUse` hook, and `.githooks/pre-commit` and
`pre-push` keyed on the Claude environment variables with `core.hooksPath` set.
Three of those four were inert on 2026-07-29 and the policy was breached three
times before it was found; do not assume a layer works because it exists.

The owner's own commits are unaffected: the hooks detect the agent shell and
allow everything else.

Conventional commits, scoped to the owning agent's area:

```text
feat(frontend): drag-to-reorder clip list
fix(backend): apply key pragma before journal_mode on every connection
docs(contract): close identity-minting question
chore(ci): cache cargo registry between runs
test(backend): cover corrupt database and unknown schema version
```

One logical change per commit. A commit touching both `src/` and `src-tauri/`
means something bypassed the contract.

## Documentation

- This book is the specification. Code comments explain why, never what.
- A behaviour change updates the relevant page in the same commit.
- New ADRs are numbered sequentially and added to
  [the index](../architecture/adr/index.md) and `SUMMARY.md`.
- `mdbook build docs` must succeed. A broken link fails the build.

### Cross-references are links

Every reference to another part of the book is a hyperlink, including section
references. `spec §4.3` is a dead end; `[spec §4.3](../product/spec.md#43-reorder)`
is not. Keep the `§N` inside the link text so the reader can see what they are
about to open.

Anchors follow mdBook's slug rule: lowercase the heading, drop punctuation,
replace spaces with hyphens. `## 4.3 Reorder` becomes `#43-reorder`. Avoid em
dashes in headings you intend to link to — they leave a double hyphen in the
anchor.

**mdBook does not verify anchors.** A link to a heading that does not exist
builds cleanly and fails silently for the reader. Check them by building and
grepping the generated `id="..."` attributes in `docs/book/`.

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
- Tests never touch the real `~/.fast-clip/` store.
- Coverage is a diagnostic, not a target.

## Versioning

One source of truth. `src-tauri/Cargo.toml` holds the version; `package.json`
must match it, and `tauri.conf.json` carries no `version` key so Tauri inherits.
`scripts/check-version.mjs` fails on any divergence, including a re-added
`tauri.conf.json` version.

**The gate is no longer enforced automatically.** The project has no GitHub
Actions credits, so `.github/workflows/ci.yml` is `workflow_dispatch:` only and
nothing runs `check-version.mjs` unless a person does. Run it locally before a
release, along with the four commands in `CLAUDE.md`. This is the third instance
of "the check exists but nothing invokes it" in this project — the other two were
the git hooks and the `import.field` truncation rule.

The gate accepts any valid semver, prereleases included. `0.1.0-1` is valid
semver and would pass it. If a prerelease is in fact unacceptable — the Tauri
bundler is the likely reason, and no page records one — write the reason down
before tightening the gate, because the gate is currently right and the
constraint is not written anywhere.
