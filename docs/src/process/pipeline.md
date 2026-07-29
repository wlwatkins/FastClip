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

| Gate | Status |
| ---- | ------ |
| G0a Product | Accepted — [specification](../product/spec.md) |
| G0b Design | In progress — `architect` closing [six open questions](../architecture/contract.md) |
| G1 Build | Blocked on G0b |
| G2 Test | Blocked |
| G3 Review | Blocked |
| G4 Merge | Blocked |

## First dispatch

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
