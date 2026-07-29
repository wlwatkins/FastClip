# WP-06 — Reordering

**Objective:** the user drags clips into an order that persists.

**Depends on:** WP-05.

**Inputs:** [contract](../architecture/contract.md),
[spec §4.3](../product/spec.md#43-reorder), the order-representation ADR from
[WP-01](./wp-01-contract.md).

## Work

### frontend-dev

Drag-to-reorder in the clip list. It must work at 250px width and be operable
from the keyboard — a drag-only interaction is inaccessible.

Send the new order using the command the architect specified. Do not maintain a
local order that can disagree with the backend's.

### backend-dev

Persist the new order atomically. A crash mid-reorder leaves either the old
order or the new one, never a half-applied one.

Emit `update_clips` with the list in display order.

### test-engineer

Order survives a restart. A reorder interrupted mid-write leaves a valid store.
Keyboard reordering produces the same result as dragging.

### critic

Check for two sources of truth. If the frontend keeps its own ordering state
alongside the backend's, they will disagree.

## Definition of done

- Order set by the user is the order after restart.
- Reordering is possible without a mouse.
- No partial order can be persisted.

## Risks

If the ADR chose a `position` field per clip rather than a single ordered list,
N records can disagree with each other. Test that case specifically.
