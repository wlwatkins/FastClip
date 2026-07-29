# The agent team

Six agents plus the orchestrator.

Agent definitions in `.claude/agents/*.md` describe **who each agent is** —
role, limits, engineering rules, output format. They do not list tasks. The
work lives in [work packages](../work/index.md), so a change of plan does not
require rewriting anyone's role.

## Who dispatches

| | |
| --- | --- |
| **Orchestrator** | The session the owner talks to. Holds the gates, adjudicates disagreements, files the critic's judgements, and is the only participant that reports to the owner. Writes no production code. |
| **Architect** | Dispatches work packages to the developers and checks what returns against the contract. Technical lead. |

The architect holds `Agent(...)` in its tool grant so it can delegate directly.
This is worth verifying on the first run: nested subagent dispatch is
documented but untested here. If it does not work, the orchestrator dispatches
from the architect's written plan instead, which changes nothing else.

There is no separate coordinator role. Two dispatchers means neither owns the
work. The integration job a coordinator would hold belongs to the `architect`,
expressed as a written contract.

## Roster

| Agent | Owns | Cannot touch | Model | Effort |
| ----- | ---- | ------------ | ----- | ------ |
| `architect` | [contract](../architecture/contract.md), ADRs, the seam | feature code | opus | high |
| `frontend-dev` | `src/**`, Vite config | `src-tauri/**` | sonnet | medium |
| `backend-dev` | `src-tauri/**` | `src/**` | opus | high |
| `devops` | `.github/**`, bundling, releases, dependencies | application code | sonnet | low |
| `test-engineer` | `tests/**`, `*.test.ts`, `#[cfg(test)]` | production code | sonnet | medium |
| `critic` | nothing | everything | opus | xhigh |

Definitions are in `.claude/agents/*.md`. They load only when Claude Code runs
inside the project directory.

## Exclusive ownership

`frontend-dev` does not edit Rust. `backend-dev` does not edit Svelte. A change
spanning the seam goes through `architect` first, which forces the interface to
be designed rather than accreted.

This is enforced by instruction, not by tooling. Claude Code restricts an
agent's *tools*, not its *paths*, and a `PreToolUse` hook cannot identify which
subagent called it. The `critic` checks lane violations at G3.

## The critic has no write access

Tools: `Read`, `Grep`, `Glob`. No `Bash` — a shell restores write access
through redirects, `sed` or `git`. `permissionMode: dontAsk` converts any
attempt to reach outside that set into a denial.

Consequence: the critic cannot run the compiler, linters or tests. It reviews
by reading and marks build-dependent findings as unverified. CI runs those.

It may return zero findings, and it may only `BLOCK` on security, data loss or
a contract violation. A critic that blocks on style gets overridden, after
which its blocks carry no weight.

## Model assignment

The two roles that write no production code run on the strongest model.
`architect` and `critic` are pure judgement, and both produce artifacts that
everything downstream trusts without rechecking. `backend-dev` also runs at the
top tier because it owns the crypto and the migration, the only code here that
can destroy user data.

The remaining developers are cheaper because the hard decisions are already
written down by the time they are dispatched. `devops` runs lowest; its output
is verified by whether CI passes.

## Dispatch

- Never run `critic` in the same turn as the agent it reviews. It reads disk,
  not claims.
- `devops` is bursty. It is instructed to report "nothing to do" rather than
  invent work.
- When an agent reports a contract ambiguity, route it to `architect` before
  dispatching anyone else.
