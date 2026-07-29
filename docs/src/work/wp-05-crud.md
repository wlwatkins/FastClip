# WP-05 — Clip list, copy, create, edit, delete

**Objective:** the core loop works end to end.

**Depends on:** WP-03, WP-04. This is the first package where the two halves
meet.

**Inputs:** [contract](../architecture/contract.md),
[spec §4.1](../product/spec.md#41-copy-a-clip) and
[§4.2](../product/spec.md#42-create-edit-delete).

## Work

### backend-dev

Implement the ratified command surface: list, create, update, delete.
`create_clip` — renamed from `new_clip` at WP-01 — takes a `ClipDraft`, a clip
without identity; the backend mints every `id`. An id arriving from the frontend
on a create is `invalid_input`, not a value to trust.

The [contract](../architecture/contract.md) is authoritative for every command
name, argument, error variant and event. Where this page and the contract
disagree, the contract wins and the disagreement is a defect on this page.

Implement the single copy command that both the window and the tray use: it
writes the clipboard and increments that clip's `use_count`, written through to
disk before returning ([spec §4.1](../product/spec.md#41-copy-a-clip)). The
frontend never writes `use_count`; an incoming one is `invalid_input`.

Emit `update_clips` after every successful mutation, and only after a successful
one — the contract's §3 rules on which commands emit it, and on the
`lock_state` exception, are binding.

The `APP_HANDLE` race this package originally existed to fix is already gone:
WP-03 deleted the `lazy_static` global and the store is Tauri-managed state. What
remains is the contract's guarantee that an early `invoke` cannot silently fail,
which is in this package's definition of done and still needs proving.

Every `println!` that formatted a clip went with `commands.rs` at WP-03. Do not
reintroduce one — `ClipRow`'s `Debug` redacts `label` and `value` to lengths, and
that redaction is asserted by a test.

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
- **Every control this package adds is reachable by `Tab`, operable by `Enter`
  and `Space`, carries an accessible name, and shows a focus indicator distinct
  from hover.** That includes the clip row, its copy target, the row actions,
  the create and edit forms, the delete confirmation, and the colour picker.

The colour picker has a second requirement: each swatch renders its token's
**foreground** colour on its own fill, not `currentColor`. Every pair in
[palette](../product/palette.md) is verified to clear 4.5:1, and the picker is
the first place that pairing is actually rendered — the Rail layout puts label
text on the row background, never on a fill.

## Risks

This is where the two halves diverge if WP-01 left anything ambiguous. Treat
any contract ambiguity report as more important than the code.
