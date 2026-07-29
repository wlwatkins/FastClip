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

- storage format and AEAD choice ([storage](../storage.md))
- how list ordering is represented
- whether the SurrealDB dependency is used or removed
- CSP policy for the webview
- code-signing for the Windows installer
