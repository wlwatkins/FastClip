# Review 013 — WP-08 rework: the `unlock_requested` event (G3)

**Reviewed:** `docs/src/architecture/contract.md` §3, §5 q20, §6;
`adr/{0013,0014}.md` and `adr/index.md`; `docs/src/SUMMARY.md`;
`docs/src/product/spec.md` §4.5, §4.8; `docs/book.toml`;
`src-tauri/src/commands/{events,mod,clips}.rs`, `src-tauri/src/tray.rs`,
`src-tauri/tauri.conf.json`; `src/App.svelte`,
`src/lib/components/{PinPrompt,PinInput}.svelte`,
`src/lib/state/lockState.svelte.ts`; `src-tauri/tests/{tray,ipc}.rs`,
`tests/{unlock-requested,event-names,app-inert}.test.ts`,
`tests/helpers/mockTauri.ts`.

**Verdict:** REWORK_ARCHITECTURE — 2 findings, 1 major, 1 minor. **Both closed.**

This round answers [review 012](./012-wp-08-tray.md) F2 and F3. The design
survived intact; both findings were about where an artefact lived, not what it
said.

## Findings

### F1 — the two new ADRs were not in the book, so F3's remedy was unreachable [major]

**Location:** `docs/src/SUMMARY.md:29` — the ADR list ended at `0012 — Logging`.
**Failure scenario:** run `mdbook serve docs`, the reading route `CLAUDE.md`
gives every agent. Open the contract, §3, `unlock_requested`, and follow
"Governed by ADR-0013". There is no page: mdBook renders only what `SUMMARY.md`
links, and `copy_files_except_ext` excludes `.md` from the verbatim copy, so the
file is neither rendered nor copied. The ADR index advertised two rows whose
links 404. `book.toml` enables search, and the index is built from rendered
chapters only, so an agent searching for `unlock_requested` or for the
truncation width finds `contract.md` and never the ADR deciding either.

**This reproduced review 012 F3's own shape one level up.** F3 said the book did
not carry the truncation number. The number moved from a Rust comment into a file
the book did not include. `tray.rs:76-83` cited ADR-0014 as its authority and a
reader of the book could not reach that authority.

**Why it survived scrutiny:** the critic checked every escape route. `book.toml`
declares no preprocessor that could generate entries, and `create-missing = false`
governs the inverse case. All of ADRs 0001–0012 and reviews 002–012 have a
`SUMMARY.md` line, so the omission broke an unbroken convention rather than one
the critic invented. It considered reading "the book" as the source tree — on
which the pages are present and readable on GitHub — and rejected it, because
`CLAUDE.md` names `mdbook serve docs` as the way to read the specification.

**It stated a falsifiable prediction rather than asserting**, having no shell:
*"`mdbook build docs` produces no `book/architecture/adr/0014-tray-label-truncation.html`.
If that file exists, this finding is wrong and I want it recorded as wrong."*
It was right.

**Disposition:** closed by the orchestrator. `SUMMARY.md` is outside the
architect's write scope, which is the reason this sat open across several rounds.

### F2 — `event-names.test.ts` justified itself with a false claim about its sibling [minor]

**Location:** `tests/event-names.test.ts:20-23`. The comment claimed
`unlock-requested.test.ts` "drives the mock with whatever name App.svelte
actually asked for, so it cannot notice the app asked for the wrong one."
**Failure scenario:** change `App.svelte:163` to `"unlockRequested"`. The sibling
fails **four times** — at its hand-typed literal comparison, and because
`setupTauriMock`'s `emit` throws `no listener registered` since it looks the
handler up by the literal the test passes, not by the name the app registered.
The sibling notices immediately.
**Why it survives scrutiny:** the critic looked for a reading that makes the
sentence true. It is not "no dedicated test exists" — the text makes a specific
mechanical claim, and `mockTauri.ts` refutes it. It also checked whether the
sibling post-dated the comment and could have closed a real gap: both files are
part of this same rework.
**Disposition:** closed by `test-engineer`, which **narrowed the claim rather
than rescuing it**. The comment now states the file's real distinct value —
catching an *extra* listener, which the sibling's `arrayContaining` cannot notice
because a superset still contains the subset — and says outright that it is not
the only place a misspelling would be caught.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Review 012 F2 path 1 closed — something focuses the prompt on raise | Yes, at the code layer | `tray.rs:357-364` raises then emits; `events.rs:108-114` emits `()`, serialising to `null`; `App.svelte:163` → `:124-127` gates on `lockState.locked` and calls `focusPin()`; `PinInput.svelte:38-40` is the imperative focus the mount-time effect could not provide |
| Review 012 F2 path 2 (settings panel) | Deliberately open, honestly recorded | Four places agree: `contract.md:1741-1747`, ADR-0013 Consequences, `App.svelte:118-123`, and a test asserting the panel is left untouched |
| Review 012 F3 closed — the width is in the book | Yes, after F1 | ADR-0014, now in `SUMMARY.md` |
| The contract is implementable without inference | Yes, after the architect closed the one gap it named | See below |
| ADR-0013 reconciles with ADR-0008 rather than rationalising | Yes | See below |
| The truncation number is defensible | Criterion yes; number yes, as an estimate | See below |
| Tests prove what their names claim | Yes | Nine assertions checked individually, each shown able to fail |
| No new defect introduced | No | F1 and F2 were both introduced by this round |

## ADR-0008: the reconciliation holds, and the critic's own earlier review was wrong

The critic tested its own review 012 F2 and found the reasoning over-read.

**The letter.** ADR-0008's decision is "**a tray copy** produces no IPC traffic at
all". After ADR-0013 a tray copy still produces none — `clips::copy` does a
lookup, a clipboard write and an increment with no emit, and `refresh` builds a
menu. ADR-0008's text is untouched, not reinterpreted.

**The spirit.** "The traffic follows the consumer" is a new phrase for
ADR-0008's own rule, written at G0b: *"who consumes `use_count`?"* ADR-0013 asks
the identical question and gets the opposite answer, because the PIN input is in
the webview. Application, not invention.

**What was invented was the critic's.** Review 012 F2 wrote "ADR-0008 keeps tray
actions from emitting". ADR-0008 says *a tray copy*. **One word generalised, and
the generalisation used to route a finding.** The routing was still correct — a
new event is the architect's — but the stated reason was over-read, and
ADR-0013's Context is right to attribute that reading to the critic rather than
to the ADR.

## The focus mechanism is load-bearing, not theatre

`focusPin()` does two things and the second changes behaviour. The immediate
`focus()` covers the ordering where the native focus event fires before the IPC
event is delivered — the likely case, since `set_focus()` precedes the emit. A
one-shot listener covers the reverse. The test moves focus to a **decoy** after
the event and asserts the PIN input takes it back, so deleting the arming effect
fails it.

The critic attacked the arming for a misfire and could not produce one that
matters: the listener attaches on the effect flush, before any focus event that
follows the handler; the flag is one-shot and its cleanup removes the listener;
left armed with no event, unmount removes it; and a late misfire refocuses the
PIN input in a locked window, which is the correct place for focus.

**One state where `focusPin()` is a no-op by construction, and it is not a
defect.** During backoff `PinInput` is `disabled` and cannot take focus. The
contract mandates that disabling, and the field is visibly greyed with a
countdown. Nothing refocuses it when the countdown reaches zero either — but that
is identical at launch and predates this rework.

## The truncation number

**The criterion is sound** and depends on no font metric: "about as wide as the
window", derived from spec §1's docked 300-pixel window and spec §4.5's reason
for existing, which is not disturbing the application the user is working in.

**The number is defensible as an estimate, and ADR-0014 says so in the right
place.** 40 × 6.5 + 48 ≈ 308 against a window `tauri.conf.json:17` confirms is
300 logical pixels — both DIP, so the comparison is sound. The critic noted that
two of three terms are eyeballed and stacking them to land within 3% is more
precision than the inputs support, then accepted the ADR's own framing: *"The
metric is an estimate; the criterion is not."* The second argument carries more
weight and is independent of the arithmetic — at 40 characters truncation is the
exception, so the ellipsis keeps meaning "there is more here", which it would not
at a width where every row is cut.

## The inference gap the critic named but could not file

`contract.md:1425` said the frontend acts "only while its last received
`lock_state` says `locked: true`", while step 1 required the listener registered
before step 2 *because* the event can arrive during the sequence. Between those,
a reader had to resolve what "last received" means before any `lock_state` had
arrived.

**The critic could not turn it into a finding because both readings produce the
same observable outcome** — under the strict reading the discarded event is
followed by `PinPrompt` mounting and its `autofocus` effect focusing the field
anyway. It named the sentence and the cheapest fix instead of inflating it.

**The architect has since closed it:** the contract now states that an event
arriving before step 3 is **discarded, not buffered**, with both readings tabled
and the mount-time-focus coincidence flagged as not to be relied on.
`test-engineer` added a test distinguishing discard from replay **via the
reassert arming rather than the final focus state** — because the final state is
identical either way — and named where the test would *not* catch a replay: one
built on a fresh focus mechanism rather than reusing `focusPin()`.

## What the critic verified

Both sides of the wire character for character, including that
`app.emit(UNLOCK_REQUESTED, ())` serialises to `null` and is asserted as the raw
string `"null"` rather than round-tripped through `serde_json::Value` — the
difference between `null` and an absent body.

The event has **no command behind it**: `emit_unlock_requested`'s only caller is
`tray::request_unlock`, so `tests/ipc.rs`'s per-command emit table gains no row,
as ADR-0013 requires. Four Rust-side assertions each shown able to fail,
including one covering four non-unlock menu ids so a prefix-match slip in
`on_menu_event` fails it.

No new disclosure surface: the payload is `null`, the log lines format a constant
name and an emit error, and `Entry`'s hand-written `Debug` still redacts.

**The settings-panel test deliberately asserts nothing about focus**, because
jsdom does not honour `inert` and a focus assertion would encode jsdom's
behaviour rather than the browser's. The file header says so, citing where that
limitation was already established.

## What could not be verified

No shell — every check below is a prediction the critic marked as one.

**Whether WebView2 restores pre-minimise focus, and in what order relative to
IPC delivery.** This is the residue of review 012 F2 path 1 and no test in this
repository can close it. The implementation covers both orderings the critic
could reason about; the one it does not cover is a restoration dispatched to JS
*after* the window's own focus event, in which case the reassert fires too early
and there is no second fallback.

`tests/tray.rs:16-74` carries a step-by-step manual checklist with a negative
case, and both sides state that the join is not observable. **Nothing on disk
records that anyone has run it.**

## Gate status

WP-08 **cannot exit** on this rework. Four items were open; two are now closed.

| # | Item | State |
|---|---|---|
| 1 | The owner's settings-panel answer | **Open.** Spec §4.8 is unmet in the path ADR-0013 documents as the common one |
| 2 | Review 012 F1, the `clipboard-manager` grant | **Closed** — `devops` removed it; verified independently |
| 3 | The manual checklist has no recorded result | **Open, owner.** A native tray cannot be driven from here |
| 4 | F1 above | **Closed** |

Item 2 is worth recording as a process fault rather than a finding: **the
critic's blocker list was built from the architect's, which was itself stale.**
The architect carried F1 as open across several rounds on the strength of the
earlier routing without re-reading `capabilities/default.json`, where it had
already been fixed. That is [defect class 3](../reference/defect-classes.md) —
a record treated as authoritative because it was one's own. The orchestrator made
the identical error the same night on review 008's F2.
