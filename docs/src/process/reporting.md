# Reporting

Every agent returns a structured report. An agent that free-forms its output
turns the next gate into a judgement call instead of a check.

Reports are the only thing that survives an agent's context. Write them for
someone who was not there.

## Where the templates live

Each template lives in the agent that produces it, so the format and the role
cannot drift apart. There is no copy of them on this page.

| Report | Produced by | Defined in |
| ------ | ----------- | ---------- |
| `ARCHITECTURE_DECISION` | `architect` | `.claude/agents/architect.md` |
| `IMPLEMENTATION_REPORT` | `frontend-dev` | `.claude/agents/frontend-dev.md` |
| `IMPLEMENTATION_REPORT` | `backend-dev` | `.claude/agents/backend-dev.md` |
| `IMPLEMENTATION_REPORT` | `devops` | `.claude/agents/devops.md` |
| `TEST_REPORT` | `test-engineer` | `.claude/agents/test-engineer.md` |
| `CRITIC_JUDGEMENT` | `critic` | `.claude/agents/critic.md` |

The three implementation reports share a name but not a shape. Each carries a
section only that role can fill: contract surface consumed and accessibility
for the frontend, effect on existing user data for the backend, proof each
check can fail for devops.

## Rules

**A section with nothing to report gets "none", never deletion.** A missing
section reads as an oversight, and the reader cannot tell the difference
between "nothing to say" and "forgot to check".

**Never report a command as run that was not run.**

**`UNVERIFIED` and `INCONCLUSIVE` are respectable.** An optimistic `PASS` is
not. A verdict the evidence does not support is worse than no verdict, because
it stops anyone looking again.

**Report the ambiguity you worked around.** A developer who hit an unclear
contract, picked something sensible and shipped it has created a divergence
nobody knows about. That report is worth more than the code.

**The report is the handoff.** The next agent cannot ask you a question — your
context is gone when you return.

## Filing

The orchestrator files `CRITIC_JUDGEMENT` under
[`docs/src/reviews/`](../reviews/index.md), because the critic has no write
access. The other reports are consumed at the gate and do not need filing
unless a decision in them changes the book.
