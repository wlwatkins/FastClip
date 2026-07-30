# Review 009 — WP-13 Search and filter (G3)

**Reviewed:** `src/lib/search.ts`, `src/App.svelte`,
`src/lib/components/{ClipList,ClipRow}.svelte`, `src/lib/copy.ts`,
`tests/search.test.ts`.

**Verdict:** REWORK_TESTS → resolved.

This package has **no backend work by design**: its `backend-dev` section reads
*"None. If this package appears to need a backend change, the design is wrong — stop
and escalate."*

## Both assigned checks passed

**No search command was added.** `command_handler!` lists the registered commands and
none is search; a case-insensitive grep for `search|filter|query|needle` across
`src-tauri/src` returns only SQL helpers, `LevelFilter` and iterator `.filter()`.

**The query is never logged**, and on three independent grounds rather than one.
There are zero `console.` calls anywhere in `src/`, so the spy test guards an already
empty set. `package.json` has no frontend log plugin, so there is no JS-side API
reaching the Rust logger. And `log_plugin` calls `clear_targets()` before adding only
Stdout and Folder, so ADR-0012's webview exclusion holds by statement rather than by
default. `searchQuery` has six readers, all in `App.svelte`; none reaches a toast,
`describeError`, or any `invoke`.

## Findings

### F1 — the lock-clears-query test passed whether or not the query was cleared [major]

**Location:** `tests/search.test.ts:216-231` against `src/App.svelte:218-226`
**Failure scenario:** delete `closeSearch()` from the lock effect and change nothing
else. The effect still sets `phase = "locked"`, which unmounts the `{:else}` branch
holding the toolbar and search box — and **both** assertions (search button absent,
searchbox absent) are satisfied by that unmount alone. The test goes green while
`searchQuery` still holds its value and `searchOpen` is still true, because both are
`$state` declared outside the conditional block on a component instance that
persists. On the next unlock the toolbar re-renders and the search box reappears
**pre-populated with a fragment of a clip's `value` that survived the lock** — what
the contract's `lock` section exists to prevent.

The test's own comment claimed the disappearance "is proof the query was cleared".
It is not proof; it is a consequence of the block being removed.

**Why it survives scrutiny:** the critic checked four ways out. Unmounting does not
reset parent state. No other test covers the property — `locked: true` appears in only
two test files and the other is a launch-time lock with no search. ADR-0002 permits
plaintext in webview memory, but the harm here is *re-display after unlock*. And the
implementation is correct; this is a test-layer finding only.

**Disposition:** fixed by `test-engineer`, carried through the unlock leg — emit
`lock_state { locked: false }` then `update_clips` with all three clips, and assert
the searchbox absent and all three labels present. Proved red by removing
`closeSearch()` and observing the failure at the searchbox assertion, then restored.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Filters on `label` and `value`, case-insensitively | Yes | `search.ts:15-21`; unit and end-to-end tests |
| Reordering inert while a query is active | Yes | Four layers — `disabled`, `draggable="false"`, refused `dragover`, no-op keyboard move. Coverage lives in `tests/reorder.test.ts` from WP-06 and was checked rather than duplicated |
| `Ctrl+F` opens and focuses; `Escape` clears and hides | Yes | `App.svelte:198-208`, `:98-104` |
| No backend command added | Yes | See above |
| Query never logged | Yes | Three independent grounds |
| Filtered list preserves the user's order | Yes | `Array.prototype.filter`; tested with a deliberately alphabetically-inverting subset **and** an all-pass case, which catches a sort a subset test would miss |
| Search state not persisted | Yes | No `localStorage`/`sessionStorage`/`indexedDB` anywhere in `src/` |
| Strings from the copy deck | Yes | `copy.ts:34-44` matches `copy.md` character for character |

## What the critic verified

**The empty-result message cannot echo the query.** It is a constant with no
interpolation point, chosen by a boolean, with a comment stating why.

**The drag-inert seam has no gap.** The guard is keyed on `searchQuery !== ""` and
`filterClips` returns the unfiltered array iff the query is empty — so "list is
filtered" and "drag is disabled" are the same condition. There is no state in which a
partial permutation could be constructed and reach `reorder_clips`.

It also accepted `test-engineer`'s red-proof economy and did the work to justify
accepting it, resolving its own doubt about one assertion by finding that agent's
earlier probe note in a different file proving jsdom does not exclude inert subtrees
from queries.

## The lesson worth keeping

The orchestrator had instructed `test-engineer` to assert at the phase-transition
level rather than binding to the two call sites WP-07 was about to replace. That was
right, and the test did survive the replacement.

But **surviving a refactor and detecting a regression are different properties**, and
this test had only the first. "Would this still pass after the change I expect" is a
weaker question than "would this fail if the behaviour were removed".
