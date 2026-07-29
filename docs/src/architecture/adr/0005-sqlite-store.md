# ADR 0005 — SQLite via SQLCipher as the store

**Status:** Accepted
**Deciders:** owner

## Context

The pre-refactor store is a `HashMap` serialised to a JSON file, rewritten
whole on every change.

Two decisions made that untenable. [Spec §4.1](../../product/spec.md#41-copy-a-clip)
requires `use_count` to be written through on **every copy** — the hottest path
in the app. [ADR-0004](./0004-optional-pin-encryption.md) requires the store to
be optionally encrypted. Together, a whole-file format means re-serialising and
re-encrypting every clip each time the user clicks one.

## Decision

**SQLite, with SQLCipher for encryption at rest.**

- Clips live in a table with a `position` column for user ordering.
- `use_count` is incremented with a targeted `UPDATE`, not a full rewrite.
- When encryption is enabled, SQLCipher encrypts the database at page level.

The key construction from [ADR-0004](./0004-optional-pin-encryption.md) is
unchanged. SQLCipher consumes the random DEK as a raw key; the DEK is still
wrapped by `Argon2id(PIN, salt)` and then DPAPI-protected. SQLCipher replaces
the *cipher plumbing*, not the *key policy*.

## Rationale

**Partial writes.** `UPDATE clips SET use_count = use_count + 1 WHERE id = ?`
touches a few bytes. The whole-file alternative encrypts and fsyncs the entire
store per click, which grows with the user's data on the one path that must
never feel slow.

**Durability is not ours to invent.** SQLite's journal handles crash safety
that we would otherwise implement with temp-file-and-rename, and would
implement less well.

**Encryption without leaks.** SQLCipher encrypts pages in place. The
alternatives — decrypting to a temp file, or into an in-memory database flushed
on close — either put plaintext on disk or make durability a fiction, since
nothing is committed until the flush.

**It settles the SurrealDB question.** That dependency has been declared and
unused since the beginning. It is removed.

## Consequences

- **A new dependency with a C build.** `rusqlite` with a bundled SQLCipher
  feature compiles C on the CI runner. `windows-latest` has the toolchain, but
  it costs build time and binary size, and it must be cached or Rust CI becomes
  unbearable. `devops` owns confirming this in [WP-02](../../work/wp-02-toolchain.md).
- **Schema migrations become a permanent concern.** Use `PRAGMA user_version`
  or a migrations table from the first release. This is not the same thing as
  [ADR-0003](./0003-no-legacy-migration.md), which refused to read *pre-refactor*
  data; it is about this project's own schema evolving.
- **The store is no longer hand-readable, even unencrypted.** ADR-0003 relied on
  the old file being plaintext JSON someone could recover by hand. That property
  is gone, which makes
  [export §4.6](../../product/spec.md#46-export-and-import-json) the only
  user-facing recovery path. Export matters more than it did.
- **WAL mode writes sidecar files.** A `-wal` and a `-shm` accompany the
  database. Anything that copies, backs up or deletes the store must account for
  all three, and the "new filename" rule from ADR-0003 covers the set.
- **Multi-process access is defined rather than corrupting.** SQLite locks. The
  missing single-instance guard remains a defect worth fixing, but two instances
  now block each other instead of overwriting from stale memory.
- Enabling and disabling encryption use SQLCipher's `sqlcipher_export()` to
  convert between a plain and an encrypted database, rather than hand-rolled
  read-decrypt-write.

## Deferred to the architect: which Rust crate

This ADR fixes SQLite and SQLCipher. It does **not** fix the crate, because the
two candidates differ in exactly the place that matters — how reliably they
link and configure SQLCipher.

| Candidate | For | Against |
| --------- | --- | ------- |
| `rusqlite` | `bundled-sqlcipher` is a first-class feature; the C build and the key pragma are the documented path. Synchronous, which suits a local file. | Hand-written SQL. No migration framework; `PRAGMA user_version` by hand. |
| `sea-orm` | Entity codegen and `sea-orm-migration`, which is the strongest argument for it. Async, and tokio is already a dependency. | SQLCipher is not first-class. It inherits SQLx's [pragma-ordering defect](https://github.com/launchbadge/sqlx/issues/2009), where chaining `.journal_mode()` or `.foreign_keys()` inserts statements between `PRAGMA key` and the cipher pragmas and yields `file is not a database`. Linking against SQLCipher rather than vanilla SQLite is a `libsqlite3-sys` feature-unification problem across the whole tree. |

The schema is one clips table of six columns and a small meta table, served by
roughly eight statements. That does not on its own justify an ORM. The honest
argument for `sea-orm` is the migration framework and the learning value, not
the query layer.

**Requirement, whichever is chosen:** prove the SQLCipher link before any code
depends on it. A time-boxed spike in [WP-02](../../work/wp-02-toolchain.md) that
opens an encrypted database on `windows-latest` in CI, writes a row, closes,
reopens with the key, and reads it back. If `sea-orm` cannot do that in the
time box, `rusqlite` is the fallback and the spike has paid for itself.

Betting the security-critical component on a link nobody has demonstrated is
the failure this requirement exists to prevent.

## Rejected

**Encrypted JSON, whole-file.** Simpler, no C dependency, hand-readable when
unencrypted. Rejected because the per-copy rewrite grows with the user's data
and sits on the hot path.

**Encrypted JSON with a separate counts file.** Keeps writes cheap without
SQLite, but introduces two files that can disagree and a cleanup obligation
when a clip is deleted.

**SQLite decrypted into memory at launch.** No SQLCipher dependency, but
nothing is durable until the flush, which contradicts write-through entirely.

## Revisit if

The SQLCipher build proves unworkable on the release runner. The fallback is
encrypted JSON with batched count flushes, which trades accuracy of the ranking
for simplicity.
