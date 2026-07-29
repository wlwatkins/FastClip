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
| [0004](./0004-optional-pin-encryption.md) | Optional PIN-gated encryption | Accepted; supersedes 0002's encryption decision |
| [0005](./0005-sqlite-store.md) | SQLite via SQLCipher as the store | Accepted; crate choice deferred to G0b |
| [0006](./0006-tailwind.md) | Tailwind as the only styling system | Accepted |

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

- how list ordering is represented
- CSP policy for the webview
- code-signing for the Windows installer
