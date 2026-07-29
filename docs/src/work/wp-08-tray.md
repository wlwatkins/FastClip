# WP-08 — Tray icon

**Objective:** copy a clip from the system tray without raising the window.

**Depends on:** WP-05.

**Inputs:** contract, spec §4.5.

## Work

### backend-dev

Add the tray icon and a right-click menu listing clips by label. Choosing one
copies its value. Rebuild the menu whenever the clip list changes. Truncate
long labels in the menu.

The tray menu is native, so no webview is involved and the backend writes to
the clipboard itself. Use the single clipboard path the architect specified in
WP-01. Two independent clipboard implementations must not appear.

### test-engineer

Menu contents track the clip list after create, delete and reorder. Native tray
interaction may not be automatable — if so, record it as an explicit gap rather
than letting silence imply coverage.

### critic

Check for a second clipboard code path. Check that tray labels cannot leak a
clip value into a menu string.

## Definition of done

- Right-clicking the tray copies a clip without raising the window.
- The menu reflects the current list.
- One clipboard implementation exists.

## Risks

Tray behaviour is hard to test automatically. Expect this package to have a
larger untested surface than any other, and say so rather than implying
coverage.
