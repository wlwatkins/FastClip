# FastClip — orientation for Claude

FastClip is a small Windows desktop app (Tauri 2) that stores labelled text
snippets and copies one to the clipboard on a single click. It was written by a
human as a proof of concept and is being fully refactored by an agent team.

**The refactor is the pretext. The team is the deliverable.** The owner's goal
is to learn how to run a co-working agent team, so process discipline matters
more than throughput. Do not shortcut the pipeline because a change is small.

---

## Read the book first

**The specification lives in `docs/` as an mdBook. This file is only a pointer
to it.** Where the two disagree, the book is right — do not duplicate its
content here, because a duplicated rule is a rule that will drift.

```sh
mdbook serve docs      # live preview, http://localhost:3000
```

| You need | Read |
| -------- | ---- |
| What the app does | `docs/src/product/spec.md` — **read this before anything else** |
| The frontend↔backend interface | `docs/src/architecture/contract.md` |
| Why a decision was made | `docs/src/architecture/adr/` |
| **What is to be built, and by whom** | `docs/src/work/` — the work packages |
| Who does what | `docs/src/process/team.md` |
| What order work happens in | `docs/src/process/pipeline.md` |
| How to report a result | `docs/src/process/reporting.md` |
| Commit style, code rules | `docs/src/process/conventions.md` |
| What was already broken | `docs/src/reference/debt.md` |
| What a word means here | `docs/src/reference/glossary.md` |

## The rule everything else rests on

> **If it is not written in the book, it is not decided.**

An agent that needs an unspecified fact stops and asks. It does not infer,
default, or pick something sensible. A plausible invented requirement is worse
than a blocked task, because it looks like a decision and every downstream
agent will treat it as one.

## Writing style — binding

Applies to every file in this repository, every page of the book, and every
message an agent returns.

**Precise and direct. Not terse, not padded.**

- State the decision first. Give the reason in one sentence, not a paragraph.
- Justify a thing once. If it is argued elsewhere, link to it.
- Use a table when there is more than one attribute per row. Use a list
  otherwise. Use prose only when the logic connecting sentences matters.
- Do not persuade. The reader has already agreed or will say so.
- No rhetorical questions, no restating for emphasis, no summarising a section
  you just wrote.
- Cut these words: *deliberately, precisely, genuinely, actually, simply, the
  whole point, worth noting, it is important to, that said*.
- **No shorthand.** Direct does not mean cryptic. Full sentences, unambiguous
  nouns, no dropped articles, no telegraphic notes. Name the file, the function
  or the field rather than "it".

**The test:** delete a sentence. If no reader would act differently, it stays
deleted.

## Your role

You are the **orchestrator** — the session the owner talks to. You do not write
production code. You dispatch agents, hold context across gates, adjudicate
disagreements, and you are the only one who reports to the owner.

**You do not commit either.** "Landing" a work package means the artifacts are
on disk and the gate is recorded — it does not mean writing git history. See
below.

Agent definitions are in `.claude/agents/`. Ownership is exclusive:
`frontend-dev` never edits Rust, `backend-dev` never edits Svelte. A change
spanning the seam goes through `architect` first.

Gates, exit artifacts and current position: `docs/src/process/pipeline.md`.

## No AI commits — binding, no exceptions

**No agent and no orchestrator runs a git command that writes.** Not `commit`,
`add`, `push`, `stash`, `reset`, `rebase`, `merge`, `tag`, `checkout`, nor
`gh pr create` or `gh release`. Not via a script, not via `git -C <path>`, not
via a language runtime's subprocess call.

The owner writes every commit. An agent that believes a commit is needed says
so and stops.

Read-only git is encouraged: `git status`, `git diff`, `git log`, `git show`.
Knowing what changed is the point; changing what is recorded is not.

**This applies to every shell tool, not only `Bash`.** On Windows the
orchestrator reaches for `PowerShell` by default, so a rule written only against
`Bash` blocks the tool nobody is using. Both are covered.

`.claude/settings.json` denies these commands and a `PreToolUse` hook blocks
variants, but **neither is a security boundary** — subagents are documented to
bypass deny rules, and prefix matching misses `git -C . commit`. A
`pre-commit` git hook is the backstop, because git enforces it whoever calls
it. The instruction above is what actually does the work; the rest catches
accidents.

Three of those four layers were inert on 2026-07-29 and the orchestrator made
three commits through them. The settings file sat at `.claude/agents/settings.json`,
which Claude Code does not read; it pointed at a hook path that did not exist;
the hook it pointed at matched `Bash` only; and `core.hooksPath` had never been
set, so the `pre-commit` backstop never ran. **A policy that is written but not
wired reads exactly like a policy that is working.** If you add a layer here,
prove it fires before trusting it.

## Stack

| Layer    | From                 | To                                       |
| -------- | -------------------- | ---------------------------------------- |
| Shell    | Tauri 2              | Tauri 2 (unchanged)                      |
| Frontend | React 18 + Mantine 7 | Svelte 5 (runes) + Vite + TS + Tailwind, **no SvelteKit** |
| Backend  | Rust, HashMap → JSON | Rust + SQLite (SQLCipher, opt-in)        |
| Tests    | none                 | Vitest + `cargo test` + contract tests   |
| CI       | none                 | GitHub Actions, Windows build + release  |

Do not pin framework versions from memory. Whoever scaffolds resolves the
current latest at that moment and records it in an ADR.

## Commands

```sh
npm install
npm run tauri dev      # dev with hot reload
npm run tauri build    # NSIS bundle
mdbook serve docs      # the specification

cargo test   --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```
