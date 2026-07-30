# ADR 0013 — The tray asks the webview to focus the PIN prompt, with an event

**Status:** Accepted.
**Deciders:** architect. The product half of the question this ADR does **not**
answer is escalated to the owner — see [Consequences](#consequences).

## Context

[Spec §4.8](../../product/spec.md#48-encryption-and-unlocking) says the tray's
*Unlock FastClip* item "raises the window **with the PIN prompt focused**".

That sentence has two halves owned by two processes. Raising the window is a
window operation and `src-tauri/src/tray.rs` does it. Focusing an input is a DOM
operation and only the webview can do it. Nothing carried the request across, so
[review 012](../../reviews/012-wp-08-tray.md) F2 found the raise implemented and
the focus not — and found it in a state where no frontend code could satisfy the
sentence, because the settings panel can be open over the prompt.

Two facts constrained the answer:

| Fact | Consequence |
| ---- | ----------- |
| [Contract §3](../contract.md#3-events) declared two events, both emitted by commands | A signal originating in a native menu had no shape to take. |
| [ADR-0008](./0008-use-count-stays-backend-side.md) says a tray copy generates no IPC traffic at all | Read as a rule about tray actions in general, it forbids the answer. |

The critic read the second one that way, correctly flagged that resolving it was
not `backend-dev`'s to do, and routed it here.

## Decision

**A third event, `unlock_requested`, emitted by the tray's *Unlock FastClip*
handler after the raise.**

| | |
| --- | --- |
| Name | `unlock_requested`, `snake_case` like everything else on the wire ([contract §0](../contract.md#0-wire-rules)) |
| Payload | `null`. No data. |
| Trigger | The user chose *Unlock FastClip*. No command emits it. |
| Sender's gate | None. The item exists only in a locked menu; the backend does not re-check. |
| Receiver's gate | The frontend acts only while its last `lock_state` says `locked: true`. |
| On failure to emit | Absorbed and logged. The window is raised regardless. |

The full surface is [contract §3](../contract.md#unlock_requested). This ADR
records why.

## Rationale

**The traffic follows the consumer, which is the rule ADR-0008 already
applies.** ADR-0008 kept `use_count` and the tray copy off the seam by asking
one question — who consumes this? — and answering "the tray menu, which is built
in Rust, on the same side as the value". Asked here, the same question gives the
opposite answer: the consumer of an unlock request is the PIN input, which lives
in the webview and nowhere else. ADR-0008 is not bent to allow this and is not
superseded by it; it is applied.

**The event is named for the gesture, not for the reaction.** `unlock_requested`
says what the user did. `focus_pin_prompt` would say what the frontend must do,
and would become a false name the moment the owner decides the item should also
dismiss an open settings panel — which is the open product question this ADR
deliberately leaves open. A gesture name survives either answer with no second
contract change, which is the property the owner's decision must not cost.

**Emitting unconditionally puts one gate in one place.** The backend could check
`store.is_locked()` before emitting. It would remove nothing: an event is
asynchronous, so the frontend must gate on its own last `lock_state` in any case,
and a second gate on the sending side gives one decision two owners — the defect
[contract §0](../contract.md#0-wire-rules) names as two sources of truth.

**The requirement is the outcome, not a call.** The window is being unminimised
and brought forward as the event lands, and a webview that restores focus to
whatever held it before the minimise can undo a focus set too early. The contract
therefore states that the PIN input **holds** focus once the window has settled,
and leaves how to `frontend-dev`. Prescribing a `tick()`, a
`requestAnimationFrame` or a window-focus listener from this page would be an
architect writing frontend code, and would be wrong the first time WebView2
behaved differently from the guess.

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| A frontend-only listener on the webview's window-focus event | It fires on every raise: alt-tab, a click on the taskbar, the window being restored by any means. It cannot distinguish the tray gesture from a user returning to a window they left open on the settings panel, so it must pull focus on every raise or on none. Neither is what §4.8 asks for, and it leaves the product question nowhere to land — dismissing a modal on every window focus is wrong under both answers. |
| The backend calling `eval` on the webview to focus the element | Puts a DOM selector in Rust. The seam stops being describable on one page, and renaming an element in `src/` silently breaks `src-tauri/`. |
| Naming it `focus_pin_prompt` | Names the reaction. See above: one owner decision away from being a lie, and a renamed event is a contract change in both halves. |
| A payload of `{ "source": "tray" }` | Anticipates a second source. The only candidate is a global hotkey, which [spec §6](../../product/spec.md#6-non-goals) rules out. Designing for a requirement the product has explicitly declined is the cost this project pays elsewhere as `icon`, `visible` and `clear_time`. |
| Reword spec §4.8 to drop "with the PIN prompt focused" | Removes a requirement the owner wrote, to save the architect an event. Rejected on lane before merit. |
| Have the tray item invoke a command instead of emitting an event | A command is the frontend asking the backend. This is the backend telling the frontend. Inverting it would mean the webview polling for a gesture, which is the shape [closed question 11](../contract.md#closed-at-g0b) already rejected for lock state. |

## Consequences

- **[Contract §3](../contract.md#3-events) has three events and the frontend
  registers three listeners before its first `invoke`.** The startup sequence's
  step 1 changes; nothing else in the sequence does.
- **This is the first event with no command behind it.** `tests/ipc.rs`'s
  per-command emit table, which asserts `Required`/`Forbidden` for all sixteen
  commands, gains no row — and must not, because a command that emitted this
  event would be a defect.
- **The product question stays open and is escalated.** Whether *Unlock
  FastClip* dismisses an open settings panel is what the user meant by clicking
  it, not how it is built. Until the owner answers, the frontend focuses the PIN
  input and the settings-panel case is left undone and documented in place. The
  event does not change with the answer.
- **The most likely route into the unanswered state is the ordinary one.** The
  *Lock now* control lives in the settings panel and the panel stays open across
  a lock, showing the "Settings while locked" notice. Lock, minimise, come back
  through the tray is the common path, not an exotic one.
- **A missed event costs a click.** No retry, no error surface, no state
  divergence: the window is raised either way and the prompt is on screen.
- **Neither half can be tested end to end.** Driving a native tray menu needs a
  notification area and a message loop, which is the untestable surface
  [WP-08](../../work/wp-08-tray.md) predicted. The frontend's handler is testable
  in isolation by dispatching the event; the backend's emit is testable by
  observing the emit; the join between them is a manual check.
- **The webview gains no new capability.** `core:default` already grants event
  listening, and this event needs nothing else — unlike the tray commands, which
  `src-tauri/capabilities/default.json` denies one by one.

## Revisit if

A second surface can raise the window from outside the webview — a global
hotkey is the candidate, and it is a
[spec §6](../../product/spec.md#6-non-goals) change first. At that point the
payload gains a source discriminant and this ADR is superseded rather than
amended.
