# The gated pipeline

The gates describe how a single [work package](../work/index.md) moves. Every
package runs G1 → G2 → G3 → G4; the whole refactor runs G0a and G0b once, then
repeats the rest twelve times.

Each gate's exit artifact is a file. A gate opens only when the previous
artifact exists on disk. Reports use the templates in
[reporting](./reporting.md).

```text
G0a PRODUCT   owner         → docs/src/product/spec.md
G0b DESIGN    architect     → contract.md updated, ADRs written
G1  BUILD     frontend-dev  ┐ parallel; they share no files,
              backend-dev   │ only the contract
              devops        ┘
G2  TEST      test-engineer → tests exist, red before green
G3  REVIEW    critic        → docs/src/reviews/NNN-<topic>.md
G4  MERGE     orchestrator  → land, or return to G1
```

## Rules

**A gate exit is a file.** "I updated the contract" is not an exit; a diff to
[contract.md](../architecture/contract.md) is. Without this rule the pipeline
degrades into agents asserting completion to each other.

**G0a belongs to the owner.** Product questions are answered by the person who
wants the software. An agent handed one produces a plausible answer, and every
downstream agent then treats it as settled.

**Critic findings are not advisory.** Each is fixed, or waived by the
orchestrator with the reason recorded in the review file.

**A G3 rejection re-enters at G1.** The loop is G1 → G2 → G3. A one-line fix
does not skip tests.

**Every handoff names the next owner.**

**Do not shortcut the pipeline for small changes.** The team is the
deliverable; an ad hoc process teaches nothing reusable.

## Current position

Both one-off gates are behind us, so position is now tracked per package rather
than per gate.

| Gate | Status |
| ---- | ------ |
| G0a Product | Accepted — [specification](../product/spec.md) |
| G0b Design | **Landed** — [contract §6](../architecture/contract.md) is empty; `critic` returned `ACCEPT` on the fourth review ([003](../reviews/003-wp-01-contract.md)). Four findings carried to WP-03 and WP-07 |

### Per package

| Package | Position | Review |
| ------- | -------- | ------ |
| WP-01 Contract ratification | G4 landed | [003](../reviews/003-wp-01-contract.md) |
| WP-02 Toolchain and CI skeleton | G4 landed, one G4 condition open | [002](../reviews/002-wp-02-toolchain.md) |
| WP-03 Storage | G4 landed | folded into WP-05's review |
| WP-04 Frontend scaffold | G4 landed | folded into WP-05's review |
| WP-05 Clip list, copy, CRUD | G4 landed | [005](../reviews/005-wp-05-crud.md) |
| WP-06 Reordering | G4 landed | [007](../reviews/007-wp-06-reorder.md) |
| WP-07 Optional PIN-gated encryption | G4 landed; one minor test-layer follow-up open | [010](../reviews/010-wp-07-encryption.md) |
| WP-08 Tray icon | Reworked and re-reviewed; all five findings across both rounds closed. **Cannot exit G4** — waiting on an owner decision (spec §4.8) and a manual tray check | [012](../reviews/012-wp-08-tray.md), [013](../reviews/013-wp-08-rework.md) |
| WP-09 Export and import | G4 landed | [008](../reviews/008-wp-09-export-import.md) |
| WP-10 Palette and accessibility | G4 landed | [004](../reviews/004-wp-10-palette.md) |
| WP-11 Copy deck and README | Reworked, G2 passed. **G3 re-review not run** — dispatched 2026-07-30 and cut short. Re-dispatch before landing | [011](../reviews/011-wp-11-copy-deck.md) |
| WP-12 Release pipeline | **Blocked, owner** | — |
| WP-13 Search and filter | G4 landed | [009](../reviews/009-wp-13-search.md) |
| WP-14 Settings and window state | G4 landed | [006](../reviews/006-wp-14-settings.md) |

WP-12 is blocked on two things only the owner can supply: a signing certificate
that does not exist, and a definition of done written in terms of CI runs, which
cannot happen now that the project has no Actions credits. Its DoD needs
rewriting against a local build before the package can be dispatched.

## The G0b dispatch, kept as the worked example

```text
Read docs/src/product/spec.md, docs/src/architecture/contract.md,
docs/src/work/wp-01-contract.md, and every ADR in
docs/src/architecture/adr/ — there are six, and 0004 and 0005 define the
encryption and storage surface you are ratifying.

Close the six open questions in the contract's §6. Where a question needs a
product decision rather than an engineering one, stop and say so instead of
answering it.
```

The final sentence keeps G0a and G0b from collapsing into each other.
