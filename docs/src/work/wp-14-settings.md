# WP-14 — Settings and window state

**Objective:** persist the always-on-top toggle across restarts, and give
settings a home that works while the store is locked.

**Depends on:** WP-03, WP-04.

**Inputs:** [spec §4.4](../product/spec.md#44-always-on-top),
[contract §2](../architecture/contract.md#2-commands) —
`get_settings` and `set_always_on_top` — and
[storage](../architecture/storage.md), which places `settings.json` at
`~/.fast-clip/` outside the database.

## Why this package exists

[Spec §4.4](../product/spec.md#44-always-on-top) calls always-on-top "already
implemented; keep the behaviour". The toggle is implemented. Its persistence is
not: the current build holds the flag in React state initialised to `false`
(`src/Components/SettingsClip.tsx:12`) and calls
`getCurrentWindow().setAlwaysOnTop()` from the frontend, so the setting resets
on every launch.

WP-01 specified the command surface and found that no work package owned the
work. This package is that owner.

## Work

### backend-dev

Implement `get_settings` and `set_always_on_top` as
[contract §2](../architecture/contract.md#2-commands) defines them, including
the `storage` error variant.

Read and write `~/.fast-clip/settings.json` with the shape
`{ "version": 1, "always_on_top": false }`. It is plaintext and outside the
database because the window must be positioned before the store is unlocked and
possibly while it stays locked. Write it with the same atomic
write-temp-then-rename the store uses; a truncated `settings.json` must not
prevent the app starting.

**The backend both persists and applies the setting**, in two places: on
`set_always_on_top`, and again at window creation from the persisted value.
Applying it at window creation rather than from the webview is what stops the
window appearing in the wrong state and then correcting itself.

A missing or unreadable `settings.json` is not an error. Fall back to the
documented default, `always_on_top: false`, and continue. An unrecognised
`version` is also not an error: ignore the file, use the default, and overwrite
it at version 1. `settings.json` is the one versioned artefact whose unknown
version does not fail, because a window setting is not worth refusing to start
over.

### frontend-dev

Replace the local React-era state with a value read from `get_settings` at
startup, and drive the toggle through `set_always_on_top`.

**The frontend must not call Tauri's window API for this.** Its capability set
must not grant `core:window:set_always_on_top`. Two writers of one window
property is two sources of truth, and the startup application has to be
backend-side regardless.

### test-engineer

Cover the behaviour the current build gets wrong: set the toggle, restart, and
assert the window comes back in the same state. Cover the three failure paths —
missing file, truncated file, unrecognised `version` — and assert that all three
start the app at the default rather than blocking it.

### Others

No work.

## Definition of done

- The toggle survives a restart, in both directions.
- `settings.json` exists at `~/.fast-clip/`, is plaintext, and is readable while
  the store is locked.
- A missing or truncated `settings.json` starts the app at the default.
- The frontend calls no Tauri window API for always-on-top, and the capability
  set does not grant it.
- An interrupted write leaves either the old settings or the new ones.

## Open — owner

[Spec §8](../product/spec.md#8-acceptance-criteria) has no acceptance criterion
for always-on-top, so nothing in the specification tests this package. The
definition of done above is derived from spec §4.4 and the contract, not from
spec §8. Whether spec §8 gains a criterion is the owner's call; this package
does not invent one.

## Risks

Settings inside the database would be unreadable while the store is locked,
which is why [storage](../architecture/storage.md) put them outside it. A later
change that moves them in would break window positioning at startup without
failing any test written here.
