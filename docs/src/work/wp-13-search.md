# WP-13 — Search and filter

**Objective:** the user can filter the clip list by typing, so a long list stays
usable.

**Depends on:** [WP-05](./wp-05-crud.md) for the list,
[WP-06](./wp-06-reorder.md) because search disables reordering.

**Inputs:** [spec §4.7](../product/spec.md#47-search),
[copy deck](../product/copy.md).

## Work

### frontend-dev

The whole feature. Filtering is client-side over the list already in memory —
there is no search command, and a round trip per keystroke is wrong for a tool
whose value is speed.

The box is hidden by default, revealed and focused by a toolbar button or
`Ctrl+F`. `Escape` clears the query and hides it.

A clip matches when the query appears in its `label` or its `value`,
case-insensitively, by substring containment. Not fuzzy, not regex, not ranked.
The filtered list keeps the user's order.

Disable drag handles whenever the query is non-empty. Clearing it restores them.

Take the placeholder, empty-result and disabled-reorder strings from the copy
deck rather than authoring them inline.

### backend-dev

None. If this package appears to need a backend change, the design is wrong —
stop and escalate.

### test-engineer

A query matching only a `label`, only a `value`, and neither. Case
insensitivity both ways. A query matching nothing shows the empty-result
message rather than a blank panel. `Escape` restores the full list. Drag
handles are inert with a query present and active without one. Filtering does
not reorder the surviving clips.

### critic

Check that no search command was added to the backend. Check that the query is
never logged — it is a substring of a clip's `value`, and
[ADR-0002](../architecture/adr/0002-threat-model.md) keeps clip values out of
logs.

## Definition of done

- Typing filters on `label` and `value`, case-insensitively.
- Reordering is inert while a query is active.
- `Ctrl+F` opens and focuses; `Escape` clears and hides.
- No backend command was added.

## Risks

Search reads `value`, which is the field the rest of the design treats as
sensitive. Nothing may log the query or the matched values.
