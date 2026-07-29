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
locked. After five wrong PINs, every further attempt waits a flat 30 seconds —
no escalation, no ceiling. **Never wipe anything after failed attempts.**

Implement `lock` per [contract §2](../architecture/contract.md#2-commands) and
the "Locking on demand" section of the
[storage design](../architecture/storage.md). Take the connection mutex first
and read lock state second. Close the database and zeroise the DEK rather than
flipping a boolean, so `unlock` afterwards is the launch path exactly. Rebuild
the tray before returning. Locking never fails on I/O — a failed checkpoint or
close is absorbed, and the DEK is zeroised regardless.

Never log the PIN, the DEK, the salt, or any wrapped blob.

### frontend-dev

The settings flow to enable, disable and change the PIN. The enable path shows
the irreversibility warning from the
[copy deck](../product/copy.md) — a forgotten PIN means the clips are gone —
and offers an export first.

The launch PIN prompt. Nothing is listed, searched or copied until it is
entered. Handle `locked` on every command, not only at launch.

Wrong-PIN and backoff states with the attempts remaining and the wait.

The manual lock control, hidden when `encryption_enabled` is false. On
`locked: true` from any source, discard the clip list, close any open form and
clear the search query, and ignore any `update_clips` that arrives afterwards.

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

Manual lock: locking while unlocked leaves no command returning a label or
value and the tray showing only *Unlock FastClip*; locking while already locked
succeeds; locking with encryption off returns `wrong_state`; a mutation issued
concurrently with a lock either commits in full or returns `locked`, never
half; after a lock, the process cannot read the store until the PIN is
re-entered.

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
- A manual lock discloses nothing, in the window or the tray, and the process
  cannot read the store until the PIN is re-entered.
- **Every control this package adds is reachable by `Tab`, operable by `Enter`
  and `Space`, carries an accessible name, and shows a focus indicator distinct
  from hover.** That includes the PIN entry, the enable, disable and change-PIN
  flows, and the lock control. The PIN prompt is the first thing a user meets at
  launch; a mouse-only PIN entry locks a keyboard user out of their own store.

## Carried from review 003

[Review 003](../reviews/003-wp-01-contract.md) accepted the contract with four
findings carried rather than fixed in a fifth round. Three land here, and the
`architect` writes the missing sentences into
[storage](../architecture/storage.md) as part of this dispatch, before
`backend-dev` implements the steps.

**F2 [minor] — the conversion abort deletes `clips.db.new` with no requirement to
close the connection step 3 opened.** Reachable when `disable_encryption` fails
step 3's verification, which is the case that step exists to catch. Abort runs
before step 4's close, and deleting a file with a live SQLite handle fails on
Windows with a sharing violation — leaving a complete plaintext copy of every
label and value beside an encrypted `clips.db` for the session, while the user is
told the operation failed and still believes the store is encrypted. The natural
Rust structure drops the connection first; the document specifies this
handle-and-delete interaction at step 5 and is silent here.

**F3 [minor] — abort is enumerated as reachable at steps 1 to 5; a failed rename
at step 6 is a sixth.** `storage.md:755-756` covers it and the abort section's own
enumeration does not, so two statements disagree about step 6. The harmful branch
— setting the classification to `encrypted` without checking the rename, then
letting step 8's unconditional emission report encryption over a plaintext store
— needs a deliberately discarded `Result`.

**F4 [minor, and re-routed here] — `lock` step 5's tray rebuild has no defined
outcome.** Steps 3 and 4 absorb their failures by name; step 5 is not in that
set, and both available behaviours break a stated rule. Absorb, and the tray
keeps listing clip labels over a locked store — acceptance criterion 10 breached
in the one surface the contract says breaches it exactly as the window would.
Report, and `lock`'s error set is stated in full as `wrong_state { encrypted }`,
so the only route is `internal`, contradicting ADR-0010's "Once locking begins it
cannot fail".

Review 003 routed F4 to WP-03. It is here instead: `lock` does not exist until
encryption does, and this package already owns both `lock` and the
tray-while-locked surface. The finding is **stated as a prediction** — whether
Tauri 2's tray menu setter is fallible was not observable from a static read. If
it is infallible, F4 is void; confirm that before writing the sentence.

## Risks

The failure mode to fear is a design that appears encrypted but is not — DPAPI
alone unwrapping the DEK, or a PIN-only KDF. Both pass a casual review and both
leave the file openable by whoever holds it. The two-factor test above exists
because reading the code is not enough to catch this.

Second: a forgotten PIN is unrecoverable by design. The warning at enable time
is not boilerplate, it is the only mitigation.
