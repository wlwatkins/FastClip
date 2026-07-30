# Review 012 — WP-08 Tray icon (G3)

**Reviewed:** `src-tauri/src/tray.rs` and its nine `tray::refresh` call sites,
`src-tauri/src/commands/{clipboard,clips}.rs`, `src-tauri/src/storage/clips.rs`,
`src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`,
`src-tauri/gen/schemas/desktop-schema.json`,
`src-tauri/tests/{tray,ipc,log_content}.rs`;
`src/lib/components/{PinInput,PinPrompt,TitleBar}.svelte`, `src/App.svelte`.

**Verdict:** REWORK_ARCHITECTURE — 3 findings, 1 major.

**The code is faithful to the book. The book does not say enough.**
`backend-dev` could not have closed F2 or F3 without inventing a requirement,
which `CLAUDE.md` forbids.

## Both assigned checks came back negative

**There is one clipboard implementation**, traced rather than assumed:
`tray::copy_from_tray` → `clips::copy` → `ClipboardWriter::write_text`, and
`copy_clip` → the same `clips::copy` on the same line. `AppClipboard` is the
only production implementor of the trait, `commands/clips.rs:263` is the only
call to `write_text` in the crate, and the frontend imports no clipboard module.
See F1 for the capability, which is a granted permission rather than a second
implementation.

**No clip `value` can reach a menu string, a tooltip, an item id or a log
line.** `ranked_for_tray` selects `id, label` only and `TrayClip` has no `value`
field. `Entry` and `TrayClip` hand-write `Debug` through `Redacted`. The tooltip
is the constant `"FastClip"`. Every `log::` call in `tray.rs` formats a constant
or a `ClipError`, whose `Display` renders discriminants and wire field names
only. `MenuItem::with_id` is called with `None::<&str>` for the accelerator —
muda's only fallible Windows step, and the only one whose error text could echo
a caller string.

## Findings

### F1 — the capability grants the webview its own clipboard write [minor]

**Location:** `src-tauri/capabilities/default.json:11-12`
**Failure scenario:** `invoke("plugin:clipboard-manager|write_text", …)` from
the webview succeeds today. It writes the system clipboard **without reaching
`clips::copy`**, so `use_count` is not incremented and the tray ranking is built
from a subset of the user's copies — the outcome contract §2 `copy_clip` exists
to prevent. Nothing in `src/` makes that call today, so this is a granted
capability rather than a live second implementation.
**Why it survives scrutiny:** the critic checked whether the backend needs the
grant. It does not — `AppClipboard::write_text` calls the plugin's **Rust** API,
which the ACL does not gate. It checked what `clipboard-manager:default` grants:
the generated schema records it as *"No features are enabled by default"*, so
line 12 is the whole of the grant.
**The argument is already made in the same file.** Its twelve
`core:tray:deny-*` entries exist because "a frontend able to call
`core:tray:set_menu` … could put clip text into a native menu with the backend
unaware". Identical argument, answered the opposite way two lines above.
**Disposition:** routed to `devops`. Predates WP-08.

### F2 — the tray's Unlock item raises the window, but nothing focuses the PIN prompt, and in one permitted state nothing can [major]

**Location:** `tray.rs:383-398` (`raise_window`), with `PinInput.svelte:27-29`
and `App.svelte:329-336,429-431`
**Spec §4.8** says the item "raises the window **with the PIN prompt focused**".
The raise is implemented. The focus is not.

**Path 1 — a missing line.** The user clicks the title bar's minimise button;
with `decorations: false` that is the only way to minimise, so it becomes
`document.activeElement`. They choose *Unlock FastClip* from the tray. The
window unminimises and focuses, the webview restores focus to the minimise
button, and the user types six digits into nothing.

**Path 2 — not implementable as written.** The settings button renders in every
phase including `locked`, deliberately — `copy.md` carries a *"Settings while
locked"* sentence. With the panel open, `App.svelte:330` marks the whole subtree
containing `PinPrompt` `inert`. The tray item raises a window showing the
settings panel, with the prompt neither visible nor focusable. **No frontend
code satisfies spec §4.8 in that state**, because nobody has decided whether the
tray item should dismiss the overlay.

**Why it survives scrutiny:** the critic looked for anything that re-focuses the
prompt. `PinInput`'s `$effect` depends on `autofocus` and `inputEl` only, so it
runs at mount; there is no `window` focus listener anywhere in `src/`. It
considered a backend signal and ruled it out of `backend-dev`'s lane — the
contract declares no event for it and ADR-0008 keeps tray actions from emitting,
so that would be a contract change. It considered whether this belongs to WP-07
and concluded it does not: before WP-08 no surface existed that raised the
window into the PIN prompt from outside.
**Marked as a prediction:** whether WebView2 restores `document.activeElement`
on window focus needs a running build. **Path 2 does not depend on it.**
**Disposition:** routed to the architect. The mechanism is an engineering
decision; whether *Unlock FastClip* should dismiss an open settings panel is a
product decision and was **not** delegated — it goes to the owner with a
recommendation.

### F3 — the truncation width is decided in code and nowhere in the book [minor]

**Location:** `tray.rs:63-70`
**Failure scenario:** labels are cut at 40 Unicode scalar values plus an
ellipsis. Spec §4.5 says only "Long labels are truncated in the menu" and WP-08
says only "Truncate long labels". Nothing in `docs/` authorises 40, so there is
no criterion a WP-12 reviewer can check the tray against and the next agent
needing a truncation width has no source to agree with.
**Why it survives scrutiny:** this is **not** filed against `backend-dev`, and
that is the right call. The constant is documented in place as "**Not
specified.** … reported as a contract ambiguity rather than presented as a
decision", which `reporting.md` explicitly values above the code. The defect is
that the book still does not carry the number — a decision living only in a Rust
comment is one the next reader will re-make.
**Disposition:** routed to the architect, with the spec wording proposed for the
owner to apply.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Right-clicking the tray copies without raising the window | Yes, by reading | `tray.rs:333-344` routes a `clip:<uuid>` id to `copy_from_tray`; `:353-375` calls `clips::copy` and never touches the window. **No test executes this** |
| At most ten entries, correctly ranked | Yes | `storage/clips.rs:119` — `ORDER BY use_count DESC, position ASC LIMIT ?1`. `position` is `NOT NULL UNIQUE`, so the order is **total** and stability is not in question. Tested for empty, list order, count beating position, ties by list order, ties after a reorder, and the eleventh |
| A tray copy increments `use_count` | Yes | One `copy` for both callers; one assertion that it is exactly one; the menu re-ranks after it |
| One clipboard implementation exists | Yes, in code | One `write_text` impl, one caller, two callers of `clips::copy`. See F1 for the capability |
| Rebuilds on every clip-list and `use_count` change | Yes | Nine call sites; `tests/ipc.rs:1349-1565` walks all sixteen contract §2 commands asserting `Required`/`Forbidden` per command, with `CASES.len() == 16` forcing a row for any new command |
| A locked store shows one *Unlock FastClip* item and no labels | Yes | `tray.rs:129-153` takes the lock branch first; the `Err(Locked)` arm makes losing the race also produce `[Unlock]`. Text matches `copy.md:78` character for character |
| A locked store's tray raises the window **with the PIN prompt focused** | **No** | F2 |
| Truncation counted in Unicode scalar values | Yes | Tested with astral-plane input |
| The untested surface is recorded, not implied | Yes | `tests/tray.rs:1-10` names exactly what is not covered |

## What the critic verified beyond the tests

**The rebuild trigger set, enumerated against the mutation sites rather than
against the comments.** Create, update, delete, reorder, copy, import, unlock,
lock, enable and disable all refresh; export, list, both settings commands,
`get_lock_state` and `change_pin` correctly do not. `Store::lock()` is called
from exactly one place and `status.unlocked = false` is written in exactly one
place, so there is no route to a locked store that skips the rebuild.

**Unusually for this project, the completeness of that set is enforced by a test
rather than asserted in prose.** `CASES.len() == 16` fails if a command is added
without a row. That is the correct answer to defect class 1 in
[recurring defect classes](../reference/defect-classes.md).

**The refresh observability is honest about its own blindness.**
`install_recorder` asserts that the recorder was actually installed and that
`Debug` is not filtered, because **both failure modes would read as "zero
rebuilds"** and send the reader to the wrong file. `tests/log_content.rs`
likewise proves the sink is live with a positive control before asserting
absence. The critic singled this out as the rarest thing in the suite.

**The faulted-store branch returns an empty menu rather than an *Unlock* item**,
which is right: a PIN prompt over a plaintext store is a prompt the user cannot
satisfy.

`entries` returns owned data and drops the connection guard before `build_menu`
touches the main thread. `tests/tray.rs`'s single test is not decoration —
emptying `bundle.icon` would fail it, and nothing else guards that list.

## What could not be verified

No shell. Three things need a running build and are marked predictions, not
findings: that `Menu::new` called from the main thread runs inline rather than
deadlocking on `run_on_main_thread`; that the `TrayIcon` dropped at `tray.rs:270`
stays alive because the manager retains a clone — `refresh` depends on
`tray_by_id` finding it, so **one manual run confirms both**; and the WebView2
focus-restoration half of F2 path 1.

**Nothing exercises `build_menu`, `install`, `refresh`'s `set_menu` branch,
`copy_from_tray` or `raise_window`.** That is the gap WP-08 predicted in its
Risks section, and `tests/tray.rs` records it in the right words rather than
letting silence imply coverage.

The critic was given no implementation or test report — `reporting.md:47-52`
files neither to disk — so it judged artefacts, not claims. It notes it cannot
confirm the truncation width was reported to the orchestrator as an ambiguity,
only that the code says it should have been. It was.

## Out of lane, noted and not filed

`src-tauri/src/commands/mod.rs:3-15` still says eleven of sixteen commands are
implemented and that "the remaining five … belong to WP-07", while
`command_handler!` registers all sixteen. `tests/ipc.rs:1344` and `:1482` say
"six call sites today"; there are nine. Neither produces a wrong result — the
`CASES` table forces the real set to be correct — so neither is a finding.
Both are instances of [defect class 1](../reference/defect-classes.md) and
belong to whoever lands WP-07's paperwork.
