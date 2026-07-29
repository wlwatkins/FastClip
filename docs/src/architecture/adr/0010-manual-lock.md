# ADR 0010 — Locking is manual, and it closes the database

**Status:** Accepted at G0b, second round, on the same basis as
[0007](./0007-list-order-representation.md) to
[0009](./0009-durability-level.md).
**Deciders:** the owner decided that manual lock is in scope; the architect
decided its shape.

## Context

[Spec §4.8](../../product/spec.md#48-encryption-and-unlocking) describes
unlocking at launch and never describes locking. The
[contract](../contract.md#when-locked-is-reachable) read that literally and
stated that the store becomes locked at launch and unlocks once, keeping the
frontend's universal `locked` handling only as insurance.

The architect escalated the gap. The owner answered at G0b: manual lock is in
scope, and it is real work rather than a footnote. Scope is **manual only** — no
idle timeout, no configurable timeout.

That answer changes an invariant three other pages relied on. The store's
unlocked state is no longer a property of the process lifetime, so the DEK's
lifetime, the database connection's lifetime, the tray's contents and the
frontend's in-memory clip list all acquire a transition they did not have.

## Decision

**A `lock` command exists, it takes no PIN, and it closes the database.**

| | |
| --- | --- |
| Trigger | The user, and nothing else. No timer, no window event, no idle detection. |
| Effect | Checkpoint, close the SQLCipher connection, zeroise the DEK, clear the in-memory unlocked flag. |
| Emits | `lock_state` with `{ encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null }` |
| On an already-locked store | Success, and the event is re-sent unchanged. |
| With encryption off | `wrong_state { required: "encrypted" }`. |
| Persists | Nothing. A manual lock writes no file and is invisible at the next launch. |

The full surface is [contract §2](../contract.md#lock); the mechanism is
[storage](../storage.md#locking-on-demand). This ADR records why, not what.

## Rationale

**Closing the database is what makes the word true.** The alternative — flip a
boolean, keep the connection open and the DEK in memory — is cheaper and keeps
`unlock` instant. It also means that "locked" is a promise the backend makes
about its own willingness to answer, so any defect anywhere in the backend
re-exposes every clip with no PIN. A user who presses Lock is asking for the key
to be given up, and that is a claim only a closed connection and a zeroised key
can support.

The cost is one Argon2id unwrap on the way back in, 250–500 ms
([storage](../storage.md#argon2id-parameters-and-the-backoff)). That is paid on a
gesture the user chose, not on the copy path
([ADR-0009](./0009-durability-level.md)), and `unlock` after a manual lock is the
launch path exactly — so the expensive-looking option adds no code and the cheap
one would have added a second way to be unlocked.

**Once locking begins it cannot fail.** It can be *refused* before it begins —
`wrong_state { required: "encrypted" }` on an unencrypted store — but there is no
failure after that point. The checkpoint and the close may error; the lock takes
effect anyway and `lock` returns success. A lock that can be refused because a
disk operation failed leaves the clips on screen at the moment the user asked for
them not to be, and there is no residue to worry about: SQLCipher encrypts the
`-wal` alongside the database.

**Manual only, stated as a decision rather than an omission.** An idle timeout is
the obvious next feature and it is out of scope. It needs a duration nobody has
chosen, it fires while the user is reading a clip, and choosing its default is a
product decision. Recording the exclusion here means a later implementer finds a
decision rather than a gap.

**The frontend's universal `locked` handling becomes load-bearing.** It was
already required; it was justified as insurance. It is now the mechanism by which
a lock that lands mid-call is handled, and a call site that omits it is a defect
rather than an inconsistency.

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| Flip a boolean, keep the connection open and the DEK live | "Locked" would mean the backend declines to answer, not that it cannot. Every backend defect becomes a disclosure, and the user's mental model of a lock is wrong. |
| Require the PIN as an argument to `lock` | The caller is already unlocked. Demanding a secret to give up access protects nothing and adds a failure path to an operation that must not have one. |
| `wrong_state { required: "unlocked" }` when already locked | Adds a fourth `required` value for a double-click, and shows a failure for a state the user asked for and is already in. |
| Emit `update_clips` with an empty array so the frontend clears | That event is defined as the complete list. An empty one asserts the store has no clips, which is indistinguishable from every clip having been deleted, and false. |
| An idle timeout, configurable or fixed | Out of the owner's stated scope. The duration is a product decision and a timeout that fires while the user reads a clip is worse than no timeout. |
| Lock on window minimise or blur | Same objection, plus the window is expected to sit beside the app being worked in ([spec §1](../../product/spec.md#1-purpose)), so blur is the normal state rather than an absence. |

## Consequences

- **The specification does not yet describe this feature.** The contract leads
  `spec.md` §4.8 on exactly one point, which is a defect in the book that only
  the owner can close. It is escalated, not written around.
- **The connection's lifetime is no longer the process's.** Every clip-touching
  command must re-check lock state after acquiring the connection mutex, or it
  will report `storage` or `internal` for an ordinary lock. That check belongs in
  one shared guard, written in [WP-03](../../work/wp-03-storage.md) and
  [WP-05](../../work/wp-05-crud.md) before encryption exists, rather than
  retrofitted into each of the ten commands marked "works while locked: no" in
  [WP-07](../../work/wp-07-encryption.md).
- **[ADR-0009](./0009-durability-level.md) was amended before it was accepted.**
  Its consequence "the `-wal` and `-shm` sidecars exist at all times" became
  "whenever the database is open".
- **The tray gains a third rebuild trigger.** [WP-08](../../work/wp-08-tray.md)
  lists two — the clip list and `use_count`. Lock state is the third, and a tray
  still listing labels after a manual lock breaches
  [acceptance criterion 10](../../product/spec.md#8-acceptance-criteria).
- **The frontend must discard state, not only stop fetching.** On `locked: true`
  it drops the clip list, closes any open create or edit form, clears the search
  query, and ignores any `update_clips` that arrives afterwards. A rendered list
  is a disclosure whether or not the backend would serve it again.
- **A mutation in flight when the user locks may or may not land.** Both outcomes
  are correct and the frontend cannot tell which it got beyond the command's own
  result. Nothing partially applies.
- **No new persisted state, and no new file format.** Manual lock is the only
  feature in this refactor that writes nothing.
- **`unlock` is now reachable twice in one process lifetime.** Its
  attempt-counter and backoff behaviour is unchanged, because those live in
  `keyfile` and were already persisted across launches. Its **error set is
  wider**, though: the launch path re-reads `keyfile` and its version byte, and
  mid-session that read can fail in ways a launch-time read could not — a sync
  client restoring the file, the user deleting it, DPAPI failing to unprotect it.
  `unlock` therefore declares `crypto { bad_key_material }` and
  `unsupported_version { component: "key_material" }`
  ([contract](../contract.md#unlock)). Reusing the launch path costs no new code;
  it does not cost no new failure modes.

## Revisit if

An idle or on-blur timeout is specified, which is a
[spec §4.8](../../product/spec.md#48-encryption-and-unlocking) change first and a
contract change second. Or if the Argon2id unwrap on re-entry proves intolerable
in practice, in which case the choice is a cheaper KDF cost — a parameter already
stored in `keyfile` for this reason — and not a live DEK.
