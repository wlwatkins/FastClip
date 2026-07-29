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
