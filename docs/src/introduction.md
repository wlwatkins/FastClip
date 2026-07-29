# FastClip

FastClip is a Windows desktop tool that keeps labelled text snippets one click
away from the clipboard. It was written by a human as a proof of concept and is
being refactored by a team of AI agents.

This book is the team's shared memory. Each agent starts with an empty context
and can only read what is on disk, so anything not written here does not reach
the next agent.

## The rule

> **If it is not written here, it is not decided.**

An agent that needs an unspecified fact stops and asks. It does not infer or
default. An invented requirement looks like a decision, and every downstream
agent treats it as one.

Anything written here is binding until changed here.

## Navigation

| Section | Answers |
| ------- | ------- |
| [Product](./product/spec.md) | What the app does, and what it does not |
| [Architecture](./architecture/contract.md) | How the halves fit together, and why |
| [Process](./process/team.md) | Who does what, in what order |
| [Reviews](./reviews/index.md) | What the critic found |
| [Reference](./reference/debt.md) | What was already broken |

Read [the specification](./product/spec.md) first.

## Precedence

1. **Decision records** — supersede everything, including this page. Changing
   one means writing a new one.
2. **Specification** — what the product does.
3. **IPC contract** — derived from the specification. If they disagree, the
   contract is wrong.
4. **Code** — never the authority. Code that disagrees with this book is a
   defect even if it works.

## Building

```sh
cargo install mdbook     # once
mdbook serve docs        # preview at http://localhost:3000
mdbook build docs        # output in docs/book/
```

`docs/book/` is generated and gitignored. Never edit it.

## Status

Gate G0b. The specification is accepted. The architect is closing the remaining
questions in the [contract](./architecture/contract.md). No implementation has
started.
