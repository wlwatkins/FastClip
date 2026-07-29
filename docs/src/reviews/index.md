# Review log

Every G3 review lands here as `NNN-<topic>.md`, with a row below and a line in
`SUMMARY.md`.

The log is append-only. Superseding a review means writing a new one.

## Index

| # | Topic | Verdict | Findings | Waived |
| - | ----- | ------- | -------- | ------ |
| — | *no reviews yet* | | | |

## Verdicts

| Verdict | Meaning |
| ------- | ------- |
| `PASS` | Nothing found. A legitimate and expected outcome. |
| `PASS WITH FINDINGS` | Real issues, none blocking. The orchestrator schedules them. |
| `BLOCK` | Security, data loss, or a contract violation. Does not merge. |

`BLOCK` is limited to those three categories. The critic may not block on
style.

## Template

```markdown
# Review NNN — <topic>

**Reviewed:** <commit or file list>
**Verdict:** PASS | PASS WITH FINDINGS | BLOCK

## Findings

### F1 — <one-line claim> [critical|major|minor]

**Location:** `path/to/file.rs:123`
**Failure scenario:** <concrete inputs → concrete wrong outcome>
**Why it survives scrutiny:** <the counter-argument considered and rejected>

## What I checked and found sound

<specific, so a PASS is credible>

## What I could not verify

<the limits of a static read; the critic cannot run builds or tests>
```

## Waivers

A finding the orchestrator decides not to fix is waived, with the reason
recorded in the review file. A waived finding is a decision with an owner; an
ignored one resurfaces later with no history attached.
