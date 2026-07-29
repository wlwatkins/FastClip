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

## Carried from review 003

[Review 003](../reviews/003-wp-01-contract.md) accepted the contract with four
findings carried rather than fixed in a fifth round. One lands here, and the
`architect` writes the missing sentence into
[storage](../architecture/storage.md) as part of this dispatch, before
`backend-dev` implements the step.

**F1 [major] — startup recovery step 5's `keyfile` delete has no defined failure
behaviour.** Steps 1, 2, 6 and 7 each state what their own failure does; step 5
does not. The idiomatic `?` makes startup record the fault, so `get_lock_state`
returns `storage` and the user meets a failure screen over an intact plaintext
store — and that message points at import, which is how a user replaces a store
that was never damaged. The section that would grant the delete non-fatality
(`storage.md:302-304`) still names step 4, which after a round-three renumbering
deletes nothing.

Decide and write down whether a failed step 5 delete is absorbed or fatal. The
same review's F4 is carried to [WP-07](./wp-07-encryption.md) instead of here,
because it concerns `lock`, which does not exist until encryption does.

## Risks

The C build. A bundled SQLCipher compile is the slowest thing in CI and the
most likely to break on a runner. WP-02's spike exists to find that out before
this package depends on it.
