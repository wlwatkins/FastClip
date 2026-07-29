# WP-08 — Tray icon

**Objective:** copy a clip from the system tray without raising the window.

**Depends on:** WP-05.

**Inputs:** [contract](../architecture/contract.md),
[spec §4.5](../product/spec.md#45-tray-icon-with-right-click-copy).

## Work

### backend-dev

Add the tray icon and a right-click menu. It shows **at most ten clips**,
ranked by descending `use_count`, with the user's list order filling any
remaining slots and breaking ties
([spec §4.5](../product/spec.md#45-tray-icon-with-right-click-copy)). A fresh
install therefore has a populated menu.

Choosing an entry calls the same copy command the window uses — written in
[WP-05](./wp-05-crud.md) — so a tray copy counts identically. Two independent
clipboard implementations must not appear.

Rebuild the menu whenever the clip list or any `use_count` changes. Truncate
long labels.

### test-engineer

Ranking is correct: ten most-used first, list order filling the remainder and
breaking ties. Menu contents track the clip list after create, delete and
reorder, and after a copy changes the ranking. With more than ten clips, the
eleventh does not appear until its count overtakes another.

Native tray interaction may not be automatable — if so, record it as an
explicit gap rather than letting silence imply coverage.

### critic

Check for a second clipboard code path. Check that tray labels cannot leak a
clip value into a menu string.

## Definition of done

- Right-clicking the tray copies a clip without raising the window.
- The menu shows at most ten entries, correctly ranked.
- A tray copy increments `use_count`.
- One clipboard implementation exists.

## Risks

Tray behaviour is hard to test automatically. Expect this package to have a
larger untested surface than any other, and say so rather than implying
coverage.
