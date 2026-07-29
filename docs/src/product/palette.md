# Design and palette

## Chosen layout

**Status:** CHOSEN — **Rail**. Option 1 of the six mocked at 300×600 in
`design-options.html`.

| | |
| --- | --- |
| Option | **Rail** |
| Clip row | Flat, no fill and no border. A 3px rounded colour rail at the left edge, inset 4px, spanning the row minus 8px vertically. |
| Row background | Transparent at rest; `#1a1f27` on hover. |
| Actions | Edit and delete, hidden until hover, right-aligned. They sit on a scrim in the hover colour so they never overlap the label. |
| Label | Single line, truncated with an ellipsis. Clearance of 62px on the right for the action pair. |
| Icons | Inline SVG, Material outlined style, 1.7px stroke. 17px in the toolbar, 15px in row actions. **Bundled, never fetched** — the current build pulls icons from `api.iconify.design` on every launch, which violates [spec §2](./spec.md#2-users). |

## Why

Rail is the densest of the six and the quietest. Colour identifies a clip
without competing with the label, which matters at 300px wide where a filled
button leaves little room for text.

Hiding the row actions until hover is what makes the density work: the list
reads as content rather than as a wall of controls.

## Binding

`frontend-dev` implements this and does not redesign it mid-package. A layout
change after [WP-05](../work/wp-05-crud.md) means rebuilding work that already
passed review.

Refinements to spacing, radius and the exact hover colour are expected during
[WP-10](../work/wp-10-palette.md). A different layout is not.

Once recorded this is binding. `frontend-dev` implements it and does not
redesign mid-package — a layout change after [WP-05](../work/wp-05-crud.md)
means rebuilding work that already passed review.

Styling is Tailwind only ([ADR-0006](../architecture/adr/0006-tailwind.md)).

## Colour palette

**Status:** Requirements agreed, values not yet chosen. `frontend-dev` fills
this in at G1; `critic` verifies the contrast claims at G3.

Colour is a scanning aid. With thirty clips it is how a user finds one without
reading. The set is closed rather than free because free choice lets a user
pick a fill that renders their own label illegible.

## Requirements

1. Eight to ten hues, distinguishable at a 250px-wide button.
2. Label text meets WCAG AA (4.5:1) against its own fill. Each token ships its
   own foreground colour.
3. Legible against the dark application background.
4. Distinguishable under common colour-vision deficiency. Vary lightness as
   well as hue; red against green alone fails roughly 8% of male users.
5. Stored by token name, never hex, so the palette can be retuned without
   rewriting user data. Declared once as a Tailwind theme extension; a raw hex
   in a class is a review finding.
6. Tokens are chosen for legibility alone. There is no obligation to
   approximate any existing colour —
   [ADR-0003](../architecture/adr/0003-no-legacy-migration.md) removed the
   migration, so no stored value is ever converted.

## Tokens

*`frontend-dev` fills this in. One row per token.*

| Token | Fill | Foreground | Contrast |
| ----- | ---- | ---------- | -------- |
|       |      |            |          |

## Default

New clips get a default token rather than an arbitrary starting colour. The
current build initialises the picker to the literal
`'rgba(47, 119, 150, 0.7)'`, which is how the colour-reset defect in
[inherited debt](../reference/debt.md) hides — every edit silently rewrites the
stored value to it.

Pick a default deliberately and name it in the table above.

## Verification

The contrast figures above are a claim. `test-engineer` writes a test that
computes the ratio for every token and fails below 4.5:1. An accessibility
claim that is never checked drifts the first time someone tweaks a hue.
