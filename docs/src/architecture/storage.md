# Storage and encryption

**Status:** Format decided by
[ADR-0005](./adr/0005-sqlite-store.md); schema and crate ratified by the
architect at G0b.

Read [ADR-0004](./adr/0004-optional-pin-encryption.md) and
[ADR-0005](./adr/0005-sqlite-store.md) before touching this. They rule work out
as well as in.

## Current state

`DataBase` is a `HashMap<Uuid, Clip>` serialised as pretty-printed JSON to
`%LOCALAPPDATA%\FastClip\db`. `save()` writes over the live file, so a crash
mid-write truncates it and the user loses every clip.

All of it is replaced, including the location.

## Location

`dirs::home_dir()` joined with `.fast-clip` — on Windows, `C:\Users\<user>\.fast-clip\`.

`dirs::home_dir()` returns an `Option`. It is not `expect()`ed; a machine with
no resolvable home directory produces a typed error and a message, not a panic.
The current `DataBase::new()` panics on exactly this path.

**One caveat, recorded rather than argued.** `%LOCALAPPDATA%` exists to hold
machine-local state that must never roam, which is what a DPAPI-bound file
wants — a roaming profile that copies `~/.fast-clip` to another machine
produces a store that cannot be opened there
([ADR-0004](./adr/0004-optional-pin-encryption.md)). The failure is a clear
"cannot open on this account" message rather than data loss, and export remains
the way to move clips. The owner chose the home directory for discoverability.

## Required properties

| Property | Reason |
| -------- | ------ |
| SQLite | [ADR-0005](./adr/0005-sqlite-store.md). A `position` column carries user ordering; `use_count` is incremented with a targeted `UPDATE` rather than a whole-file rewrite. |
| SQLCipher when encryption is on | Page-level encryption, keyed by the DEK from [ADR-0004](./adr/0004-optional-pin-encryption.md). Off by default. |
| Keyed from PIN **and** DPAPI | A random DEK wrapped by `Argon2id(PIN, salt)`, then DPAPI-protected. SQLCipher consumes the DEK; it does not change the key policy. |
| Schema-versioned | `PRAGMA user_version` or a migrations table, from the first release. This project's own schema will change even though it never reads pre-refactor data. |
| Crash-safe | SQLite's journal, not hand-rolled temp-file-and-rename. WAL mode writes `-wal` and `-shm` sidecars; anything that copies, backs up or deletes the store handles all three. |
| Located at `~/.fast-clip/` | `dirs::home_dir()` joined with `.fast-clip`. A directory, not a file — SQLite's `-wal` and `-shm` sidecars and the wrapped DEK blob live beside the database. |
| Separate from the pre-refactor store | The old build wrote `%LOCALAPPDATA%\FastClip\db`. A different directory makes [ADR-0003](./adr/0003-no-legacy-migration.md)'s non-collision guarantee structural rather than a naming convention — there is no path on which the two can meet. |

## No legacy path

[ADR-0003](./adr/0003-no-legacy-migration.md) removed the migration from
pre-refactor data. That is different from schema migration, which SQLite makes a
permanent concern and which starts now.

The old JSON store was at least hand-readable in an emergency. A SQLite file is
not, so [export](../product/spec.md#46-export-and-import-json) is the only
user-facing recovery path. It matters more than it did.

Cases to cover: an empty database, a corrupt file, a database from an
unrecognised `user_version` — rejected with `unsupported_version`, never
misread — and a pre-refactor `db` file present on disk, which must be left
untouched and produce an empty clip list rather than an error.

## Switching encryption on and off

Use SQLCipher's `sqlcipher_export()` to convert between a plain and an
encrypted database rather than hand-rolling read-decrypt-write. A process
killed mid-conversion must leave a readable database in one state or the other.

## Open — architect, G0b

- **Which crate**, and the spike that proves it links SQLCipher on
  `windows-latest`. See
  [ADR-0005](./adr/0005-sqlite-store.md).
- The schema: columns, indices, and whether `position` is dense or sparse.
  A sparse ordering makes a reorder one `UPDATE` rather than N.
- Argon2id parameters, and how they were chosen for this machine class.
- How lock state is represented, and who owns it.
- Where the DPAPI-protected wrapped DEK lives relative to the database.
- What the user sees when the store cannot be opened. A blank window is not
  acceptable; the message points at
  [export and import](../product/spec.md#46-export-and-import-json).
