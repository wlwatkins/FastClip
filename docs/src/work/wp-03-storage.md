# WP-03 — Storage: SQLite, ordered and crash-safe

**Objective:** replace the `HashMap`-and-JSON store with SQLite. Ordered,
crash-safe, schema-versioned. No encryption yet — that is
[WP-07](./wp-07-encryption.md).

**Depends on:** WP-01.

**Inputs:** [storage design](../architecture/storage.md),
[contract](../architecture/contract.md),
[spec §4.3](../product/spec.md#43-reorder).

## Work

### backend-dev

Build the SQLite store per [ADR-0005](../architecture/adr/0005-sqlite-store.md)
and the schema the architect ratified. A `position` column carries user
ordering, so list order is deterministic rather than an artefact of hash
iteration.

Use the crate the architect chose after the WP-02 spike. Do not pick one
yourself, and do not proceed if the spike has not run — the SQLCipher link is
what the choice turns on, even though this package adds no encryption.

Crash safety comes from SQLite's journal, not from hand-rolled
temp-file-and-rename. Enable WAL and account for the `-wal` and `-shm`
sidecars wherever the store is copied or removed.

Set up schema versioning now, before there is anything to migrate.

Add `use_count` to the `Clip` struct, defaulting to 0. It is backend-owned;
nothing outside the backend writes it.

Remove `icon`, `visible` and `clear_time` from the `Clip` struct.
Deserialisation is strict — there is no legacy format to tolerate
([ADR-0003](../architecture/adr/0003-no-legacy-migration.md)).

Write the store to `~/.fast-clip/` — `dirs::home_dir()` joined with
`.fast-clip`. Handle a `None` home directory as a typed error, not a panic. The
pre-refactor `%LOCALAPPDATA%\FastClip\db` is neither read nor modified.

Replace `expect()` in `DataBase::new()` with typed errors.

Remove `surrealdb`. [ADR-0005](../architecture/adr/0005-sqlite-store.md)
settled it: the store is SQLite, and that dependency has been declared and
unused since the beginning.

### test-engineer

Insert, update, remove and list against a database in a temporary directory.
Ordering stable across close and reopen. A corrupt file fails cleanly rather
than panicking. A database from an unrecognised `user_version` is rejected, not
misread. Starting with a pre-refactor `db` file present produces an empty list,
no error, and leaves that file untouched.

A killed process mid-write leaves a readable database — SQLite should give this
for free, which is worth confirming rather than assuming.

### critic

Weight data loss first. Can any sequence of calls leave the store truncated,
partially written, or reordered?

## Definition of done

- Order is stable across restarts.
- A process killed mid-write leaves a readable database.
- Schema version is recorded and checked on open.
- The store is created under `~/.fast-clip/` and nothing is written to
  `%LOCALAPPDATA%\FastClip\`.
- No `unwrap()` or `expect()` reachable after startup.
- `surrealdb` is gone.

## Risks

The C build. A bundled SQLCipher compile is the slowest thing in CI and the
most likely to break on a runner. WP-02's spike exists to find that out before
this package depends on it.
