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

### Spike evidence, first attempt — the crate remains open

**The spike did not succeed and no crate is chosen.** The
open-write-close-reopen-with-key-reopen-with-wrong-key cycle was never executed,
on any crate. No build time and no binary size were measured, because no attempt
reached a linked binary. The machine lacked the MSVC C++ build tools and a
suitable Perl, so most of what follows is a local environment failure rather
than a verdict on a crate. It is recorded here because two results are
independent of that machine and one of them is binding.

| Result | Status | Binding |
| ------ | ------ | ------- |
| `rusqlite 0.40.1` resolves `libsqlite3-sys 0.38.1`, whose `build.rs` fails to compile on stable `rustc 1.94.0`: `error[E0658]: use of unstable library feature 'cfg_select'` | **Ruled out.** A crate defect, reproducible anywhere on stable. | **Yes.** Whichever crate is chosen, **pin the version.** A bare `cargo add rusqlite` reintroduces a build failure that has nothing to do with SQLCipher and reads like one. `sea-orm` reaches `libsqlite3-sys` too, so this is not specific to `rusqlite`. |
| `rusqlite 0.32` + `bundled-sqlcipher` fails with "Missing environment variable OPENSSL_DIR" | **Genuine cost signal**, reproducible. | It would need an OpenSSL install step in CI. |
| `rusqlite 0.32` + `bundled-sqlcipher-vendored-openssl` vendors OpenSSL and invokes `perl ./Configure` with `no-asm`, needing neither a pre-existing OpenSSL nor NASM. It failed only on the local Perl distribution missing `Locale::Maketext::Simple`. | **Unproven hypothesis.** GitHub's `windows-latest` ships Strawberry Perl, so this path is *expected* to work there. Expected is not demonstrated. | No. Do not build on it. |
| `sea-orm` | **Not evaluated at all.** The time box went to bisecting the `rusqlite` failures. | No. |

The requirement above is unchanged: the cycle must run on `windows-latest` in
CI before any code depends on the link. That needs the workflow committed and
pushed, which is the owner's call.

### Spike evidence, second attempt — crate chosen

**Crate:** `rusqlite`, pinned to `0.32`, feature `bundled-sqlcipher-vendored-openssl`.

```toml
rusqlite = { version = "0.32", features = ["bundled-sqlcipher-vendored-openssl"] }
```

**Do not `cargo update` or bump this past `0.32` without re-running the spike.**
`rusqlite 0.40.1` resolves `libsqlite3-sys 0.38.1`, whose `build.rs` fails on
stable `rustc 1.94.0` with `error[E0658]: use of unstable library feature
'cfg_select'`. That failure is a crate defect independent of SQLCipher and is
still reproducible; it is the reason for the pin, not an incidental choice of
version.

With Strawberry Perl on `PATH` ahead of Git's Cygwin Perl, the local blocker
from the first attempt (`Locale::Maketext::Simple` missing) is gone. The
`no-asm` vendored-OpenSSL build ran to completion.

**Cycle proven, locally, on this machine:** open an encrypted database with a
key, write a row, close, reopen with the same key, read it back, reopen with
the wrong key, confirm the read fails. Two independent test files exercise
this — `spikes/sqlcipher-spike/tests/spike.rs`
(`write_close_reopen_with_correct_key_reads_back`,
`reopen_with_wrong_key_fails_to_read`) — both pass.

| Measurement | Result |
| ----------- | ------ |
| Cold build (`cargo test`, empty `target/`, `test` profile) | 15 m 52 s |
| Warm build (`cargo test` rerun, no source change) | 3 s |
| Cold build (`cargo build --release`) | 16 m 19 s |
| Binary size impact | A no-op `println!` binary linking `rusqlite` + `bundled-sqlcipher-vendored-openssl` is 4,674,048 bytes (release, stripped by default profile settings only — no explicit `strip`). A no-dependency `println!` baseline binary is 129,536 bytes. Delta: **~4.3 MiB** attributable to the vendored SQLCipher/OpenSSL static link. |

**This evidence is from a local Windows 11 machine (`rustc 1.94.0`, MSVC 14.44
via VS 2026 Professional, Strawberry Perl 5.42.2), not from `windows-latest` in
GitHub Actions.** The requirement above — that the cycle run in CI before any
application code depends on the link — is **still open**. What still needs
confirming there specifically:

- That `windows-latest`'s preinstalled Strawberry Perl (not Cygwin's) is what
  `cargo` picks up by default, without a `PATH` workaround.
- That the MSVC toolchain preinstalled on `windows-latest` links the vendored
  OpenSSL and SQLCipher without the local machine's specific VS 2026 install.
- Cold and warm build times under CI's actual hardware and network, with and
  without a cached `~/.cargo` registry and `target/`, since the ~16-minute cold
  numbers above make an uncached Rust CI job impractical.

`sea-orm` was not evaluated in this attempt either, and ADR-0005's original
order made `rusqlite` the fallback only if `sea-orm` failed the time box — a
comparison that was never run, since the time box went to reproducing the
cycle on `rusqlite` after the first attempt ruled out `rusqlite 0.40.1`. The
crate choice above rests on the architect's WP-01 argument at line 83 plus
this `rusqlite`-only feasibility spike, not on a head-to-head comparison
against `sea-orm`.

**Nothing in the [contract](../contract.md) or the
[storage design](../storage.md) depends on the answer**, and both were ratified
at G0b without it. Two things are downstream of it and both are named in the
[storage design](../storage.md): whether the bundled SQLite is new enough for
`STRICT` tables, and the build-time and binary-size cost of the C compile.

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
