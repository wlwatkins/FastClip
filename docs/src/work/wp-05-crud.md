# WP-05 — Clip list, copy, create, edit, delete

**Objective:** the core loop works end to end.

**Depends on:** WP-03, WP-04. This is the first package where the two halves
meet.

**Inputs:** contract, spec §4.1 and §4.2.

## Work

### backend-dev

Implement the ratified command surface: list, create, update, delete. `new_clip`
takes a clip without identity; the backend mints every `id`. An id arriving
from the frontend on a create is `invalid_input`, not a value to trust.

Emit `update_clips` after every successful mutation. Fix the `APP_HANDLE` race
first: the handle is set inside a spawned task in `setup()`, so an early call
finds `None` and the failure is swallowed by an `eprintln!`.

Remove every `println!` that formats a clip.

### frontend-dev

The scrollable clip list, each clip a full-width coloured button. Click copies.
Non-blocking confirmation that the copy happened — without it users click twice
and paste twice. Full value on hover.

Create, edit and delete flows with inline validation per spec §3. Delete
confirms first.

Keys are clip ids. Labels truncate with CSS, not by measuring text on a canvas.

### test-engineer

Clicking a clip issues the right IPC call with the right argument. An
`update_clips` event re-renders. Create, edit, delete including validation
failure and cancellation. A malformed payload is rejected at the boundary.
Backend: create with a client-supplied id is rejected.

### critic

Contract compliance character for character, both sides: command names,
argument casing, error shape, event name.

## Definition of done

- All acceptance criteria for the core loop pass.
- No clip value appears in stdout.
- An early `invoke` cannot silently fail.

## Risks

This is where the two halves diverge if WP-01 left anything ambiguous. Treat
any contract ambiguity report as more important than the code.
