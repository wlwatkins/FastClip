# Review log

Every G3 review lands here as `NNN-<topic>.md`, with a row below and a line in
`SUMMARY.md`.

The log is append-only. Superseding a review means writing a new one.

## Index

| # | Topic | Verdict | Findings | Waived |
| - | ----- | ------- | -------- | ------ |
| 001 | Book consistency (pre-G0b) | `REWORK_ARCHITECTURE` | 11 | 0 — all fixed |
| [002](./002-wp-02-toolchain.md) | WP-02 Toolchain and CI skeleton (G3) | `ACCEPT` | 5 minor | 0 — all fixed. G4 condition open, not waived |
| [003](./003-wp-01-contract.md) | WP-01 Contract ratification (G0b) | `ACCEPT` | 4 (1 major, 3 minor) | 0 — all 4 carried to WP-03 and WP-07 |
| [004](./004-wp-10-palette.md) | WP-10 Palette and accessibility (G3) | `ACCEPT` | 4 (1 major, 3 minor) | 0 — all four fixed |
| [005](./005-wp-05-crud.md) | WP-05 Clip list, copy, CRUD (G3) | `REWORK_ARCHITECTURE` → resolved | 5 (1 major, 4 minor) | 0 — all five fixed; ADR-0012 written |
| [006](./006-wp-14-settings.md) | WP-14 Settings, lock state, startup (G3) | `REWORK_IMPLEMENTATION` → resolved | 1 minor | 0 — fixed |
| [007](./007-wp-06-reorder.md) | WP-06 Reordering (G3) | `ACCEPT` | 1 minor | 0 — fixed |
| [008](./008-wp-09-export-import.md) | WP-09 Export and import (G3) | `ACCEPT` | 2 (1 major, 1 minor) | 0 — F1 discharged by re-measurement, F2 to `architect` |
| [009](./009-wp-13-search.md) | WP-13 Search and filter (G3) | `REWORK_TESTS` → resolved | 1 major | 0 — fixed |
| [010](./010-wp-07-encryption.md) | WP-07 Optional PIN-gated encryption (G3) | `ACCEPT` on the second pass | 1 minor + 1 first-pass major, fixed | 0 — F1 to `test-engineer` as a follow-up |
| [011](./011-wp-11-copy-deck.md) | WP-11 Copy deck and README (G3) | `REWORK_IMPLEMENTATION` | 4 (3 major, 1 minor) | 0 — F1–F3 to `frontend-dev`, F4 to the owner |
| [012](./012-wp-08-tray.md) | WP-08 Tray icon (G3) | `REWORK_ARCHITECTURE` | 3 (1 major, 2 minor) | 0 — F1 fixed by `devops`; F2 and F3 answered by ADR-0013 and ADR-0014 |
| [013](./013-wp-08-rework.md) | WP-08 rework: the `unlock_requested` event (G3) | `REWORK_ARCHITECTURE` | 2 (1 major, 1 minor) | 0 — both closed |

## Verdicts

| Verdict | Meaning | Next agent |
| ------- | ------- | ---------- |
| `ACCEPT` | Good enough to keep. Zero findings is a legitimate ACCEPT. | orchestrator lands it |
| `REWORK_IMPLEMENTATION` | The design is sound; the code is not. | the developer who owns those paths |
| `REWORK_ARCHITECTURE` | The code is faithful; the contract or design is wrong. | `architect` |
| `REWORK_TESTS` | Inconclusive — the tests cannot support a verdict. | `test-engineer` |
| `BLOCK` | Security, data loss, or a contract violation. Does not merge. | orchestrator decides |

This vocabulary is defined in `.claude/agents/critic.md` and must match it
exactly. `BLOCK` is limited to those three categories; the critic may not block
on style.

## Template

```markdown
# Review NNN — <topic>

**Reviewed:** <commit or file list>
**Verdict:** ACCEPT | REWORK_IMPLEMENTATION | REWORK_ARCHITECTURE | REWORK_TESTS | BLOCK

## Findings

### F1 — <one-line claim> [critical|major|minor]

**Location:** `path/to/file.rs:123`
**Failure scenario:** <concrete inputs → concrete wrong outcome>
**Why it survives scrutiny:** <the counter-argument considered and rejected>

## Failure layer

specification / architecture / implementation / tests / none

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|

## What I checked and found sound

<specific, so an ACCEPT is credible>

## What I could not verify

<the limits of a static read; the critic has no shell>

## Next action

architect / frontend-dev / backend-dev / test-engineer / devops / orchestrator
```

## Waivers

A finding the orchestrator decides not to fix is waived, with the reason
recorded in the review file. A waived finding is a decision with an owner; an
ignored one resurfaces later with no history attached.
