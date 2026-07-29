# ADR 0009 — Durability level: WAL with `synchronous = NORMAL`

**Status:** Accepted at G0b. Amended before acceptance: the sidecar consequence
now reads "whenever the database is open" rather than "at all times", because
[ADR-0010](./0010-manual-lock.md) closes the connection on lock.
**Deciders:** architect

## Context

[Spec §4.1](../../product/spec.md#41-copy-a-clip) puts a `use_count` write on
the hottest path in the application, and says so: "This puts a disk write on the
hottest path in the app. It is accepted for accuracy, and it is a performance
risk worth measuring."

[Contract §6](../contract.md#6-open-questions-for-the-architect) question 6
asked whether the copy command returns before or after that write. The
specification already answers that — after — so the open engineering question is
what "written to disk" is required to mean, because the two plausible readings
differ in cost by roughly two orders of magnitude.

[Acceptance criterion 9](../../product/spec.md#8-acceptance-criteria) states the
requirement exactly: "A copy from either path increments `use_count` and the new
value **survives an immediate process kill**."

## Decision

**`PRAGMA journal_mode = WAL` and `PRAGMA synchronous = NORMAL`.** Every
mutation is committed before its command returns.

| Failure | Survived |
| ------- | -------- |
| Process killed, `taskkill /F`, panic, crash | Yes. The commit is already in the write-ahead log, held by the operating system. |
| Operating system crash or power loss | The last commits may be lost. The database is not corrupted. |

## Rationale

In WAL mode, `NORMAL` means SQLite writes the commit to the WAL through the
ordinary file API but does not `fsync` on each transaction. Process death cannot
lose it, because the data has already left the process. Only the operating
system going down with dirty pages can.

`FULL` adds an `fsync` per commit. On a click-to-copy tool, that `fsync` **is**
the latency the specification flagged as a risk, and it buys protection against
a failure mode no acceptance criterion mentions, for a local list of text
snippets whose recovery path — [export](../../product/spec.md#46-export-and-import-json)
— already exists.

The criterion was written as "process kill", not "power loss". Reading it as the
stronger claim would pay the whole cost on every click for a guarantee nobody
asked for.

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| `synchronous = FULL` | An `fsync` per copy, on the path the specification names as the one that must never feel slow. Protects against power loss, which no requirement asks for. |
| `synchronous = OFF` | A process kill can then leave the database inconsistent, which fails criterion 9 and gives up the crash safety [ADR-0005](./0005-sqlite-store.md) chose SQLite for. |
| Batch or debounce the count and flush later | Contradicts [spec §4.1](../../product/spec.md#41-copy-a-clip) and [ADR-0005](./0005-sqlite-store.md). It is the fallback named in ADR-0005's "revisit if", to be reached only if SQLCipher proves unworkable. |
| `FULL` for clip mutations, `NORMAL` for the copy path | Two durability levels in one database means the guarantee depends on which statement ran last, and `synchronous` is a connection-level pragma. Complexity for a distinction nobody can state. |

## Consequences

- **A power cut can lose the last few `use_count` increments, and in the worst
  case the most recently created clip.** The database itself is never left
  corrupt. This is the bad consequence and it is accepted knowingly.
- **The `-wal` and `-shm` sidecars exist whenever the database is open.**
  Anything that copies, moves, backs up or deletes the store handles all three
  files. A clean close removes them, and every path that closes the connection
  checkpoints with `PRAGMA wal_checkpoint(TRUNCATE)` first: a conversion between
  plain and encrypted, which closes before the database file is renamed, and
  [`lock`](../contract.md#lock), which closes to drop the key
  ([storage](../storage.md#locking-on-demand)).
- **The claim is measurable, so it is measured.**
  [WP-05](../../work/wp-05-crud.md)'s test report records the observed p95 of
  `copy_clip` end to end. No pass or fail threshold is set here, because the
  specification sets none; a number in the report is what lets the owner say
  whether a copy "feels slow".
- **Multi-process behaviour improves.** WAL lets a reader and a writer proceed
  concurrently, which matters if the missing single-instance guard is ever
  fixed by allowing a second window rather than forbidding it.

## Revisit if

The measurement in WP-05 shows commit latency dominating the copy path — in
which case the problem is not `synchronous`, and this ADR is evidence that it
was not — or if a requirement for power-loss durability is ever written down.
