# ADR 0014 — Tray menu labels are cut at 40 characters

**Status:** Accepted.
**Deciders:** architect.

## Context

[Spec §4.5](../../product/spec.md#45-tray-icon-with-right-click-copy) says "Long
labels are truncated in the menu" and names no width.
[WP-08](../../work/wp-08-tray.md) says "Truncate long labels" and names none
either. A `label` may be up to 100 characters
([spec §3](../../product/spec.md#3-data-model),
[contract §1](../contract.md#1-data-types)).

`backend-dev` implemented 40, documented the constant in place as not specified,
and reported it as a contract ambiguity rather than presenting it as a decision —
which is what [reporting](../../process/reporting.md) asks for.
[Review 012](../../reviews/012-wp-08-tray.md) F3 filed the gap against this page
rather than against the code, on the grounds that a number living only in a Rust
comment is one the next reader re-makes.

## Decision

**A label is cut at 40 Unicode scalar values, and a cut label gains one ellipsis
character.** Truncation happens before Win32 escaping, so a doubled `&` does not
spend the user's budget.

Named in code as `LABEL_LIMIT` in `src-tauri/src/tray.rs`. Nothing on the wire
changes: the tray menu is built entirely inside the backend
([ADR-0008](./0008-use-count-stays-backend-side.md)), so this is not a contract
question and does not appear in [the contract](../contract.md).

## Rationale

**The criterion is the window.** FastClip's window is 300 pixels wide and is
expected to sit docked at a screen edge, beside the application being worked in
([spec §1](../../product/spec.md#1-purpose)). A tray menu two or three times that
width reads as belonging to a different application, and it covers the thing the
user is working in — which is the surface
[spec §4.5](../../product/spec.md#45-tray-icon-with-right-click-copy) exists to
leave undisturbed, since the whole point of a tray copy is not raising the
window.

**The arithmetic that turns the criterion into a number**, in device-independent
pixels at the default Windows text scale:

| Term | Value |
| ---- | ----- |
| Menu font | Segoe UI at 9 pt — 12 DIP — which is the Windows 10 and 11 default for menu text |
| Mean advance, mixed-case Latin, at that size | ≈ 6.5 DIP per character (`i` ≈ 3, `x` ≈ 6, `n` ≈ 7, `m` ≈ 10, space ≈ 3.5) |
| Menu item chrome | ≈ 48 DIP: the left check/icon gutter Windows reserves whether or not items carry icons, plus right padding. These items carry no icon and no accelerator, so there is no accelerator column. |
| **40 characters** | 40 × 6.5 + 48 ≈ **308 DIP** — the same size as the 300 DIP window, not a different one |
| 100 characters, the label maximum | ≈ 700 DIP: more than twice the window, and over a third of a 1920 DIP desktop |

**And a reason from below: 40 is longer than nearly every label a person writes
for a snippet.** "Support greeting", "Ticket template — escalation", "AWS prod
ssh" are 16, 29 and 12. Truncation is therefore the exception. A width at which
most rows are cut communicates nothing, because every row would end the same way
and the ellipsis would stop meaning "there is more here".

**The metric is an estimate; the criterion is not.** The number was derived from
published font metrics rather than measured on a running build, and a native menu
cannot be measured by an automated test — that is the untestable surface
[WP-08](../../work/wp-08-tray.md) predicted in its Risks section. If a manual
check shows the menu materially wider than the window, **what changes is the
constant, not the rule.** The rule is "about as wide as the window".

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| No limit — let the menu size itself to its widest label | One 100-character label sets the width of all ten rows, so the menu's width is decided by the user's least typical clip. Spec §4.5 requires truncation in any case. |
| Measure the text against a pixel budget at build time | Needs a device context and the menu's own font at the current DPI, inside a rebuild that runs on **every copy** — the hottest path in the application ([ADR-0009](./0009-durability-level.md)). It buys accuracy against a value that changes only when the user changes their system font. |
| A round 50, or the 100-character maximum halved | Numbers with no criterion behind them. The next reader would derive their own, which is the defect this ADR closes rather than relocates. |
| Truncate on bytes | Splits a multi-byte character. [Contract §0](../contract.md#0-wire-rules) counts every length in Unicode scalar values, and the tray has no reason to be the one place that does not. |
| Escape `&` before truncating rather than after | Win32 reads a single `&` as a mnemonic marker, so a literal one must be doubled. Doubling first would make a label of ampersands show half as much of the user's text as any other label — a rendering rule silently changing how much content the user sees. |
| Truncate the middle, keeping the tail | The distinguishing part of a snippet label is at the front: "Ticket template — escalation" and "Ticket template — refund" differ at the end, but so would every pair of labels a middle-truncation kept the end of. Front-anchored truncation matches how the list in the window reads. |

## Consequences

- **A truncated item is 41 characters**, not 40. The budget is on the user's
  text; the ellipsis is the application's mark on it and does not come out of it.
- **Two clips whose labels differ only after character 40 render identically in
  the menu.** Accepted. The window is where clips are managed and it shows the
  full label; the tray is a shortcut, not a browser.
- **[Spec §4.5](../../product/spec.md#45-tray-icon-with-right-click-copy) should
  gain the number**, so that a reader looking for the requirement finds it on the
  page that states requirements. The exact sentence is escalated to the owner.
  Until it is applied this ADR is the authority, and §4.5 is silent rather than
  wrong.
- **The doc comment on `LABEL_LIMIT` stops saying "Not specified"** and cites
  this page instead. That is the only code change this ADR asks for.
- **A `WP-12` reviewer now has a criterion.** Before this page there was nothing
  to check the tray against.

## Revisit if

The window's width changes from 300 pixels, since the number is derived from it.
Or if a manual check on a running build finds the menu materially wider than the
window, in which case the constant moves and the criterion does not.
