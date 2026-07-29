# Storage and encryption

**Status:** Format decided by
[ADR-0005](./adr/0005-sqlite-store.md). Schema, key material, durability and
lock-state ownership ratified by the architect at G0b, and amended in the second
round for [manual lock](#locking-on-demand), and again for the flat backoff of
[ADR-0011](./adr/0011-flat-backoff.md). **One item remains open: which Rust
crate**, which waits on the [WP-02](../work/wp-02-toolchain.md) spike.

Read [ADR-0004](./adr/0004-optional-pin-encryption.md),
[ADR-0005](./adr/0005-sqlite-store.md),
[ADR-0009](./adr/0009-durability-level.md),
[ADR-0010](./adr/0010-manual-lock.md) and
[ADR-0011](./adr/0011-flat-backoff.md) before touching this. They rule work out
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

## Files

| File | Contents | Encrypted | Readable while locked |
| ---- | -------- | --------- | --------------------- |
| `clips.db` | The clips table. | Only when the user opted in. | no |
| `clips.db-wal`, `clips.db-shm` | SQLite's write-ahead log and shared-memory index. Present while the database is open, and removed by a clean close ([locking](#locking-on-demand), a conversion, shutdown). | With the database. | — |
| `keyfile` | The DPAPI-protected wrapped DEK, the KDF parameters, and the failed-attempt state. Present **only** when encryption is on. | DPAPI. | yes, by the backend |
| `settings.json` | `{ "version": 1, "always_on_top": false }`. | no | yes |
| `fastclip.log` and its rotations | Diagnostics ([ADR-0012](./adr/0012-logging.md)). **Never any clip `label` or `value`**, and never a PIN, DEK or salt. | no | yes |

`settings.json` is plaintext and outside the database because
[spec §4.4](../product/spec.md#44-always-on-top) requires the always-on-top
toggle to apply at launch, which is before any PIN has been entered. It holds no
clip data. It is written atomically: a temporary file in the same directory,
then a rename.

Rejected: a `settings` table inside the database. It would be unreadable while
locked, which is the one moment the window has to be positioned correctly.

Its `version` field is the one on-disk version whose unrecognised value is not an
error: the file is ignored, `always_on_top` falls back to `false`, and the next
write replaces it at version 1
([contract](./contract.md#versions-on-disk)). The same applies to a missing,
truncated or unparseable file. Nothing in it is worth failing a launch over.

## Required properties

| Property | Reason |
| -------- | ------ |
| SQLite | [ADR-0005](./adr/0005-sqlite-store.md). A `position` column carries user ordering; `use_count` is incremented with a targeted `UPDATE` rather than a whole-file rewrite. |
| SQLCipher when encryption is on | Page-level encryption, keyed by the DEK from [ADR-0004](./adr/0004-optional-pin-encryption.md). Off by default. |
| Keyed from PIN **and** DPAPI | A random DEK wrapped by `Argon2id(PIN, salt)`, then DPAPI-protected. SQLCipher consumes the DEK; it does not change the key policy. |
| Schema-versioned | `PRAGMA user_version`, from the first release. This project's own schema will change even though it never reads pre-refactor data. |
| Crash-safe | SQLite's journal, not hand-rolled temp-file-and-rename. WAL mode writes `-wal` and `-shm` sidecars; anything that copies, backs up or deletes the store handles all three. |
| Located at `~/.fast-clip/` | `dirs::home_dir()` joined with `.fast-clip`. A directory, not a file — SQLite's sidecars, the key material and the settings live beside the database. |
| Separate from the pre-refactor store | The old build wrote `%LOCALAPPDATA%\FastClip\db`. A different **directory** makes [ADR-0003](./adr/0003-no-legacy-migration.md)'s non-collision guarantee structural rather than a naming convention — there is no path on which the two can meet. |

## Schema

One table. `PRAGMA user_version = 1`.

```sql
CREATE TABLE clips (
    id        TEXT    NOT NULL PRIMARY KEY,
    label     TEXT    NOT NULL,
    value     TEXT    NOT NULL,
    colour    TEXT    NOT NULL,
    use_count INTEGER NOT NULL DEFAULT 0 CHECK (use_count >= 0),
    position  INTEGER NOT NULL UNIQUE CHECK (position >= 0)
) STRICT;

CREATE INDEX clips_ranking ON clips (use_count DESC, position ASC);
```

| Column | Notes |
| ------ | ----- |
| `id` | Lowercase hyphenated UUID, 36 characters, backend-minted. |
| `label`, `value`, `colour` | Validated at the IPC boundary ([contract §4](./contract.md#4-errors)). The database enforces presence, not length: length is one rule with one owner, and duplicating it here would let the two drift. |
| `use_count` | Backend-owned. Never crosses the seam ([ADR-0008](./adr/0008-use-count-stays-backend-side.md)). |
| `position` | Dense, `0..N-1`, no gaps ([ADR-0007](./adr/0007-list-order-representation.md)). |

`clips_ranking` serves the tray query —
`ORDER BY use_count DESC, position ASC LIMIT 10`
([spec §4.5](../product/spec.md#45-tray-icon-with-right-click-copy)) — in one
index scan. The list query orders by `position` alone and uses the implicit
unique index on that column.

**No meta table.** `PRAGMA user_version` holds the schema version and
`settings.json` holds the settings. A key-value meta table would duplicate the
first and be unreadable while locked for the second.

**`STRICT` requires SQLite 3.37 or newer.** If the crate chosen after the WP-02
spike bundles something older, `backend-dev` reports it rather than silently
dropping the keyword — type discipline then rests on the Rust layer alone, which
is a weaker guarantee and someone should know it changed.

### Position is dense, and every renumber is a two-step write

`UNIQUE(position)` makes a duplicate position impossible on disk rather than
merely unlikely. It also blocks an in-place permutation: setting row A to
position 2 fails while row B still holds it. Inside the one transaction the
backend therefore offsets every position by `+N` first, then writes the final
`0..N-1` values.

**This applies to `delete_clip`'s renumbering too, and it is a real hazard rather
than a theoretical one.** The obvious statement —

```sql
UPDATE clips SET position = position - 1 WHERE position > ?;   -- do not do this
```

— is correct only if the rows happen to be visited in ascending `position` order.
Visited descending, the first row decremented collides with the row below it that
has not moved yet, and SQLite raises `SQLITE_CONSTRAINT` on the spot: a `UNIQUE`
index is checked per row as it is written, and SQLite has no deferred form of it
outside foreign keys. Visitation order is a property of the query plan — a range
scan on the `position` index gives ascending order, a full table scan gives rowid
order — and after any reorder, rowid order bears no relation to position order.

The design must not depend on which plan is chosen, in either direction. So
**delete renumbers with the same offset-then-write pair as reorder**, inside the
same transaction: offset the affected rows clear of the occupied range, then
write their final values. One technique for both paths, and no statement that can
transiently duplicate a position regardless of the order rows are visited in.

`ORDER BY` on an `UPDATE` was considered and rejected: SQLite accepts it only
when built with `SQLITE_ENABLE_UPDATE_DELETE_LIMIT`, which no crate under
consideration guarantees ([the crate is still open](#open--the-crate)), so it
would make correctness depend on a build flag nobody checked.

Importing appends, continuing from the current maximum, and needs neither step —
it occupies positions nothing else holds.

Sparse positions were rejected in
[ADR-0007](./adr/0007-list-order-representation.md): they only pay off with a
delta-style reorder command, which was itself rejected.

## Durability

`PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, decided in
[ADR-0009](./adr/0009-durability-level.md). Every mutation commits before its
command returns. A process kill cannot lose a committed write; a power cut can
lose the last few. Read that ADR before changing either pragma.

## Connections

**At most one connection to the live store, behind a mutex.** Every command
takes the mutex, so `list_clips` during an `import_clips` transaction waits
rather than failing.

It is opened at these points and no others:

| Opened | When |
| ------ | ---- |
| [Startup recovery](#startup-recovery) step 7 | the store is `plaintext` or was just created |
| `unlock` | the store is `encrypted`, once a PIN has unwrapped the DEK |
| A conversion's step 7 | the reopen after the commit point |
| A conversion's abort | reopening the original store, which was never replaced |

and closed by [`lock`](#locking-on-demand), by a conversion's step 4, and at
shutdown.

A conversion also opens `clips.db.new` at its step 3, which is a *different file*
and never the live store. It is closed at step 4, before the rename.

The first two rows are the only places the store is opened **on the path into
it**, which is what lets [contract §2](./contract.md#opening-the-database) name
one place each of `crypto { corrupt }` and `unsupported_version { schema }` is
*discovered*. **No command opens the store because it found it closed** — that is
the rule, and it is narrower than "no command opens the store". A conversion
opens one as a step of the work it was asked to do. `list_clips` is never the
first thing to touch the store.

Discovering a fault once does **not** mean one command reports it. A fault is
recorded and then relayed by whichever command next needs the store
([contract §2](./contract.md#how-it-reaches-the-caller--from-wherever-the-caller-asks)).

The two conversions are the exception, and they are not on that path: each opens
the database it is building, and each **reopens the converted store before
returning** ([below](#switching-encryption-on-and-off)). A conversion reporting
`crypto { corrupt }` is reporting a failure of the work it was asked to do, not
discovering the state of an existing store.

`PRAGMA busy_timeout = 5000` on that connection, for the case a mutex cannot
cover: a second FastClip process. SQLite locks between processes, and without a
timeout the second one returns `SQLITE_BUSY` immediately and the user sees
`storage` for what is a wait of milliseconds.

Rejected: a connection pool. WAL allows concurrent readers, but with a single
window issuing one command at a time the pool would add a key-pragma-ordering
obligation on every checkout ([encryption](#encryption)) for no measurable gain,
and getting that ordering wrong is the documented route to
`file is not a database`.

## Startup recovery

Before the backend serves any command, in this order:

1. Create `~/.fast-clip/` if it is absent. Failure is `storage`, reported by
   whichever command is called first.
2. **Delete every intermediate file**: `clips.db.new`, `clips.db.new-wal`,
   `clips.db.new-shm`, and `keyfile.new`. They exist only inside a conversion
   ([below](#switching-encryption-on-and-off)), so any that survives a restart is
   debris from an interrupted one. **This step does not depend on the
   classification and runs before it**, so no later branch can suppress it. A
   delete that fails is absorbed ([below](#when-a-delete-fails)).
3. **Classify `clips.db` into one of four states** ([below](#classifying-clipsdb)).
   Steps 4 to 7 branch on that classification and none of them re-derives it.
4. **If the classification is `unreadable`, stop here.** Delete nothing further,
   create nothing, open nothing. Record the fault; `get_lock_state` returns
   `storage` and the frontend shows the failure screen
   ([contract](./contract.md#startup-sequence)). Steps 5 to 7 do not run.
5. Delete `keyfile` if the classification is `absent` or `plaintext` — **never on
   any other classification.** The database header is the source of truth
   ([below](#who-owns-lock-state)) and a key file beside a plaintext database, or
   beside no database at all, is debris from an interrupted conversion. The
   `absent` case matters because step 6 is about to create a plaintext database,
   and a `keyfile` left from before would then sit beside one. **A delete that
   fails is absorbed** ([below](#when-a-delete-fails)), like step 2's.
6. If the classification is `absent`, create `clips.db` with
   `PRAGMA user_version = 1` and the schema above. A first run therefore reaches
   an empty clip list, not a failure screen.
7. If the classification is `plaintext`, or was `absent` and step 6 created one,
   **open the database and read `PRAGMA user_version`.** Hold the connection
   ([connections](#connections)). **Pass condition:** the database opens and
   `user_version` is less than or equal to this build's schema version. Too new
   → record the fault and serve it from `get_lock_state`
   ([contract](./contract.md#opening-the-database)); it will not open → the same,
   as `crypto { corrupt }`. Do not fail startup in either case, or there is no
   window in which to show the message. **Lower** is migrated and then held, not
   rejected — there is nothing to migrate at version 1, and this is the sentence
   that says so rather than leaving the case unwritten.
   If the classification is `encrypted`, do **not** open it: there is no DEK until
   a PIN unwraps one, so the same two faults belong to `unlock`.

**Why the sweep is first and ungated.** The `unreadable` stop exists to prevent
deleting `keyfile` and creating a fresh database when the store's state is
unknown — that pair is what destroys a store. The intermediates are a different
class of file: `clips.db.new` and its sidecars are debris by definition, and at
every kill instant `clips.db` is intact beside them, so an intermediate is never
the only copy of anything.

Gating the sweep on the classification suppressed it in the case where it matters
most, and the two causes are positively correlated: a second FastClip instance
produces the `unreadable` classification *and* is the likeliest reason the sweep
would fail. An interrupted `disable_encryption` leaves a complete plaintext
`clips.db.new` holding every label and value, and with the sweep gated it would
survive every launch for as long as the handle was held —
[criterion 6](../product/spec.md#8-acceptance-criteria) breached indefinitely by
a design choice rather than by an accident.

Deleting a file another process holds simply fails on Windows, which step 2
already treats as non-fatal. There is nothing to protect by not trying.

### Classifying `clips.db`

Four states, and the fourth is the one that matters. Open the file and read its
first 16 bytes:

| Classification | Determined by | Deletes permitted |
| -------------- | ------------- | ----------------- |
| `absent` | The open failed with **`io::ErrorKind::NotFound` specifically**. | `keyfile`, intermediates |
| `plaintext` | 16 bytes read, and they are the ASCII string `SQLite format 3` with its terminating NUL. | `keyfile`, intermediates |
| `encrypted` | 16 bytes read, and they are anything else. A SQLCipher database begins with a random salt, so the magic does not match. | intermediates only |
| `unreadable` | The open or the read failed for **any other reason** — permission denied, a sharing violation, a transient I/O error, a short read. | **none** |

**`unreadable` is not `absent`, and the difference is a user's entire store.**
Both idiomatic spellings of "is it there" get this wrong: `Path::exists()` is
documented to return `false` when the metadata call fails for any reason, and
`fs::read(&path).is_err()` is true on `PermissionDenied` just as it is on a
missing file. Either one classifies a locked or unreadable **encrypted** database
as absent. Step 5 then deletes `keyfile`, step 6 creates a fresh database beside
the old one, and the wrapped DEK — which existed in exactly one file, with no
reset path in [ADR-0004](./adr/0004-optional-pin-encryption.md) — is gone. Every
clip becomes permanently unrecoverable, silently, at a routine launch.

The reachable causes are ordinary: an ACL change after a profile move, an
antivirus or backup product holding a handle, a transient I/O error, or the
second instance this build has no guard against
([inherited debt](../reference/debt.md)).

So the classification is made **once**, from the error kind of a single open, and
every deletion is gated on a *positive* classification. No step may act on "not
encrypted" or "not present"; only on `absent`, `plaintext` or `encrypted` having
been established. `unreadable` deletes nothing and creates nothing, so a launch
that hits it and a launch after the permission is fixed or the other process
exits differ only in that the first showed a message.

### Every step's failure behaviour, in one place

Startup recovery has seven steps and each one says what a failure does. The table
exists so that a step added later cannot quietly be the one that does not.

| Step | On failure |
| ---- | ---------- |
| 1 — create `~/.fast-clip/` | **Fatal.** `storage`, from whichever command is called first. Nothing works without the directory. |
| 2 — sweep intermediates | **Absorbed** ([below](#when-a-delete-fails)). |
| 3 — classify `clips.db` | Not a failure. A read that fails *is* the `unreadable` classification. |
| 4 — stop on `unreadable` | Not a step that can fail; it is the stop itself. |
| 5 — delete a stray `keyfile` | **Absorbed** ([below](#when-a-delete-fails)). |
| 6 — create `clips.db` when `absent` | **Fatal.** `storage`, from `get_lock_state`. There is no store and none could be made. |
| 7 — open and check `user_version` | **Fatal**, as `crypto { corrupt }` or `unsupported_version { schema }` from `get_lock_state`. Startup itself still completes, so there is a window to show the message. |

"Fatal" means the fault is recorded and served from a command; startup always
runs to completion, because a backend that refuses to start leaves nothing to
show the user.

### When a delete fails

**Steps 2 and 5 are both absorbed**: the delete is attempted, a failure is
logged ([ADR-0012](./adr/0012-logging.md)), startup continues, and the next
launch tries again.

The one legitimate cause is another process holding the file — a second FastClip
mid-conversion — and deleting it there would break that conversion. Refusing to
start over a stray file would also deny the user the export that is the only
recovery path this design has.

**Step 5 is absorbed for a stronger reason than step 2**, and it is worth being
explicit because the idiomatic `?` gets it wrong. The file it deletes is a
DPAPI-wrapped DEK for a database that is plaintext or absent: it decrypts
nothing, it holds no clip data, and it cannot confuse the state machine, because
the encryption state is read from the database header and never from this file's
presence ([below](#who-owns-lock-state)). Propagating that failure would put a
user in front of the failure screen — which points at import as the recovery
path — over a **completely intact plaintext store**. That is how someone replaces
a store that was never damaged, and it would be caused by a leftover file that
does nothing.

The stray `keyfile` is collected at the next launch at which the delete succeeds,
by this same step.

**The honest bound on the disclosure is therefore "until a launch at which the
delete succeeds", not "until the next launch".** The earlier wording claimed the
stronger bound; it is not available, because the delete can fail for a reason
FastClip does not control.

**Step 2 is a disclosure rule, not tidiness.** `clips.db.new` holds every
`label` and `value`, and on the disable-encryption path it holds them in
plaintext. A user whose `disable_encryption` was interrupted still believes
their store is encrypted — `clips.db` still is — while a complete plaintext copy
of it sits beside them. Sweeping at startup bounds that exposure to the first
launch at which the delete succeeds ([above](#when-a-delete-fails)).
Nothing shorter is achievable, because a process can be killed at any instant
during the write.

Step 2 also fixes a second failure that is merely annoying: a leftover
`clips.db.new` from an interrupted *enable* is an encrypted database under a
different key, and `sqlcipher_export()` into it fails, so without the sweep the
user could never enable encryption again and no message would say why.

**The sweep is not gated on the classification**, and an earlier version of this
page gated it — suppressing it on `unreadable`, which is the classification most
likely to coincide with a live plaintext disclosure. The reasoning is above.

**`clips.db-wal` and `clips.db-shm` are never swept, at startup or anywhere
else.** A WAL beside `clips.db` is committed data that a process kill is entitled
to leave, and deleting it discards exactly the writes
[ADR-0009](./adr/0009-durability-level.md) guarantees survive a kill. There is
nothing to detect, either: a sidecar that does not match its database is made
impossible by the conversions deleting the old ones **before** the rename
([below](#switching-encryption-on-and-off)), not by a sweep noticing afterwards.
This is stated as a rule because a later reader who finds the mismatch hazard
will reach for a sweep, and a sweep is the one fix that trades a disclosure for
data loss.

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

## Encryption

### What the key material file holds

`~/.fast-clip/keyfile` is a single leading version byte followed by a
DPAPI-protected blob. The version byte is **outside** the blob so that an
unrecognised format is reported as
`unsupported_version { component: "key_material" }` rather than as a DPAPI
failure.

Inside the blob, once unprotected:

| Field | Contents |
| ----- | -------- |
| `kdf` | `argon2id`, with `m_cost_kib`, `t_cost`, `p_cost` and a 16-byte random `salt`. |
| `wrapped_dek` | `XChaCha20-Poly1305(key = Argon2id(PIN, salt), nonce, DEK)`, with the 24-byte nonce stored beside it. |
| `failed_attempts` | Consecutive wrong PINs. Reset to 0 on a successful unlock. Only the transition to 5 changes behaviour; beyond that the value is inert, because the wait no longer varies with it ([ADR-0011](./adr/0011-flat-backoff.md)). |
| `locked_until_unix_ms` | The end of the current wait, or absent. |

**The KDF parameters are stored in the file, not compiled into the binary**, so
the cost can be raised later without making every existing store unopenable.
That is the whole reason they are written down.

XChaCha20-Poly1305 rather than AES-256-GCM: a 192-bit nonce removes the
nonce-collision analysis entirely, and the wrap happens rarely enough that AES
hardware acceleration is worth nothing here.

### Argon2id parameters and the backoff

Binding floor: `m_cost ≥ 19 MiB`, `t_cost ≥ 2`, `p_cost = 1` — the OWASP
minimum for Argon2id.

`backend-dev` tunes upward in [WP-07](../work/wp-07-encryption.md) so that one
unwrap takes **250–500 ms** on the owner's machine, and reports the measured
figure and the chosen tuple in the implementation report. A number nobody
measured is not a security parameter.

The DEK is 256 bits and is handed to SQLCipher as a **raw key**, which bypasses
SQLCipher's own KDF. The key pragma is the first statement on every connection;
chaining `journal_mode` or `foreign_keys` before it is a documented way to get
`file is not a database`.

**The backoff is a flat 30 seconds**
([ADR-0011](./adr/0011-flat-backoff.md)):

```text
attempts_remaining = max(0, 5 - failed_attempts)
wait_ms            = 30_000                        for failed_attempts ≥ 5
```

The fifth wrong PIN waits 30 seconds. So does every attempt after it. There is
no doubling and no ceiling, because there is nothing to cap.

`locked_until_unix_ms` records when the current wait ends.

**Why both values are still persisted**, now that neither carries an exponent:

| Value | Persisted because |
| ----- | ----------------- |
| `failed_attempts` | Without it, five attempts becomes unlimited. A user could guess four times, relaunch, guess four more, and never reach a backoff at all. |
| `locked_until_unix_ms` | Relaunching FastClip takes less than 30 seconds, so a deadline held only in memory could be skipped by restarting — which is the one thing a person at the keyboard can always do. |

Both live inside the DPAPI blob, so tampering with either requires the Windows
account that [ADR-0002](./adr/0002-threat-model.md) already concedes to an
attacker running as the user.

### Who owns lock state

| Fact | Owner | Read from |
| ---- | ----- | --------- |
| Is the store encrypted? | The `clips.db` file itself. | The classification ([above](#classifying-clipsdb)), held in memory. It is a **four**-state answer, not a boolean: `absent`, `plaintext`, `encrypted`, `unreadable`. Nothing may treat "not `plaintext`" as `encrypted`, or "not readable" as `absent`. Established at startup and **replaced at exactly one other point**: a conversion's commit ([below](#the-classification-is-written-twice)). |
| Is it unlocked? | The backend process, in memory. | Set when a PIN successfully unwraps the DEK, cleared by [`lock`](./contract.md#lock); never persisted. Every launch starts locked. |
| How many attempts remain, and until when? | `keyfile`. | Persisted, so restarting FastClip does not clear a backoff. |

The database file is the source of truth for *encrypted or not*; `keyfile` is
subordinate. A `keyfile` beside a plaintext database is a leftover from an
interrupted conversion: it is ignored and removed. This makes the state
single-owner and makes both conversions crash-safe with one commit point.

Rejected: treating the presence of `keyfile` as the encryption flag. It gives
two files that can disagree, with no way to tell which is right.

#### The classification is written twice

The classification is a value in memory, not a fact re-derived on each read. It
is **written at exactly two points**:

| Written | To |
| ------- | -- |
| Startup recovery step 3 | whichever of the four states the header read established |
| A conversion's commit — step 6, the rename | `encrypted` after `enable_encryption`, `plaintext` after `disable_encryption` |

**The second write is not optional, and omitting it breaks the ordinary path with
no fault involved.** Encryption off, so the classification is `plaintext`. The
user enables encryption; it succeeds and emits `locked: false`, and the frontend
shows the Lock control. The user presses Lock. [`lock`](#locking-on-demand) reads
lock state after taking the mutex, from this value — which would still say
`plaintext` — and returns `wrong_state { required: "encrypted" }`, an error
[the contract](./contract.md#lock) describes as unreachable in normal use. The
user could neither lock nor disable the encryption they had just enabled until
they restarted.

It is written **at** the commit rather than after the whole command, because the
rename is the instant the fact changes. A conversion whose step 7 reopen fails
still committed, and the classification must already describe the store as it now
is — the same reason step 8's emission is unconditional
([above](#the-emission-is-not-conditional-on-the-command-succeeding)).

An aborted conversion writes nothing: it never reached step 6, and the store is
byte-for-byte what the startup read classified.

### Locking on demand

[`lock`](./contract.md#lock) is the only way the store becomes locked while the
process is running. It is manual only: there is no idle timeout, and adding one
is a specification change before it is a code change.

**What locking does, in this order:**

1. **Acquire the store's connection mutex** — the same one every command takes
   ([connections](#connections)). This waits for any command already executing to
   finish, so no transaction is interrupted and no committed write is lost.
2. Read lock state, **after** taking the mutex and not before. Encryption off →
   release and return `wrong_state { required: "encrypted" }`. Already locked →
   release and go to step 5. Reading it first would let a
   `disable_encryption` that was already running complete in between, and `lock`
   would then lock a store that had just become plaintext.
3. `PRAGMA wal_checkpoint(TRUNCATE)`, then close the SQLCipher connection.
4. Zeroise the in-memory DEK and clear the in-memory unlocked flag. Release the
   mutex.
5. Rebuild the tray menu to the single *Unlock FastClip* item
   ([spec §4.8](../product/spec.md#48-encryption-and-unlocking)), emit
   `lock_state`, return.

`lock` therefore waits behind a long `import_clips` rather than pre-empting it.
That is the cost of "no transaction is interrupted", and it is bounded by the
longest single command.

`keyfile` is not written. `failed_attempts` is 0 — the successful unlock that
must have preceded this reset it — and stays 0. Locking is not a failed attempt
and starts no backoff.

**Steps 3 and 4 cannot fail in a way the user is told about.** If the checkpoint
or the close errors, the DEK is zeroised and the flag cleared regardless, the
failure is logged ([ADR-0012](./adr/0012-logging.md)), and `lock` still returns
success. Refusing to lock because a disk operation failed
would leave the clips on screen, which is what the user pressed the control to
prevent. The residue discloses nothing: with encryption on, SQLCipher encrypts
the `-wal` file too.

**Why the connection closes rather than a flag being flipped.** With the
connection open and the DEK live, "locked" would mean only that the backend
declines to answer, and any defect anywhere in the backend would re-expose every
clip with no PIN. Closing it makes the process genuinely unable to read the
store, which is what the word claims to a user who pressed a button labelled
Lock. The cost is that `unlock` afterwards pays the Argon2id unwrap again —
250–500 ms, [above](#argon2id-parameters-and-the-backoff) — and reopens the
database. That is the launch path exactly, so it is not new code.

Rejected: keeping the connection open behind a boolean. It is cheaper, it keeps
`unlock` instant, and it means less.

**Every clip-touching command re-checks lock state after acquiring the mutex.** A
command checks on entry, before it queues for the connection; one that passed
that check and then waited behind `lock` would otherwise wake to a closed
connection and report `storage` or `internal` for what is an ordinary lock. The
second check means a concurrent `lock` produces exactly two outcomes — the
command completed before `lock` took the mutex, or it returns `locked`. It never
produces a third *from the lock*. A command can still fail on its own merits at
the same moment, for a reason that has nothing to do with locking
([contract](./contract.md#lock)).

That check belongs in one shared guard rather than in sixteen command bodies.
[WP-03](../work/wp-03-storage.md) and [WP-05](../work/wp-05-crud.md) build those
commands before encryption exists, so the guard must be a single function from
the start. Retrofitting a re-check into each of the ten commands marked "works
while locked: no" in [contract §2](./contract.md#2-commands) is how one gets
missed.

### Switching encryption on and off

Use SQLCipher's `sqlcipher_export()` to convert between a plain and an encrypted
database rather than hand-rolling read-decrypt-write.

Both conversions use the same shape: build the new database beside the old one,
**verify its contents**, remove the old database's sidecars, rename the new one
into place as the single commit point, reopen the store, emit `lock_state`, and
delete every intermediate file whatever happens.

**`sqlcipher_export()` does not carry `PRAGMA user_version` across.** It copies
schema and data; `user_version` is a header field and is not either. Every
conversion therefore **writes it explicitly** onto the new database. Without that
write, both conversions produce a store at `user_version = 0` — indistinguishable
from a database that never set one — and
[the migrate-don't-reject rule](./contract.md#versions-on-disk) has nothing to
apply, so every converted store looks like it needs migrating from version zero
forever.

Enabling:

1. Generate a random 256-bit DEK. Wrap it, DPAPI-protect it, write `keyfile.new`,
   flush, rename to `keyfile`. **Then read `keyfile` back, unwrap it with the same
   PIN, and assert the recovered DEK equals the one generated.** Failure → abort.
   This is the same class of pass condition as step 3 and it protects the same
   thing: key material that cannot be unwrapped is discovered here, where the
   store is still plaintext and nothing has been lost, rather than after the
   commit point, where it would leave an encrypted store whose only key does not
   open it.
2. Record `n = SELECT count(*) FROM clips` and `v = PRAGMA user_version` from the
   **source**. Then `sqlcipher_export()` into `clips.db.new`, keyed with the DEK,
   and `PRAGMA clips_new.user_version = v`.
3. **Open `clips.db.new` with the DEK and verify it.** Key pragma first, then
   three assertions ([below](#what-step-3-asserts)). Any assertion failing, or
   the open failing → abort.
4. `PRAGMA wal_checkpoint(TRUNCATE)` on both databases, then close both
   connections.
5. **Delete `clips.db-wal` and `clips.db-shm`.** They are the *plaintext*
   sidecars. Failure → abort.
6. Rename `clips.db.new` over `clips.db`. **Commit point.** Set the
   classification to `encrypted` here, not at the end of the command
   ([below](#the-classification-is-written-twice)).
7. Reopen `clips.db` with the DEK and hold the connection
   ([connections](#connections)).
8. **Emit `lock_state`, whether or not step 7 succeeded**
   ([below](#the-emission-is-not-conditional-on-the-command-succeeding)).
9. Delete `clips.db.new-wal` and `clips.db.new-shm` if the close left any.
   Absorbed on failure. Return `null`, or `storage` if step 7 failed.

Disabling has the same steps with the roles swapped. It has no step 1 — there is
no key to mint — and it deletes `keyfile` at step 9, after the commit point
rather than before it:

1. —
2. Record `n` and `v` from the source, `sqlcipher_export()` into a plaintext
   `clips.db.new`, and write `user_version = v` onto it.
3. Open `clips.db.new` and verify it, as above. Failure → abort.
4. Checkpoint both, close both.
5. Delete the **encrypted** `clips.db-wal` and `clips.db-shm`. Failure → abort.
6. Rename `clips.db.new` over `clips.db`. **Commit point.** Set the
   classification to `plaintext` here.
7. Reopen the now-plaintext `clips.db` and hold the connection.
8. **Emit `lock_state`, whether or not step 7 succeeded.**
9. Delete `keyfile`. Absorbed on failure. Return `null`, or `storage` if step 7
   failed.

#### What step 3 asserts

"Verify it" is three comparisons against values recorded at step 2, not three
statements executed and discarded. A `user_version` read and not compared, or a
row count taken and not compared, verifies only that the file opens — and a file
that opens with the wrong contents then reaches the commit at step 6 with the
abort path never taken.

| Assertion | Passes when | Catches |
| --------- | ----------- | ------- |
| `PRAGMA integrity_check` | returns exactly `ok` | a structurally damaged copy |
| `SELECT count(*) FROM clips` | equals `n` from step 2 | a truncated or partial export |
| `PRAGMA user_version` | equals `v` from step 2 | the explicit write above having been skipped or lost |

All three must pass. Any one failing is an abort, with the store untouched.

**Abort means abort to the state before the command was called**, and it is
reachable at steps 1 to 5, all of which precede the commit point. Delete
`clips.db.new` and its sidecars, delete `keyfile.new`, delete `keyfile` on the
enable path — where it was written before the commit point — **reopen the
original `clips.db` and hold the connection**, and return the error. The store is
byte-for-byte what it was and the application is still usable. A recovered
failure that left the user unable to copy a clip until they restarted would be a
worse outcome than the failure.

#### The reopen, and why it is verified first

Step 3 opens and exercises the new database while it is still an intermediate
file. Step 7 opens the same bytes again under a different name.

Without step 3, a conversion that produced a damaged or short database would only
discover it after the commit point, with the store already replaced. With step 3
asserting integrity, row count and version, a step 7 failure is an *access*
failure rather than a content one — the file was proved good moments earlier, so
what remains is the file being unopenable at that instant: an antivirus or backup
handle, a second instance, a disk fault.

**If step 7 fails**, the conversion has committed and the store is in its new
state on disk. The command returns `storage`, and the backend serves `storage`
from every clip-touching command for the rest of the session rather than opening
on demand ([connections](#connections)). The next launch reaches the store
through [startup recovery](#startup-recovery), which classifies it and reports
the fault properly. `storage` is already in the declared error set of every
clip-touching command, so this needs no new variant and no new frontend branch.

#### The emission is not conditional on the command succeeding

**Step 8 emits `lock_state` whether or not step 7 succeeded**, on both paths.

The rename at step 6 is the commit point: it is the instant the store's state
changes, and after it the database header — the single source for "is the store
encrypted?" — already gives the new answer. The event reports **the state of the
store**, not the outcome of the call.

An earlier version of this page tied the emission to the command succeeding and
then argued that only the enable path could misreport, because the disable path's
post-commit work was absorbed. **That was false about its own sequence.** The
reopen is post-commit, precedes the `keyfile` delete, and is explicitly not
absorbed. A reopen failure on the disable path would have returned `storage` with
no emission, leaving the settings view claiming encryption is **on** over a
plaintext store for the rest of the session — the exact state
[contract §4](./contract.md#disable_encryption) calls
[spec §5](../product/spec.md#5-security-posture) asserted backwards.

Emitting unconditionally removes the asymmetry rather than arguing about which
direction of it is tolerable. Neither path can misreport, because neither path
decides whether to speak.

The frontend applies a complete `LockState` and is idempotent, so an emission
followed by a rejected command costs it nothing: it shows the true store state
and the error for the operation that partly failed.

#### Failures after the commit point are absorbed, not reported

**Step 9 cannot fail either conversion** — deleting `keyfile` on the disable
path, deleting `clips.db.new-wal` and `clips.db.new-shm` on the enable path. Log
it ([ADR-0012](./adr/0012-logging.md)) and continue. The return value is decided by step 7 alone.

Step 7, the reopen, is **not** in the absorbed set: it decides whether the
application can serve clips, so it is reported. Step 8's emission happens
regardless of it
([above](#the-emission-is-not-conditional-on-the-command-succeeding)), which is
what stops a reported step 7 from suppressing the state change a committed
conversion already made.

This is the rule [`lock`](#locking-on-demand) already follows, for the same
reason. The conversion has committed: the store on disk is in its new state, and
the database header already says so. Propagating a failed `keyfile` delete with
`?` would return `storage` for a conversion that worked and left a stray file
that the next launch removes anyway.

The stray `keyfile` is not left dangling: startup recovery deletes a `keyfile`
beside a `plaintext` store at the next launch, which is what that step is for.

**Step 5 is why the ordering is written down.** Sidecars are deleted *before* the
rename, never after. SQLite unlinks them on the last connection's clean close,
and this build has no single-instance guard
([inherited debt](../reference/debt.md)), so a second process holding the
database open can leave them in place — which is why the explicit delete exists
at all, and why a failure to delete aborts rather than continues. Deleting after
the rename leaves a window in which an **encrypted `clips.db` sits beside a
plaintext `clips.db-wal` holding every label and value, readable in a text
editor**, in breach of
[acceptance criteria 5 and 6](../product/spec.md#8-acceptance-criteria). With the
delete before the rename there is no such instant: every kill point leaves either
a plaintext database with no sidecars or an encrypted one with no sidecars.

Deleting the checkpointed sidecars costs no durability. Step 4 wrote the WAL's
contents into the database file and closed the connection, so the sidecars hold
nothing the database does not.

**The rename in step 6 is the commit point**, and it is the only instant at which
the store's state changes.

| Killed at | Leaves | Recovered by |
| --------- | ------ | ------------ |
| Enable, before its sidecar delete | Plaintext `clips.db` with its own legitimate sidecars, a stray `keyfile`, possibly an encrypted `clips.db.new` | Startup recovery steps 2 and 5 delete the intermediates and the `keyfile`. The store is plaintext, which is what it was. Its sidecars are its own and are left alone. |
| Enable, between the sidecar delete and the rename | Plaintext `clips.db`, **no sidecars**, a stray `keyfile`, an encrypted `clips.db.new` | As above. The missing sidecars cost nothing; they were checkpointed. |
| Enable, after the rename — including before the reopen at step 7 | Encrypted `clips.db` with its `keyfile`, no plaintext sidecar of any kind, and possibly an encrypted `clips.db.new-wal` | Startup recovery step 2 sweeps the stray sidecar. Nothing else to do: no ordering leaves an encrypted database with no key, and none leaves it with a plaintext WAL. The missing reopen does not survive the kill — the next launch opens the store normally. |
| Disable, before its sidecar delete | Encrypted `clips.db`, its encrypted sidecars, its `keyfile`, and **a complete plaintext `clips.db.new`** | Startup recovery step 2 deletes `clips.db.new`. |
| Disable, between the sidecar delete and the rename | Encrypted `clips.db`, no sidecars, its `keyfile`, and a plaintext `clips.db.new` | As above. |
| Disable, after the rename but before the `keyfile` delete | Plaintext `clips.db`, no sidecars, a stray `keyfile` | Startup recovery step 5 deletes the `keyfile`. |

**Every pre-commit failure path aborts as defined above** — an error, a
rolled-back export, a validation failure, a refused delete, any of them. The
startup sweep exists for the one case that cannot run a cleanup at all: the
process not returning.

The fourth row of that table is the reason the sweep is mandatory rather than
housekeeping. Without it, an interrupted `disable_encryption` leaves every clip
in the clear next to a store the user still believes is encrypted, indefinitely,
in breach of the same two criteria.

The conversion is also why `disable_encryption` requires the store unlocked: the
plaintext export needs the DEK, and the PIN it also requires is what proves the
user meant it.

### What the user sees when the store cannot be opened

A blank window is not acceptable. Every failure below maps to a variant in
[contract §4](./contract.md#4-errors) and to a sentence in the
[copy deck](../product/copy.md).

| Situation | Variant | Reported by |
| --------- | ------- | ----------- |
| Wrong PIN | `bad_pin { attempts_remaining, retry_after_ms }` | `unlock` |
| An unlock attempted while a wait is running | `backoff { retry_after_ms }` | `unlock` |
| `keyfile` missing, unreadable, or from another Windows account or machine | `crypto { reason: "bad_key_material" }` | `get_lock_state` at launch; `unlock`, `disable_encryption` or `change_pin` on any later read |
| Key unwrapped, database will not open or fails its integrity check | `crypto { reason: "corrupt" }` | `unlock` with encryption on. With it off, recorded at startup and relayed by whichever command next needs the store — `get_lock_state` first if the frontend follows the startup sequence |
| `user_version` newer than this build | `unsupported_version { component: "schema" }` | as the row above |
| `keyfile` version newer than this build | `unsupported_version { component: "key_material" }` | `get_lock_state` at launch; `unlock`, `disable_encryption` or `change_pin` on any later read — the same three readers as the row above |
| Home directory unresolvable, `~/.fast-clip/` not creatable or not reachable | `storage` | `get_settings`, the first command to need the directory, and then `get_lock_state` for the same reason |
| `clips.db` absent and not creatable, permissions, disk full | `storage` | `get_lock_state`. `get_settings` does not touch the database. |
| **`clips.db` present but its header cannot be read** — permissions, a sharing violation, a second instance, a transient I/O error | `storage` | `get_lock_state`, from the `unreadable` classification ([above](#classifying-clipsdb)) |

The third column says where the user meets each fault, not which command is
permitted to carry it. For the two rows that are recorded at startup, **any
command needing the store relays the same recorded value**; naming
`get_lock_state` records that it is first in the
[startup sequence](./contract.md#startup-sequence), which is why that sequence
can have one failure branch per step. A variant that a command could not have
obtained — `bad_pin` from `list_clips`, say — is a backend defect.

Every one of these messages points at
[export and import](../product/spec.md#46-export-and-import-json) as the
recovery path, because after [ADR-0005](./adr/0005-sqlite-store.md) it is the
only one — **except the last**. An `unreadable` store is not damaged and must not
be described as though it were. Nothing was deleted, nothing was created, and
closing the other process or fixing the permission recovers it whole. The
[copy deck](../product/copy.md) needs a sentence for this that does **not**
suggest re-importing, because acting on that advice over a locked file is how a
user replaces a store that was intact.

## Open — the crate

**Which crate provides SQLite and SQLCipher.** Deferred to
[ADR-0005](./adr/0005-sqlite-store.md) on the evidence of the
[WP-02](../work/wp-02-toolchain.md) spike, which must open an encrypted database
on `windows-latest` in CI, write a row, close, reopen with the key, read the row
back **and assert it equals what was written**, and confirm that reopening with
the wrong key fails.

**The spike's first attempt did not produce that evidence.** It ruled out
`rusqlite 0.40.1` on stable Rust — a `libsqlite3-sys 0.38.1` defect, not a
SQLCipher one — and established that the crate version must be pinned whichever
crate wins. Everything else it reported is a local environment failure. The
detail is in [ADR-0005](./adr/0005-sqlite-store.md).

Nothing on this page depends on the answer except two things, both of which the
spike must report:

- the bundled SQLite version, which decides whether `STRICT` tables are
  available
- the build time and binary size cost of the bundled C compile

`architect` records the decision in ADR-0005 once the spike reports.
`backend-dev` does not pick a crate.
