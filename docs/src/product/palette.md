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

**Status:** Chosen at WP-10. Nine tokens, values below; `critic` verifies the
contrast claims against the test at G3.

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

Nine tokens: `red`, `amber`, `lime`, `green`, `teal`, `blue`, `violet`,
`pink`, `slate`. Fill and foreground are Tailwind theme entries
(`src/app.css` `@theme`, `--color-clip-<token>` and
`--color-clip-<token>-fg`); nothing in a component holds a literal hex.

Contrast is the WCAG relative-luminance ratio between Foreground and Fill,
computed to two decimal places — the formula in
[WCAG 2 §1.4.3](https://www.w3.org/TR/WCAG21/#contrast-minimum), not
estimated. Four tokens (`amber`, `lime`, `green`, `teal`) are light fills
with a dark foreground; five (`red`, `blue`, `violet`, `pink`, `slate`) are
dark fills with a light foreground — the split itself widens the lightness
spread the CVD requirement calls for, on top of the per-token spread below.

| Token | Fill | Foreground | Contrast |
| ------ | ------- | ------- | ------- |
| red | `#ce2a38` | `#f5f7fa` | 4.88:1 |
| amber | `#db994d` | `#12151b` | 7.55:1 |
| lime | `#b4d47d` | `#12151b` | 11.02:1 |
| green | `#6cd091` | `#12151b` | 9.64:1 |
| teal | `#48cbbe` | `#12151b` | 9.19:1 |
| blue | `#296dbb` | `#f5f7fa` | 4.89:1 |
| violet | `#8050d3` | `#f5f7fa` | 4.88:1 |
| pink | `#c32e82` | `#f5f7fa` | 4.85:1 |
| slate | `#636c83` | `#f5f7fa` | 4.89:1 |

Every ratio clears 4.5:1 with a margin of at least 0.35, rather than sitting
on the boundary, so a later hue nudge that shaves a few hundredths off does
not silently fail the requirement it currently passes.

**Lightness (HSL `L`), read alongside hue for the CVD requirement:** blue 45,
slate 45, pink 47, red 49, teal 54, violet 57, amber 58, green 62, lime 66.
Red and green — the pair that fails outright under a hue-only palette — sit
13 points apart, the widest practical gap available once both also have to
clear their own contrast floor. No two tokens share both a lightness band and
an adjacent hue.

All nine fills also clear 3:1 against the application background (`#18181b`)
and the row hover colour (`#1a1f27`, below) — not a requirement for the rail,
which is decorative rather than text, but the margin exists rather than being
spent, so the rail reads at 250px without the user leaning on colour alone.

## Default

New clips get a default token rather than an arbitrary starting colour. The
current build initialises the picker to the literal
`'rgba(47, 119, 150, 0.7)'`, which is how the colour-reset defect in
[inherited debt](../reference/debt.md) hides — every edit silently rewrites the
stored value to it.

**Default: `slate`.** It is the one token that carries no category meaning —
every hue in the other eight reads as a choice the user made, so a clip that
has not been assigned a colour should not resemble one that was deliberately
set to `red` or `blue`. `slate`'s desaturation (12% versus 50–66% for the rest)
keeps it visually recessive at 250px: it does not compete with a hue the user
picked on purpose.

## Verification

The figures above were a claim before this package: computed by hand against
the WCAG formula and recorded on this page, with nothing yet checking that
`src/app.css`'s actual theme values matched them. `test-engineer`'s
`tests/palette-contrast.test.ts` discharges that: it reads the
`--color-clip-<token>` / `--color-clip-<token>-fg` custom properties straight
out of `src/app.css` — not this page's table — and independently recomputes
the WCAG 2 §1.4.3 contrast ratio for every token, failing below 4.5:1. It does
not assert against the numbers above; it would catch a hue nudged in
`app.css` without this table being updated to match, which asserting against
the table would not. `test-engineer` proved the test's own sensitivity by
perturbing `pink`'s foreground and watching it fail. The test passes as of
WP-10, so requirement 2 — the only figure this page states as a ratio — is
verified, not merely claimed.

Requirements 1, 3 and 4 (distinguishable at 250px, legible against the dark
background, distinguishable under colour-vision deficiency) are **not**
covered by that test or any other in the toolchain: nothing here renders
pixels or simulates colour-vision deficiency. The lightness-spread reasoning
above is a design argument for those three, checked by a human at review, not
a number a test recomputes. A later reader relying on this page should treat
requirement 2 as VERIFIED and requirements 1, 3 and 4 as argued, not tested.
