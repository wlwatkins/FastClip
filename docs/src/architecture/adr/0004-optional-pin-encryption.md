# ADR 0004 — Optional PIN-gated encryption

**Status:** Accepted. Supersedes the encryption decision in
[ADR-0002](./0002-threat-model.md); that ADR's IPC reasoning still stands. The
**backoff** consequence below is superseded by
[ADR-0011](./0011-flat-backoff.md), which makes the wait a flat 30 seconds;
everything else here stands.
**Deciders:** owner

## Context

[ADR-0002](./0002-threat-model.md) decided that the store is always encrypted at
rest, keyed from Windows DPAPI, with no passphrase — on the grounds that a
prompt is wrong for a click-to-copy speed tool.

The owner wants encryption to be **opt-in**, gated by a 6-digit PIN entered at
launch. Most clips are not secrets; paying a prompt on every launch to protect
a list of email signatures is a bad trade, and the user is the one who knows
which case they are in.

### The problem a PIN creates

A 6-digit PIN has 10⁶ possible values. If the PIN alone derives the key, an
attacker who copies the file can try every one offline. At a deliberately slow
100 ms per guess that is roughly a day on one core, and far less on a GPU.

The threat ADR-0002 exists to defend is precisely *the file leaving the
machine* — profile sync, backup images, a disk without BitLocker. A
PIN-derived key does not defend it. It only appears to.

## Decision

Encryption is **opt-in, off by default**, and when enabled the key requires
**both a 6-digit PIN and Windows DPAPI**.

### Key construction

1. A random 256-bit data encryption key (DEK) is generated when the user
   enables encryption. The store is encrypted with the DEK using an AEAD
   construction from an established crate.
2. The DEK is wrapped: `AEAD(key = Argon2id(PIN, salt), DEK)`.
3. That wrapped blob is then protected with **DPAPI**, bound to the Windows
   user account.

Opening the store requires the file, the Windows account, and the PIN. Two of
three is not enough.

This is the shape Windows Hello uses: the PIN is not the entropy, it is the
gate. The entropy is the random DEK, and machine-binding is what makes 10⁶
guesses worthless to someone holding only the file.

### Consequences of that construction

- **A forgotten PIN means the clips are unrecoverable.** There is no reset and
  no backdoor, because any reset that works without the PIN means the PIN
  protects nothing. The opt-in dialog says this in plain words and offers an
  export first.
- **The store cannot be moved to another machine or Windows account.** DPAPI
  binding is what makes the PIN safe; it also makes the file non-portable.
  Export is the way to move clips.
- Changing the PIN re-wraps the DEK. It does not re-encrypt the store, so it is
  instant regardless of clip count.
- Disabling encryption requires the PIN, then rewrites the store as plaintext.

## Behaviour

| State | Store | Launch |
| ----- | ----- | ------ |
| Encryption off (default) | unencrypted SQLite | opens straight to the clip list |
| Encryption on | SQLCipher-encrypted SQLite | PIN prompt before any clip is shown |

Wrong PIN attempts get a backoff after the fifth. ⚠️ This ADR specified
exponential backoff starting at 30 seconds and doubling;
[ADR-0011](./0011-flat-backoff.md) supersedes that with a **flat 30 seconds**,
and the sentence is left here rather than edited so the change is visible.
**Nothing is ever wiped after failed attempts** — a destructive lockout turns a
mistyped PIN into data loss, which is a worse outcome than the attack it would
prevent. That part is unaffected.

## What this does not protect against

Unchanged from [ADR-0002](./0002-threat-model.md): software running under the
user's account. Such software can read the DPAPI blob and log the PIN as it is
typed. FastClip is still not a password manager.

**A locked FastClip discloses nothing.** The tray menu shows a single *Unlock
FastClip* item and no clip labels — labels are content, and leaking one from a
locked application would defeat the feature. Choosing it raises the main window
with the PIN prompt focused; the PIN is never entered into a native menu.

## Consequences for the rest of the book

- [Spec §5](../../product/spec.md#5-security-posture) is rewritten. The
  user-facing claim now depends on whether encryption is on.
- The README warning must describe **the default**, which is unencrypted.
  Softening it on the strength of an opt-in feature most users will not enable
  would be false.
- [WP-07](../../work/wp-07-encryption.md) grows a settings flow, a PIN prompt,
  a locked application state, and enable/disable/change-PIN paths.
- New failure modes reach the frontend: wrong PIN, backoff active, store locked.

## Revisit if

The PIN proves too weak in practice for a user who syncs their profile to a
machine they do not control — at which point the answer is a longer passphrase,
not a longer PIN.
