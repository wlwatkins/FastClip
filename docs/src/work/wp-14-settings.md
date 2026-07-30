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

## This package also owns `get_lock_state` and the startup sequence

`get_settings` is step 1 of the contract's startup sequence, so this is the
package where that sequence is first buildable — and the sequence needs
`get_lock_state`, which [review 005](../reviews/005-wp-05-crud.md) found no
package owned at all.

**It cannot wait for WP-07.** `App.svelte` currently invokes `list_clips`
directly at launch, so the failure screen the contract specifies has no trigger:
a store that will not open produces an error toast rather than the state the
design calls for. Leaving `get_lock_state` to WP-07 means shipping several
packages whose startup path cannot report a broken store.

Splitting it is what makes this tractable. With encryption off, `get_lock_state`
returns `{ encryption_enabled: false, locked: false, attempts_remaining: null,
retry_after_ms: null }` or the recorded startup fault, and **neither needs a line
of crypto**. WP-07 extends it for the encrypted case; this package builds the
command and the sequence around it.

## Work

### backend-dev

Implement `get_settings` and `set_always_on_top` as
[contract §2](../architecture/contract.md#2-commands) defines them, including
the `storage` error variant.

Implement **`get_lock_state`** per the same section, for the
encryption-off case and for relaying a recorded startup fault. The relay
mechanism already exists — `Store::with_unlocked_store` returns the fault
recorded by startup recovery — so this command reports it rather than
rediscovering it.

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

Build the **startup sequence** as [contract §3](../architecture/contract.md)
defines it: `get_settings`, then `get_lock_state`, then branch, then
`list_clips`. Every step has a defined failure branch and step 4's `storage`
goes to the failure screen — explicitly **not** an empty list, which is
indistinguishable from a fresh install. Replace `App.svelte`'s direct
`list_clips` call, which is the WP-05 shortcut this replaces.

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
- **`get_lock_state` is registered** and, with encryption off, returns either the
  unlocked state or the fault startup recovery recorded.
- **The startup sequence runs as the contract defines it** — `get_settings`,
  `get_lock_state`, branch, `list_clips` — and `App.svelte` no longer calls
  `list_clips` directly.
- **A store that will not open reaches the failure screen, not an empty list.**
  An empty list is indistinguishable from a fresh install, which is how a user
  is told their clips are gone when they are merely unreachable. This is the
  criterion the package exists to make testable.
- **The always-on-top toggle is reachable by `Tab`, operable by `Enter` and
  `Space`, carries an accessible name, and shows a focus indicator distinct from
  hover.**

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
