# Review 011 — WP-11 Copy deck and README (G3)

**Reviewed:** `docs/src/product/copy.md`, `README.md`, `src/lib/copy.ts`,
`src/lib/errorMessage.ts`, `src/lib/contract/validate.ts`, every component in
`src/lib/components/`, `src/App.svelte`, `src-tauri/src/tray.rs` (one string),
`assets/images/screenshot.png`.

**Verdict:** REWORK_IMPLEMENTATION — 4 findings, 3 major, none security.

Measured against the tree as read on 2026-07-30. `SettingsPanel.svelte` was 706
lines at that moment. No finding depends on a line count in that file — the
lesson from review 008's F1 was applied without being asked for.

## The BLOCK-level check passed

The work package makes overstating either security string a `BLOCK`, and names
the trap: *"The temptation at this point is to delete the password warning
outright, because the encryption work is finished and it feels earned. It is
not. The threat model did not change."*

It was not deleted. It was reworded, and every claim describes **less** than the
ADRs grant:

| README | ADR |
|---|---|
| "By default, clips are stored unencrypted on this computer… If you never open Settings, your clips are plaintext." | ADR-0004:33 "opt-in, off by default"; ADR-0004:95 "must describe **the default**, which is unencrypted" |
| "does not make FastClip a password manager, and it does not protect against another program running under your Windows account — that program can read the same files FastClip can." | ADR-0002:57-61 "does not defend against an attacker who can execute code as the logged-in user"; ADR-0004:82-84 |
| "What it does protect is the file leaving this machine: cloud sync, backup images, a shared or resold computer, a disk without BitLocker." | ADR-0002:50-54 lists that exact set |

The forgotten-PIN paragraph reproduces ADR-0004:54-55 without softening.
"your clips are secure" appears nowhere: a repo-wide grep returns two hits, both
prohibitions. No occurrence of *secure*, *safe* or *protected* remains in
`README.md`, and the same grep over `src/` returns nothing.

## Findings

### F1 — the truncation rule for `import.field` is in neither the deck nor the code [major]

**Location:** `src/lib/errorMessage.ts:65-66`; the rule belongs at `copy.md:152`
and is delegated by `contract.md:1518`.
**Failure scenario:** an import file whose first clip carries a multi-megabyte
unrecognised key. `export_file.rs:247-252` returns that key verbatim as `field`
— review 008 already established that a JSON object key has **no** length bound.
`describeImportError` appends `("${field}")` unbounded, `handleImport` passes it
to `showToast`, and `Toast.svelte:18` renders the whole thing in one `<span>` in
a fixed bottom bar for two seconds. The window is covered by a wall of the
user's own file content while WebView2 lays out a multi-megabyte text node.
**Why it survives scrutiny:** the critic looked for the truncation elsewhere. A
grep for `truncat` across the tree returns only `ClipRow.svelte:123`'s CSS class
on a clip label. Not `BLOCK`: contract §4 argues the echoed key is the user's
own content, so this is a rendering defect and not a disclosure, and Svelte
escapes text interpolation so it is not injection.

**This rule was added by the architect in response to review 008 and never
implemented.** The same gap as the export-version rule, found the same day.

### F2 — the deck is not exhaustive; six sentences reach the user with no deck row [major]

**Location:** `errorMessage.ts:15-18`, `:94`, `:105`; gaps at `copy.md:30-39`
and `:113`.
**Failure scenario:** reorder a row while the stored id set has changed.
`reorder_clips` rejects with `invalid_input { reason: "not_a_permutation" }`,
`App.svelte:318` toasts it, and the user reads **"The list changed elsewhere.
Reloading."** — a sentence in no book page. `copy.md:106` says the message for
`invalid_input` is "the Validation sentence for `reason`", but the table has
rows for five of the nine declared `InvalidReason` values. `not_permitted`,
`unknown_field` and `malformed_uuid` are in the same position.
**Why it survives scrutiny:** `describeError` must be total to compile, so the
developer had to write *something* — that is the point. The deck owed a decision
it did not make. The standard is the deck's own: `copy.md:37` carries a row for
`not_a_palette_token` **because the reason is declared**, even though the UI
cannot reach it.

**The `locked` case is worse than a gap.** `errorMessage.ts:94` returns "FastClip
is locked.", which **contradicts** `copy.md:113`'s explicit decision that this
variant gets no separate sentence because the PIN prompt is the answer.
Reachable: click a clip to copy at the moment a manual lock lands (ADR-0010) and
the sentence toasts on top of the prompt.

### F3 — four user-facing strings are still authored inline [major]

| Location | String |
|---|---|
| `src/App.svelte:395` | `aria-label="Clips"` |
| `src/lib/components/PinPrompt.svelte:98` | `aria-label="Unlock FastClip"` |
| `src/lib/components/ColourPicker.svelte:8` | `legend = "Colour"` default |
| `SettingsPanel.svelte:321`, `:367` | `filters: [{ name: "FastClip export" }]` |

Two are the drift class this project has now produced five times.
`ColourPicker`'s default `legend` renders the sr-only fieldset legend and the
radiogroup's `aria-label`; `ClipForm.svelte:169` never passes `legend`, so a
screen-reader user hears "Colour" from the inline default while the visible span
at `ClipForm.svelte:168` renders `CLIP_COLOUR_FIELD_LABEL` from the deck.
`PinPrompt.svelte:98` is a **third** copy of a string `copy.md:78` reserves for
the Rust tray item at `tray.rs:83`.

**Why it survives scrutiny:** the critic checked whether `aria-label` is exempt.
`copy.md:1` — "Every string the user reads, in one place" — puts it in scope. It
excluded class names, `data-tauri-drag-region`, `role`/`id` values, and
`defaultPath: "fastclip-export.json"` (a filename). It also dropped
`COLOUR_META`'s nine display labels rather than pad the count: they are one
machine-checked declaration per token with a load-time completeness assertion at
`colour.ts:46-50`, which already satisfies "written once, deliberately".

### F4 — the README screenshot depicts the pre-refactor UI [minor]

**Location:** `README.md:38`, `assets/images/screenshot.png`.
**Failure scenario:** the image shows filled pill buttons with tag icons, a
floating green "+", a bottom-left settings control and native decorations
including a maximise button. The shipped UI is the rail layout: flat rows with a
3px colour rail, a custom title bar with settings/minimise/close and no maximise
(`decorations: false`), a "New clip" text button and a search control. A user
comparing the README to the app sees controls that do not exist and does not see
search or drag handles, which do.
**Why it survives scrutiny:** the critic **opened the PNG** rather than
inferring from its filename. Minor because the fix needs a human to run the app
and take a capture, not because the claim is soft.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| No user-facing string authored inline | **No** | F3 |
| Every contract error variant has a message | Yes, at the `kind` level | All 13 `ClipError` kinds have a branch; all 9 `InvalidReason`, all 5 `ImportReason` and both `crypto` reasons keyed in `Record<>` maps. Set difference empty in **both** directions — no orphan message for a deleted variant. Qualified by F2 |
| README security claim matches ADR-0002 | Yes | Table above |
| "your clips are secure" appears nowhere | Yes | Two hits, both prohibitions |
| "Secure local storage" line fixed | Yes | No *secure*/*safe*/*protected* remains in `README.md` |
| Feature list true, completed to-dos removed | Partly | Every listed feature verified against code; F4 |

## What the critic verified

**Deck fidelity where a row exists is character-perfect.** Every string in
`copy.ts` and `errorMessage.ts` with a `copy.md` row was compared character for
character — the export and irreversibility warnings, the unreadable-store
sentence including both em dashes, both crypto sentences, all five import
reasons and both assembly forms, the five present validation sentences, the
backoff and attempts-remaining plurals, every PIN dialog title and label, and
the three toasts. No divergence. The one string the deck places in Rust,
`tray.rs:83`, matches `copy.md:78` exactly.

**A raw error cannot reach the user.** `validate.ts:230-231` maps an
unrecognised `kind` to `{ kind: "internal" }`, and every variant with a
malformed payload does the same rather than passing the payload through. The
sole interpolation of backend text is `import.field`, which the contract
compels — F1 is its length, not its presence. `CommandError.message` embeds only
`error.kind` and nothing in `src/` reads it.

**No PIN reaches a message.** `PinPrompt` clears `pin` on both success and
failure before any branch runs, and no function in `copy.ts` or
`errorMessage.ts` takes a PIN argument.

**Every README feature checked against code** — create/edit/delete, click to
copy, colour choice, always-on-top, drag and keyboard reorder, label and value
search, JSON export and import, and PIN protection gated by DPAPI as well as the
PIN. None is absent. The five remaining to-do items are all genuinely unbuilt.

**The tests that touch this copy can fail.**
`settings-panel-export-warning.test.ts` asserts on literal expected text rather
than importing the constants it checks, so a wording change breaks it. The
critic went looking for a tautology and found the opposite.

## What could not be verified

No shell. **Prediction, unverified:** `tsc` and `svelte-check` currently enforce
the `kind`-level exhaustiveness of `describeError`, because the switch has no
`default` and the annotated return type is `string`.

**Nothing on disk enforces agreement between `copy.md` and `copy.ts`.** That
guarantee rests entirely on review. Worth an assertion if `test-engineer` is
ever dispatched here — and the package assigns no `test-engineer` lane, which is
why the critic reviewed artefacts directly rather than checking claims against
reports that do not exist.

`assets/images/demonstration.gif` was not opened. It is from the same era as the
screenshot and probably carries the same defect.

`ClipForm.svelte:86-95` renders a server-side `invalid_input` for `label` or
`value` as a form-level alert rather than beside the field, which would make the
deliberately field-agnostic sentences at `copy.md:41-43` ambiguous. The critic
could not construct an input that reaches it — `validation.ts` mirrors the
backend closely enough that the client rejects first in every case tried,
including the `U+0085` and whitespace-only cases the file's own comment
anticipates. **Recorded as unreached, not filed as a finding**, which is the
right call: an unreachable path is not a defect, and inventing one to pad the
count would be worse than silence.

## Disposition

F1, F2 and F3 routed to `frontend-dev`, with `copy.md` in scope — the package
assigns the deck to that lane. Where the deck and the code disagree the deck is
the decision; where the deck never made one, `frontend-dev` makes it and writes
it down.

F4 routed to the owner. It needs a capture from a running app, which no agent
can produce. It does not hold the package.
