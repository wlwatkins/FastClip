# Review 005 — WP-05 Clip list, copy, create, edit, delete (G3)

**Reviewed:** `src-tauri/src/commands/{mod,wire,clips,clipboard,events}.rs`,
`src-tauri/src/redact.rs`, `lib.rs`, `storage/clips.rs`, `build.rs`,
`Cargo.toml`, `tests/ipc.rs`; `src/App.svelte`, `src/lib/ipc/commands.ts`,
`src/lib/state/`, `src/lib/{colour,validation,errorMessage}.ts`,
`src/lib/components/*.svelte`; `tests/*.test.ts`, `tests/helpers/mockTauri.ts`,
`src/lib/contract/validate.test.ts`.

**Verdict:** REWORK_ARCHITECTURE → all findings resolved, package landed.

Gates verified by the orchestrator, not the critic, which has no shell: Rust 109
passing, `fmt` and `clippy` clean, `Cargo.lock` unchanged; frontend 77 passing
across 11 files, `typecheck` clean, `svelte-check` 135 files 0 errors, `build`
clean. There is no CI, so local runs are the only evidence.

## The assigned check passed

Contract compliance, character for character, both sides. Command names,
argument keys (`clip`, `clip_id`), the event name, `Clip`'s four fields, all 13
`ClipError` variants, all 9 `InvalidReason`, 5 `ImportReason`, 4 io reasons, 2
crypto, 2 component and 3 `wrong_state` — identical in `src-tauri/src/error.rs`
and `src/lib/contract/types.ts`. **No drift in either direction.**

Both halves were built in parallel with no coordination beyond the contract.
Four review rounds on that document bought a clean seam, which is the return
this gate existed to measure.

## Findings

### F1 — the contract asserted a property the backend cannot honour [major]

**Location:** `contract.md` "Opening the database" and §3 step 4, against
`src-tauri/src/storage/store.rs:140-155` and `:220-242`
**Failure scenario:** the contract stated *"A `list_clips` returning
`unsupported_version` is a backend defect"*. On disk all five commands can return
exactly that: startup recovery records the fault with no connection, the store is
not encrypted so both lock checks pass, and `with_unlocked_store` returns the
recorded fault verbatim. `store.rs:220-242` is a passing test asserting it does.
The invariant held only via `get_lock_state` — which no work package owned, and
which does not exist — so `App.svelte` went straight to `list_clips` and the
state was reachable.
**Why it survives scrutiny:** the obvious backend fix is forbidden by the
contract's own "Do not map a failed open during a conversion to `storage`"
paragraph, and would discard the `crypto` sentence pointing at import as the
recovery path. Every available fix was a contract decision.
**Disposition:** fixed by `architect`, which named its own diagnosis as the
substance: it had conflated *where a fault is discovered* with *where it is
reported*, then written exclusivity rules as if both were settled. **Calling
something a backend defect is only legitimate when the backend can avoid doing
it.** Discovery stays at one point; reporting happens wherever the caller asks,
relaying one recorded value. Error rows updated on eleven commands, and its
sweep caught a twelfth: `enable_encryption` is marked "works while locked: yes"
because it never returns `locked`, but it must read the plaintext store to
convert it. The contract now separates "returns `locked`" from "needs the store",
because conflating them is what hid it.

### F2 — the frontend's control-character check was narrower than the backend's [minor]

**Location:** `src/lib/validation.ts:12` against `src-tauri/src/commands/wire.rs:180`
**Failure scenario:** the regex stopped at U+007F; Rust's `char::is_control` is
category `Cc`, U+0000–U+001F **and U+007F–U+009F**. A label containing U+0092 or
U+0085 passed inline validation and was rejected by the backend after a round
trip — the round trip the frontend copy exists to prevent. U+0085 diverged worse:
Rust's `trim()` treats it as whitespace, so the user was told a field they filled
in was empty.
**Disposition:** fixed by `frontend-dev`. Range widened to
`[\x00-\x1F\x7F-\x9F]`, with the U+0085/`trim()` divergence recorded in a comment.

### F3 — drag-selecting text out of a dialog dismissed it and discarded the edit [minor]

**Location:** `ClipForm.svelte:102-106`, `ConfirmDialog.svelte:38-42`
**Failure scenario:** mouse down inside the Value textarea, drag past the top
edge, release over the overlay. The DOM dispatches `click` on the nearest common
ancestor of press and release — the overlay itself — so the inner
`stopPropagation` never ran. The form unmounted and up to 10 000 characters of
unsaved edit were lost with no confirmation and no undo.
**Why it survives scrutiny:** an `event.target === event.currentTarget` guard
does not fix it, because in this gesture the target genuinely *is* the overlay.
Click-outside-to-dismiss is the frontend's own invention, so this is a defect in
a self-imposed affordance.
**Disposition:** fixed by `frontend-dev` with a `mousedown`/`click` pair
dismissing only when both ends land on the overlay. Covered by
`tests/dialog-overlay-drag.test.ts`, 5 tests, proved by reverting one component
to the naive `onclick={oncancel}` and observing exactly one failure — the case
exercising the bug, with the other component's cases untouched.

### F4 — the integration test's header restated the false premise the file disproves [minor]

**Location:** `src-tauri/tests/ipc.rs:20-24`
**Failure scenario:** the header said *"The store is managed in `setup`, which
runs inside `build`"* — the exact belief that produced the defect `backend-dev`
found by building. A developer "simplifying" `.manage()` into a `setup` hook on
its authority reintroduces the plain-string rejection contract §0 forbids, **and
every test in the file still passes**, because the `build()` helper constructs
its own application and does not use `setup`.
**Disposition:** fixed by `backend-dev`. The header now leads with the fact and
records what the mistake produced. It grepped every remaining mention of `setup`
and confirmed the five survivors are all true. It noted honestly that nothing
structurally enforces `Builder::manage`, and declined to add a test asserting the
shape of source text.

### F5 — no log sink was registered, so several "absorbed but logged" decisions were absorbed silently [minor]

**Location:** no `log` implementation anywhere; 28 `log::` call sites
**Failure scenario:** `colour.rs:155-159` is the only place the remedy for a
pre-WP-10 development store is stated, and it was discarded before being written
anywhere. `events.rs:28-36` justifies not failing a command on a failed emission
because the failure is "logged at error level rather than absorbed silently";
with no sink it was absorbed silently.
**Disposition:** [ADR-0012](../architecture/adr/0012-logging.md), Accepted.
`tauri-plugin-log`, stdout plus a rotating file, webview target off. The
architect found **four** of its own accepted decisions load-bearing on the log
existing and listed them. The substance of the ADR is the content rule: a file
sink creates a new file in `~/.fast-clip/` and therefore a new way to breach spec
§8 criterion 6, so a `log::` call formatting a clip `label` or `value` is
`BLOCK`-level, and `log::error!("{error}")` on a `ClipError` is safe by
construction because no variant carries clip text. Assigned to **WP-02**,
`devops`, with the criterion-6 grep test added to its definition of done.

## The four reported ambiguities — all closed

`backend-dev` reported four; the critic confirmed all four genuine and correctly
classified, and the architect closed each. `Clip.id` gained a validation row. A
failed `update_clips` after a commit is absorb-log-return-null, with the accepted
cost named. "First key" is lexicographically smallest, with the reason it needed
specifying: `serde_json::Map` is a `BTreeMap` only while `preserve_order` is off,
so a transitive dependency enabling that feature would change this contract's
observable behaviour without touching the page. `copy_clip` step 3 finding no row
is `not_found`, not `storage`.

## What the critic checked and found sound

No `println!` anywhere in the backend; all 28 `log::` call sites read, none
formats a label, value, rejected colour token or payload. `ClipRow` and wire
`Clip` hand-write `Debug`; assertions pin it. The `manage` fix is complete — no
`setup` hook exists, and `Store::open_default()` is evaluated on the builder
chain so recovery finishes before a webview exists. `build.rs` cannot affect the
shipped binary: `cargo:rustc-link-arg-tests` applies to `tests/` targets only.
The dev-dependency reasoning holds — `edition = "2021"`, no `[workspace]`, no
parent manifest, so resolver 2 applies and `Cargo.lock` gains nothing.

No assertion that cannot fail, and no mock so complete the real code never runs:
`mockTauri.ts` replaces `window.__TAURI_INTERNALS__.invoke` only, so the real
`invoke()` and `listen()` execute. Lanes held in both directions.

## What could not be verified

The critic has no shell and asserts no command output. Specifically unverifiable
by static read: `tauri`'s own `test` feature contents; whether `rusqlite`'s error
`Display` can embed a bound parameter value, moot today with no sink and material
the day one is installed; jsdom's limits as the test report states them; and
rendered pixels for focus indicators, truncation and contrast at 250px.

## Recorded for WP-12

Spec §8 criterion 2 says `icon`, `visible` and `clear_time` "appear nowhere in
the codebase", which the contract contradicts by requiring them to be rejected
*by name*. They appear correctly as rejection fixtures. **The criterion needs
qualifying, not the code.** `spec.md` is the owner's.

## Next action

orchestrator — landed. `get_lock_state` assigned to WP-14, which builds the
startup sequence and cannot wait for WP-07; the log sink assigned to WP-02.
