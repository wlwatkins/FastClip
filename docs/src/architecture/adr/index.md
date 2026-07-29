# Decision records

An ADR records a decision that closes off an option. Write one when a decision
picks a framework, storage format or crypto scheme, rules something out of
scope, or would look wrong to a reader without today's context.

An accepted ADR outranks every other page in this book. Changing one means
writing a new ADR that supersedes it, not editing the old one.

## Index

| # | Title | Status |
| - | ----- | ------ |
| [0001](./0001-plain-svelte-not-sveltekit.md) | Plain Svelte + Vite, not SvelteKit | Accepted |
| [0002](./0002-threat-model.md) | Threat model and the scope of security work | Accepted |
| [0003](./0003-no-legacy-migration.md) | No migration from pre-refactor data | Accepted |
| [0004](./0004-optional-pin-encryption.md) | Optional PIN-gated encryption | Accepted; supersedes 0002's encryption decision; its backoff consequence superseded by 0011 |
| [0005](./0005-sqlite-store.md) | SQLite via SQLCipher as the store | Accepted; crate choice deferred to the WP-02 spike |
| [0006](./0006-tailwind.md) | Tailwind as the only styling system | Accepted |
| [0007](./0007-list-order-representation.md) | List order is a property of the list, not of a clip | Accepted |
| [0008](./0008-use-count-stays-backend-side.md) | `use_count` does not cross the IPC seam | Accepted |
| [0009](./0009-durability-level.md) | Durability level: WAL with `synchronous = NORMAL` | Accepted; amended before acceptance by 0010 |
| [0010](./0010-manual-lock.md) | Locking is manual, and it closes the database | Accepted at G0b, second round |
| [0011](./0011-flat-backoff.md) | Wrong-PIN backoff is a flat 30 seconds | Accepted; supersedes 0004's backoff consequence |

0007 through 0010 are Accepted and **G0b is landed**. The critic returned
`ACCEPT` on the fourth review of the ratified contract
([003](../../reviews/003-wp-01-contract.md)); 0010 was Proposed until then only
because the critic had not yet seen it.

The rule at the top of this page now applies to all four without exception:
changing one means writing a new ADR that supersedes it, not editing the old one.
The in-place amendment 0009 received during G0b — after `lock` made its sidecar
consequence false — was permitted because the gate was still open. That
permission has ended.

**0011 is the first ADR written under that rule.** The owner changed the
wrong-PIN backoff from exponential to a flat 30 seconds after G0b landed, and
because the escalation lives in [0004](./0004-optional-pin-encryption.md), which
is Accepted, the change is a new ADR rather than an edit. 0004's superseded
sentence is left in place and marked, so a reader meets the change rather than a
document that never held the old decision.

0010 was written in the same gate, after the owner put manual lock in scope, and
was Accepted with the rest once the critic had reviewed it. It records why rather
than what: the `lock` surface itself is ratified in
[contract §2](../contract.md#lock).

## Template

```markdown
# ADR NNNN — <title>

**Status:** Proposed | Accepted | Superseded by NNNN
**Deciders:** <who decided>

## Context
What forced a choice.

## Decision
The choice, stated so it cannot be misread.

## Rationale
Why this over the alternatives. Name the alternatives.

## Consequences
What is easier, what is harder, and what would make us revisit this. Include
the bad consequences; an ADR with no downsides listed is marketing.
```

## Anticipated

Listed so they are not decided silently:

- CSP policy for the webview
- code-signing for the Windows installer

How list ordering is represented was on this list and is now
[0007](./0007-list-order-representation.md).
