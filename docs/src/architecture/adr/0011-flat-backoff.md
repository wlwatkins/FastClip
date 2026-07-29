# ADR 0011 — Wrong-PIN backoff is a flat 30 seconds

**Status:** Accepted. Supersedes the backoff consequence in
[ADR-0004](./0004-optional-pin-encryption.md) only; every other part of that ADR
stands.
**Deciders:** owner

## Context

[ADR-0004](./0004-optional-pin-encryption.md) and
[spec §4.8](../../product/spec.md#48-encryption-and-unlocking) both specified
exponential backoff after the fifth wrong PIN, starting at 30 seconds and
doubling.

That escalation was refined once before it was questioned. At G0b the architect
proposed a one-hour ceiling on the doubling; the owner selected five minutes
instead, and that was recorded. **This ADR is not a correction of that record** —
the record is accurate about what was chosen then. It is the owner changing the
decision, on reasoning that applies to escalation itself rather than to where it
should stop.

## Decision

**After five failed attempts, every wait is 30 000 ms. No doubling and no
ceiling.**

```text
attempts_remaining = max(0, 5 - failed_attempts)
wait_ms            = 30_000                        for failed_attempts ≥ 5
```

The fifth wrong PIN waits 30 seconds. So does the sixth, the twentieth and the
hundredth.

## Rationale

The owner's reasoning, recorded rather than re-derived:

**An escalating wait defends against an attacker who does not exist.** The wait
is imposed by FastClip's own user interface. An attacker capable of automating
that interface to grind through PINs is equally capable of reading
`~/.fast-clip/keyfile` and attacking the wrapped DEK offline, where no
FastClip-imposed delay applies at all. The escalation therefore raises the cost
of the slower of two attacks that the same attacker can choose between, which is
no cost at all.

What it does have is a price, and it is paid entirely by the legitimate user who
mistyped their own PIN.

**What a flat 30 seconds still buys**, and why the backoff is not removed
altogether:

- Friction against someone who has walked up to an unlocked machine and is
  guessing by hand. Thirty seconds per attempt after the fifth is enough to make
  that pointless and short enough to be recoverable by its owner.
- It gives `attempts_remaining` something to count towards, which is what the
  [copy deck](../../product/copy.md) tells the user about.

**The real defence against the offline attack is the KDF**, not the wait:
Argon2id tuned to 250–500 ms per unwrap
([storage](../storage.md#argon2id-parameters-and-the-backoff)), with parameters
stored in the key file so the cost can be raised later. That is the number to
change if the threat model changes, and this ADR does not touch it.

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| Doubling to a five-minute ceiling — the decision this supersedes | The reasoning above applies to any escalation, so choosing where it stops is choosing between values of something that buys nothing. |
| Doubling with no ceiling — [ADR-0004](./0004-optional-pin-encryption.md) and `spec.md` as originally written | Same objection, plus it reaches a week at the twentieth attempt, which turns a mistyped PIN into data loss by another route. [Spec §4.8](../../product/spec.md#48-encryption-and-unlocking) rejects a destructive lockout for exactly that reason. |
| No backoff at all | Gives up the walk-up case for nothing, and leaves `attempts_remaining` counting towards an event that never happens. |
| A longer flat wait | Same trade as escalation, in one step: it costs the mistyping user and the automating attacker is already elsewhere. |

## Consequences

- **The formula loses its exponent**, and with it the per-attempt table in
  [storage](../storage.md#argon2id-parameters-and-the-backoff).
- **`failed_attempts` is no longer an exponent.** It is still persisted and still
  reset to zero on a successful unlock, but the argument for persisting it is now
  a different one and has been rewritten rather than adjusted: without it, five
  attempts becomes unlimited, because a user could guess four times, relaunch,
  and guess four more without ever reaching a backoff. `locked_until_unix_ms` is
  persisted for the parallel reason — relaunching FastClip takes less than 30
  seconds, so a wait held only in memory could be skipped by restarting.
- **No wire change.** `bad_pin { attempts_remaining, retry_after_ms }` and
  `backoff { retry_after_ms }` keep their shapes, and `retry_after_ms` now always
  carries `30000` when it is non-null. The field is kept rather than dropped: it
  is what lets the frontend count down without a constant of its own, and a
  frontend that hardcoded 30 seconds would have to be found and changed if this
  decision moves again.
- **The specification is being edited in parallel** to state 30 seconds flat, so
  the backoff is no longer a point on which the contract leads `spec.md`.
  [Contract §2](../contract.md#lock) recorded two such divergences in a table;
  with one left, the table is gone and the remaining divergence — manual lock —
  is stated in prose.
- **Every evaluated failure from the fifth onward returns the same
  `bad_pin { attempts_remaining: 0, retry_after_ms: 30000 }`.** Under the
  superseded scheme the fifth failure was a transition worth describing on its
  own, because the wait it announced differed from the next one. It no longer
  does, and [contract §2](../contract.md#unlock) says so — the frontend has one
  branch there rather than a series.
- **[ADR-0004](./0004-optional-pin-encryption.md)'s other decisions are
  untouched**: encryption stays opt-in and off by default, both factors are still
  required, the PIN is still a gate rather than the entropy, and nothing is ever
  wiped after failed attempts.

## Revisit if

The threat model changes such that an offline attack on `keyfile` is no longer
available to an attacker who can automate the prompt — which would mean the key
material had moved somewhere FastClip does not control, and that is a change to
[ADR-0004](./0004-optional-pin-encryption.md) rather than to this ADR.
