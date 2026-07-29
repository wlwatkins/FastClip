# WP-07 — Optional PIN-gated encryption

**Objective:** a user who wants their store encrypted can turn it on, protected
by a 6-digit PIN. A user who does not is unaffected.

**Depends on:** [WP-03](./wp-03-storage.md) and
[WP-09](./wp-09-export-import.md). Export must exist before encryption is
offered — the opt-in warning is worthless if the escape hatch it points at has
not been built.

**Inputs:** [ADR-0004](../architecture/adr/0004-optional-pin-encryption.md),
[ADR-0002](../architecture/adr/0002-threat-model.md),
[spec §4.8](../product/spec.md#48-encryption-and-unlocking),
[storage design](../architecture/storage.md).

Read ADR-0004 first. It rules work out as well as in: encryption is **off by
default**, the IPC boundary is **not** hardened, and the PIN is a gate rather
than the entropy.

## Work

### backend-dev

Generate a random 256-bit DEK when the user enables encryption. SQLCipher
encrypts the database with it ([ADR-0005](../architecture/adr/0005-sqlite-store.md)).
Wrap the DEK as `AEAD(key = Argon2id(PIN, salt), DEK)`, then protect that blob
with DPAPI.

SQLCipher replaces the cipher plumbing, not the key policy. Apply the key pragma
before any other pragma on every connection — chaining other pragmas first is a
known way to get `file is not a database`.

**Both factors are required.** A design where DPAPI alone can unwrap the DEK
turns the PIN into decoration; one where the PIN alone can turns 10⁶ guesses
into an offline attack. If your implementation can open the store with only one
of them, it is wrong.

Implement enable, disable, change-PIN, and unlock. Changing the PIN re-wraps the
DEK and does not re-encrypt the database. Enabling and disabling use
`sqlcipher_export()` to convert between plain and encrypted rather than
hand-rolled read-decrypt-write — a process killed mid-conversion leaves a
readable database in one state or the other, never half-converted.

Track lock state and return `locked` from every clip-touching command while
locked. After five wrong PINs, back off exponentially from 30 seconds.
**Never wipe anything after failed attempts.**

Never log the PIN, the DEK, the salt, or any wrapped blob.

### frontend-dev

The settings flow to enable, disable and change the PIN. The enable path shows
the irreversibility warning from the
[copy deck](../product/copy.md) — a forgotten PIN means the clips are gone —
and offers an export first.

The launch PIN prompt. Nothing is listed, searched or copied until it is
entered. Handle `locked` on every command, not only at launch.

Wrong-PIN and backoff states with the attempts remaining and the wait.

### backend-dev — tray

While locked, the tray menu shows a single *Unlock FastClip* item and no clip
labels. Choosing it raises the window with the PIN prompt focused. The PIN is
never typed into a native menu.

### test-engineer

Round-trip with encryption on: enable, restart, unlock, clips intact.

**Both factors are necessary.** Construct the case where the PIN is known but
the DPAPI blob is absent, and the case where DPAPI is available but the PIN is
wrong. Both must fail. This is the acceptance test for ADR-0004 and it is the
one that catches a design that only looks encrypted.

Wrong PIN five times triggers backoff and destroys nothing. Enable interrupted
mid-write leaves a readable store. Disable requires the PIN. Change PIN keeps
the clips readable and invalidates the old PIN.

While locked: no command returns a label or a value, and the tray exposes no
clip name.

Encryption off is the default on a fresh install, and that path is unchanged
from WP-03.

### critic

Weight this package hardest.

- Can the store be opened with only one of the two factors? That is `BLOCK`.
- Does the PIN, DEK, salt or wrapped blob reach any log or stdout?
- Is any clip label or value reachable while locked, including via the tray?
- Does any failed-attempt path delete or truncate data?
- Does enabling or disabling encryption have a window where a crash loses the
  store?
- Is the README's claim about **the default**, which is unencrypted?

## Definition of done

- Encryption is off on a fresh install and the app never prompts.
- With it on, the store is unreadable in a text editor and needs both the
  Windows account and the PIN.
- A locked FastClip discloses no label or value anywhere.
- Five wrong PINs back off; nothing is destroyed.
- Enable, disable and change-PIN are each crash-safe.

## Risks

The failure mode to fear is a design that appears encrypted but is not — DPAPI
alone unwrapping the DEK, or a PIN-only KDF. Both pass a casual review and both
leave the file openable by whoever holds it. The two-factor test above exists
because reading the code is not enough to catch this.

Second: a forgotten PIN is unrecoverable by design. The warning at enable time
is not boilerplate, it is the only mitigation.
