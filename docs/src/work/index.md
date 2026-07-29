# Work packages

The refactor is divided into fourteen work packages. A work package is the unit
the `architect` dispatches: it has one objective, named dependencies, a task
for each agent that participates, and acceptance criteria that decide whether
it is done.

The [agent definitions](../process/team.md) describe who each agent is and what
it may touch. This section describes what is to be built. The two are kept
apart so that a change of plan does not require rewriting anyone's role.

## Dispatch

The `architect` dispatches work packages and reviews what comes back against
the contract. The orchestrator holds the gates, adjudicates, and reports to the
owner.

A dispatched task carries four things. A task missing any of them will be
implemented against an invented requirement:

| Field | Meaning |
| ----- | ------- |
| Objective | One sentence. What changes. |
| Constraints | Scope, paths, what must not change. |
| Expected output | The deliverable and its report template. |
| Acceptance criteria | How the result will be judged. |

## Sequence

| WP | Title | Depends on | Lead |
| -- | ----- | ---------- | ---- |
| [01](./wp-01-contract.md) | Contract ratification | — | architect |
| [02](./wp-02-toolchain.md) | Toolchain and CI skeleton | — | devops |
| [03](./wp-03-storage.md) | Storage: ordered and crash-safe | 01 | backend-dev |
| [04](./wp-04-frontend-scaffold.md) | Frontend scaffold, React removed | 01 | frontend-dev |
| [05](./wp-05-crud.md) | Clip list, copy, create, edit, delete | 03, 04 | both devs |
| [06](./wp-06-reorder.md) | Reordering | 05 | both devs |
| [07](./wp-07-encryption.md) | Optional PIN-gated encryption | 03, 09 | both devs |
| [08](./wp-08-tray.md) | Tray icon | 05 | backend-dev |
| [09](./wp-09-export-import.md) | Export and import | 05 | both devs |
| [10](./wp-10-palette.md) | Palette and accessibility | 04 | frontend-dev |
| [11](./wp-11-copy-deck.md) | Copy deck and README | 07, 09 | frontend-dev |
| [13](./wp-13-search.md) | Search and filter | 05, 06 | frontend-dev |
| [14](./wp-14-settings.md) | Settings and window state | 03, 04 | both devs |
| [12](./wp-12-release.md) | Release pipeline | all | devops |

WP-01 and WP-02 run in parallel. So do WP-03 and WP-04 once the contract is
ratified, and WP-08, WP-10, WP-13 and WP-14 once their dependencies clear.

**Export comes before encryption.** WP-07's opt-in warning tells the user a
forgotten PIN is unrecoverable and offers an export first, so the export must
already exist. Building encryption first would leave that warning pointing at a
feature nobody had written.

WP-13 is numbered last but sequenced before WP-12, which gates on everything.

## Every work package ends the same way

1. The developers return implementation reports.
2. `test-engineer` returns a test report.
3. `critic` returns one verdict.
4. The orchestrator lands it, or returns it to the responsible agent.

A work package is not complete because its code works. It is complete when its
acceptance criteria are met and the critic has seen it.
