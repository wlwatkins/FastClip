# Review 007 — WP-06 Reordering (G3)

**Reviewed:** `src/lib/components/{ClipList,ClipRow}.svelte`, `src/App.svelte`,
`src/lib/ipc/commands.ts`; `src-tauri/src/storage/clips.rs`,
`src/commands/{wire,clips}.rs`, `src/lib.rs`, `tests/{ipc,kill_mid_write}.rs`;
`tests/reorder.test.ts`.

**Verdict:** ACCEPT — 1 minor finding, fixed.

## The assigned check passed structurally

"Check for two sources of truth. If the frontend keeps its own ordering state
alongside the backend's, they will disagree."

`filterClips` returns the **same array reference** when the query is empty
(`src/lib/search.ts:16`), so `ClipList`'s `clips` prop is identically
`clipListState.clips` whenever reordering is enabled — and that state has exactly one
writer, `setClips`, with three callers. Both gesture paths copy before splicing.
There is no ordering state on the frontend for the backend to disagree with.

`frontend-dev` declined to render a drop-target insertion line because the transient
state needed would be that second source. The critic endorsed the call.

## Findings

### F1 — dragging a clip handle onto the open search box replaces the query with the clip's UUID [minor]

**Location:** `src/lib/components/ClipRow.svelte:34`, read back at `:47`
**Failure scenario:** open the search box; the query is empty, so `searchActive` is
false and handles stay enabled — correctly. Drag a handle past the top of the list
and release over the search `<input>`. A text input accepts a `text/plain` drop by
default, sets its value and fires `input`, which `bind:value` applies. The query
becomes a 36-character UUID, nothing matches, and the list is replaced by the
empty-result message mid-drag, with no error.
**Why it survives scrutiny:** the search-active guards key on `searchQuery !== ""`
and this path *starts* empty. The row's own `ondrop` is not involved — the damage is
the input's native handling. And `text/plain` is not required.
**Marked as a prediction:** jsdom has no native drag semantics; derived from reading
the handlers plus HTML5 drag-and-drop behaviour, and the existing tests cannot
observe it.
**Disposition:** fixed. The transfer type is now
`application/x-fastclip-clip-id` in both `setData` and `getData`, kept in sync — a
private type no native drop target on the page registers.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Order set by the user survives restart | Yes | `commands/clips.rs:384` reopens over the same paths after `mem::forget`; `kill_mid_write.rs:31` across a real `process::abort` |
| Reordering possible without a mouse | Yes | Arrow-key handler on a Tab-reachable `<button>`; `tests/reorder.test.ts:124,149` |
| No partial order can be persisted | Yes | Single `BEGIN IMMEDIATE`; the kill test abandons a **half-written renumber** — offset written across every row, ten of fifty final positions, then `abort()` — and the parent reads back the committed order, dense, with no `+N` residue |
| Keyboard produces the same result as dragging | Yes | Both paths' `order` arguments captured independently and compared, in both directions |

## What the critic verified beyond the tests

**The offset-then-write renumber is sound for sparse inputs, not only dense ones.**
After `position += max+1` every row sits in `[max+1, 2max+1]`, and the targets
`0..len-1` satisfy `len-1 ≤ max` because the row count equals `ordered.len()` and
positions are distinct non-negatives bounded by `max`. The ranges cannot overlap
whatever order SQLite visits rows in — so `UNIQUE(position)` cannot be transiently
violated in the general case, not just the tested one.

The permutation check runs **inside the same transaction that rewrites the
positions**, on the single mutex-guarded connection, so the set checked is the set
rewritten and there is no window to race.

## Three reported ambiguities — two dismissed, one routed

`malformed_uuid` cannot name which element of `order` was malformed: **not a gap.**
`InvalidInput` carries `field` and `reason`; §4's table lists `order[]` as a rule
about elements while `field` names the wire field, which is `order`.

An empty `order` array: **already decided** by the existing text. `[]` is an exact
permutation of an empty store and a length mismatch against a populated one;
`required` is defined for an *absent* key.

`locked` "before doing anything else" versus argument validation: **routed to the
architect** as a one-sentence amendment, since it described eleven commands and made
every compliant implementation look like a deviation. The property that matters —
the lock decided before the argument is compared against the stored id set — is
correct and pinned by `tests/ipc.rs:1030-1033`.

## Recorded

`test-engineer` found **two bugs in its own draft tests** by probing: a query that
filtered the list to one clip so `moveClip`'s bounds check no-opped the move
regardless of the guards under test (green with both guards removed), and a
`reorderClips` mock never wired into the harness so `not.toHaveBeenCalled()` asserted
against a function nothing could call. Both fixed and re-probed red.

It declined to unify the duplicated `generate_handler!` list, with reasoning about
proc-macro expansion, and recorded the residual risk. `backend-dev` and the critic
agreed independently that it deserved its own package before WP-07. It was done.
