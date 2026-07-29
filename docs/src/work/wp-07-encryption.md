# WP-07 — Encryption and migration

**Objective:** the store is unreadable in a text editor, and no existing user
loses a clip.

**Depends on:** WP-03.

**Inputs:** [ADR-0002](../architecture/adr/0002-threat-model.md),
[storage design](../architecture/storage.md).

Read ADR-0002 before starting. It rules work out as well as in: the IPC
boundary is **not** hardened, and no `copy_clip` command is added for security
reasons.

## Work

### backend-dev

Encrypt the store at rest with an AEAD construction from an established crate.
Do not hand-roll. Take the key from the Windows credential store (DPAPI), not
from a passphrase prompt.

Write the migration: read the existing plaintext file once, drop the removed
fields, map Mantine colour names to palette tokens, write back encrypted.

Build the migration before the crypto is finished, and get it tested first.

### test-engineer

The migration gets the most tests in this project. Use a real pre-refactor
database file produced by the current build, checked in as a fixture — not a
synthesised one.

Cases: empty store, already-migrated store, truncated file, file with unknown
extra fields, file containing every Mantine colour name in use, and a
partially-written file.

Crypto: ciphertext is not plaintext, wrong key fails cleanly, corrupted input
fails cleanly rather than panicking.

### frontend-dev

Surface a failed migration. A blank window is not acceptable. The message
points the user at import as the recovery path.

### critic

Weight this package hardest. Any path that leaves plaintext on disk, or that
can lose a clip on upgrade, is `BLOCK`.

## Definition of done

- The store is unreadable in a text editor.
- Migration from a real pre-refactor file preserves every clip.
- A decryption failure produces a message, not a blank window.

## Risks

This is the only code in the project that can destroy user data. It runs once,
on a machine nobody can inspect, against a file nobody wrote.
