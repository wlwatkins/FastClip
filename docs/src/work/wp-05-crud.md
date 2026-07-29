# WP-05 — Clip list, copy, create, edit, delete

**Objective:** the core loop works end to end.

**Depends on:** WP-03, WP-04. This is the first package where the two halves
meet.

**Inputs:** [contract](../architecture/contract.md),
[spec §4.1](../product/spec.md#41-copy-a-clip) and
[§4.2](../product/spec.md#42-create-edit-delete).

## Work

### backend-dev

Implement the ratified command surface: list, create, update, delete. `new_clip`
takes a clip without identity; the backend mints every `id`. An id arriving
from the frontend on a create is `invalid_input`, not a value to trust.

Implement the single copy command that both the window and the tray use: it
writes the clipboard and increments that clip's `use_count`, written through to
disk before returning ([spec §4.1](../product/spec.md#41-copy-a-clip)). The
frontend never writes `use_count`; an incoming one is `invalid_input`.

Emit `update_clips` after every successful mutation. Fix the `APP_HANDLE` race
first: the handle is set inside a spawned task in `setup()`, so an early call
finds `None` and the failure is swallowed by an `eprintln!`.

Remove every `println!` that formats a clip.

### frontend-dev

The scrollable clip list, each clip a full-width coloured button. Click invokes
the backend copy command rather than writing the clipboard directly — the count
must be recorded.
Non-blocking confirmation that the copy happened — without it users click twice
and paste twice. Full value on hover.

Create, edit and delete flows with inline validation per
[spec §3](../product/spec.md#3-data-model). Delete
confirms first.

Keys are clip ids. Labels truncate with CSS, not by measuring text on a canvas.

### test-engineer

Clicking a clip issues the copy command with the right argument, and the clip's
`use_count` increases by exactly one. A count survives an immediate process
kill after a copy. An `update_clips` event re-renders. Create, edit, delete including validation
failure and cancellation. A malformed payload is rejected at the boundary.
Backend: create with a client-supplied id is rejected.

### critic

Contract compliance character for character, both sides: command names,
argument casing, error shape, event name.

## Definition of done

- All acceptance criteria for the core loop pass.
- No clip value appears in stdout.
- A copy increments `use_count` durably.
- An early `invoke` cannot silently fail.

## Risks

This is where the two halves diverge if WP-01 left anything ambiguous. Treat
any contract ambiguity report as more important than the code.
