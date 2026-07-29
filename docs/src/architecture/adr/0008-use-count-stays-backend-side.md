# ADR 0008 — `use_count` does not cross the IPC seam

**Status:** Accepted at G0b.
**Deciders:** architect

## Context

The baseline [contract](../contract.md) listed `use_count` as a field of the
wire `Clip`, received by the frontend and never sent by it. Its second half —
[contract §6](../contract.md#6-open-questions-for-the-architect) question 6 —
asked whether `update_clips` fires on every copy, noting that at one event per
click the tray menu would rebuild constantly.

Both halves turn on the same question: who consumes `use_count`?

| Source | Says |
| ------ | ---- |
| [Spec §3](../../product/spec.md#3-data-model) | It "exists to rank the tray menu … and has no other consumer". |
| [Spec §4.5](../../product/spec.md#45-tray-icon-with-right-click-copy) | The tray menu is native, so no webview is involved. |
| [Spec §6](../../product/spec.md#6-non-goals) | It is "never shown as a statistic to the user". |

The tray menu is built in Rust. The only consumer of `use_count` is on the same
side of the seam as the value itself.

## Decision

**`use_count` is not a field of the wire `Clip`.** The frontend neither receives
it nor sends it.

- The wire `Clip` is `id`, `label`, `value`, `colour`. Nothing else.
- `copy_clip` emits **no** event. A copy changes only `use_count`, which nothing
  on the frontend displays or orders by.
- A tray copy produces no IPC traffic at all.
- `update_clips` is emitted when the set, the content or the order of clips
  changes — create, update, delete, reorder, import, and a successful unlock.
- `use_count` arriving in any command argument remains
  `invalid_input { field: "use_count", reason: "not_permitted" }`, which is
  [spec §3](../../product/spec.md#3-data-model)'s rule kept verbatim.

## Rationale

**A field with no consumer is the defect this refactor removed three of.**
`icon`, `visible` and `clear_time` were serialised and read by nothing, and
[spec §3](../../product/spec.md#3-data-model) deleted them because "three
undefined fields carried through a rewrite produce three invented meanings".
Sending `use_count` to a frontend that must not display it is the same shape:
the only feature it enables is one [spec §6](../../product/spec.md#6-non-goals)
forbids.

**The read-only rule becomes structural.** With the field off the wire there is
no `use_count` for the frontend to send back, so "the frontend never writes
`use_count`" is enforced by the type rather than by review.

**It answers the event question without a compromise.** The reason to fire
`update_clips` on a copy would be to refresh a `use_count` the frontend holds.
It holds none, so the hottest path in the application emits nothing, re-renders
nothing, and rebuilds no menu on the frontend. The native tray menu still
rebuilds — [spec §4.5](../../product/spec.md#45-tray-icon-with-right-click-copy)
requires it — but that happens inside the backend and never touches the webview.

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| Include `use_count` read-only; emit no event on copy | Leaves a field on the wire whose only possible use is forbidden, and leaves the read-only rule enforced by review. |
| Include it and emit `update_clips` on every copy | Rebuilds the list and the tray on the hottest path, for a number nothing displays. This is the cost the open question was raised about. |
| A separate `use_counts` event carrying only the changed count | Same objection with an extra event name: no consumer exists. |

## Consequences

- **[WP-03](../../work/wp-03-storage.md) creates two types where its text
  implies one.** It says "Add `use_count` to the `Clip` struct, defaulting to 0.
  It is backend-owned; nothing outside the backend writes it" — the read-only
  framing this ADR replaces. The **row** type carries `use_count`; the **wire**
  type must not. A single struct carried forward into
  [WP-05](../../work/wp-05-crud.md) as the return type of `list_clips` puts
  `use_count` on the wire, where the boundary validator built from the contract
  rejects every payload and the list renders empty. The architect does not edit
  work packages, so this correction lives here and is flagged to the
  orchestrator.
- **[WP-05](../../work/wp-05-crud.md)'s assertion "the clip's `use_count`
  increases by exactly one" is made against the store, not against an IPC
  payload.** `test-engineer` reads the database directly. So is the criterion 9
  assertion that a count survives a process kill.
- **[WP-08](../../work/wp-08-tray.md)'s ranking is entirely backend-internal.**
  One query — order by `use_count` descending, then `position` ascending, limit
  ten — and no contract surface at all.
- **The frontend cannot preview or explain the tray order.** Nothing asks it to.
  If that is ever wanted, this ADR is superseded rather than worked around.
- **A "most used" view, a usage badge or sort-by-frequency all become contract
  changes.** That is the intended cost: each of them is a product decision, and
  each would arrive today as a frontend change nobody reviewed.
- The `Clip` type is now identical in both directions except for `id`, which
  makes `ClipDraft` a strict subset rather than a near-copy with an exception.

## Revisit if

The product asks for `use_count` to be visible or user-orderable. That is a
[spec §6](../../product/spec.md#6-non-goals) change first and a contract change
second, in that order.
