# Review 006 — WP-14 Settings and window state (G3)

**Reviewed:** `src-tauri/src/{settings,window}.rs`, `src/commands/{settings,lock}.rs`,
`src/lib.rs`, `src/commands/{wire,mod}.rs`, `src/storage/{store,paths}.rs`,
`tauri.conf.json`, `capabilities/default.json`, `tests/ipc.rs`; `src/App.svelte`,
`src/lib/components/{SettingsPanel,FailureScreen,TitleBar}.svelte`,
`src/lib/state/{settings,lockState}.svelte.ts`; `tests/{startup-sequence,settings-toggle,titlebar-keyboard}.test.ts`.

**Verdict:** REWORK_IMPLEMENTATION → resolved.

This package grew beyond its scope: review 005 found `get_lock_state` was owned by
**no work package at all**, and it was assigned here because `get_settings` is
step 1 of the startup sequence.

## Findings

### F1 — a failed window creation leaves a running process with no interface and no diagnostic [minor]

**Location:** `src-tauri/src/window.rs:36-64`, `src/lib.rs:96`
**Failure scenario:** where WebView2 is absent or broken, `builder…build()` returns
`Err`, `create_main` returns `()`, and `run()` enters `app.run(...)` with zero
windows. Tauri raises `ExitRequested` only when the last window is *destroyed*, and
none was created, so the event loop never terminates. `windows_subsystem = "windows"`
means no console; the log sink did not yet exist. No window, no message, no tray —
the user must end the process from Task Manager, and each retry adds another.
**Why it survives scrutiny:** the sibling branch eleven lines above handles the same
class correctly (`lib.rs:79-85`), which is where this failure landed *before*
`"create": false`. The doc comment defending the departure argues "a backend that
gives up leaves nothing to show the user" — true for startup recovery, inverted here,
because the thing that failed *is* the surface for showing the user. Its stated
mitigation names the tray, which did not exist and which spec §4.5 makes a copy menu.
**Disposition:** fixed. `create_main` returns `bool` with
`#[must_use = "a false return means there is no window, and the caller must stop"]`,
and `run()` logs, closes the store and returns. The doc comment is corrected rather
than deleted, since the reasoning in it produced the defect.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Toggle survives a restart, both directions | Yes | `tests/ipc.rs:705-730`, three fresh applications over one directory |
| `settings.json` plaintext, readable while locked | Yes | `tests/ipc.rs:829-876` reads it from an encrypted store while clip commands return `locked` |
| Missing or truncated file starts at the default | Yes | `settings.rs:317-341` asserts **every** prefix of a valid file |
| Frontend calls no window API; capability denies it | Yes | `capabilities/default.json:18`; `tests/settings-toggle.test.ts:116` asserts the IPC never appears |
| Interrupted write leaves old or new | Yes | Real `TerminateProcess`, byte-for-byte equality, sentinel guarding a vacuous pass |
| `get_lock_state` registered; relays the recorded fault | Yes | `tests/ipc.rs:776-800` — `list_clips` and `get_lock_state` return the byte-identical `{"kind":"crypto","reason":"corrupt"}` |
| Startup sequence per contract §3; no direct `list_clips` | Yes | `tests/startup-sequence.test.ts:86-128`, with a gated mock proving step 4 is downstream of step 3 |
| **A store that will not open reaches the failure screen, not an empty list** | Yes | The critic enumerated all six fault-producing paths and confirmed each reaches `goToFailure`; the only route to an empty list is `list_clips` resolving `[]` |
| Toggle keyboard-operable, focus distinct from hover | Yes, after rework | `test-engineer` returned FAIL because the switch had **no `hover:` class at all**, so "distinct from hover" was not demonstrable. It left the test red rather than weakening it. Fixed in both checked and unchecked states |

## What the critic checked and found sound

The `tauri.conf.json` change moving window creation into Rust: the label `"main"` is
consistent across `tauri.conf.json`, `capabilities/default.json` and `window.rs`, so
the capability set still binds; `TitleBar` resolves its window at runtime; and
`always_on_top` is applied after `from_config`, which is the ordering that makes it
override the config constant.

The placeholder attempt state is **correct rather than approximate**: with
`enable_encryption` unregistered at the time, `failed_attempts` genuinely was zero.

## Three contract ambiguities, routed to the architect

`set_always_on_top` had no declared error for a refused window call and no defined
order between persisting and applying. The settings temporary file was unnamed and
therefore absent from startup recovery's sweep. And **the contract said step 2's
`storage` is reported again by step 3, which could not be implemented** — the
recorded fault is *also* `storage` when `clips.db` is unreadable, so relaying it from
`get_settings` would refuse settings that are perfectly readable. `backend-dev`
asked the directory directly and named the deviation's cost.

## Recorded

A latent hazard: `settings::write` takes no mutex and `settings.json.new` is a fixed
path. Unreachable today — one window, control disabled synchronously before its
`await` — but a second writer would make it real.

A cold-start flake: six `waitFor` timeouts in `clip-form.test.ts` on one first run
where environment setup took 25–41 s, then two clean reruns. Not in this package's
files, not reproducible on demand, disclosed rather than hidden. The critic's
judgement: tolerable at this gate, and a prediction for WP-12 — `waitFor` runs at
testing-library's 1000 ms default and **every CI run is a first run of a session**.
