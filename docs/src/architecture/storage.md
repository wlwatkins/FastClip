# Storage and encryption

**Status:** Not yet specified. `backend-dev` drafts, `architect` ratifies.

This page holds no decisions yet. It exists so the gap is visible in the table
of contents rather than discovered during implementation.

Read [ADR-0002](./adr/0002-threat-model.md) first. It rules work out as well as
in.

## Current state

`DataBase` is a `HashMap<Uuid, Clip>` serialised as pretty-printed JSON to
`%LOCALAPPDATA%\FastClip\db`. `save()` writes over the live file, so a crash
mid-write truncates it and the user loses every clip.

## Required properties

| Property | Reason |
| -------- | ------ |
| Ordered | The specification requires user ordering; a `HashMap` cannot express it. |
| Encrypted at rest | [ADR-0002](./adr/0002-threat-model.md). AEAD, not hand-rolled. |
| Keyed from Windows DPAPI | Not a passphrase. Limits are stated in ADR-0002. |
| Versioned | A format version from day one, so key rotation and schema change stay possible. |
| Crash-safe | Write to a temporary file, fsync, rename. Never over the live file. |
| Migratable | Read the existing plaintext file once, drop removed fields, map Mantine colours to tokens, write back encrypted. |

## The migration

This is where user data gets destroyed. It runs once, on a machine you cannot
inspect, against a file you did not write.

`test-engineer` writes tests against a real pre-refactor database file — one
produced by the current build and checked in as a fixture, not a synthesised
one.

Cases to cover: an empty store, an already-migrated store, a truncated file, a
file with unknown extra fields, and a file containing every Mantine colour name
in use.

## Open

- Which AEAD construction, and which crate?
- Where does the DPAPI-protected key blob live relative to the ciphertext?
- Is the SurrealDB dependency used here or removed? It is declared in
  `Cargo.toml` and imported nowhere; an unused database engine costs compile
  time and audit surface.
- What does the user see when the store cannot be decrypted? A blank window is
  not acceptable. The failure message points at
  [export and import](../product/spec.md) as the recovery path.
