# Colour palette

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
   rewriting user data.
6. Every existing Mantine name maps to exactly one token.

## Tokens

*`frontend-dev` fills this in. One row per token.*

| Token | Fill | Foreground | Contrast | Migrates from (Mantine) |
| ----- | ---- | ---------- | -------- | ----------------------- |
|       |      |            |          |                         |

## Migration map

*To be filled in.* Every Mantine colour name reachable in existing user data
needs a row. The current UI offers the standard Mantine palette, so assume all
are in use.

Unknown values fall back to a default token rather than failing the migration.
Losing a clip's colour is acceptable; losing the clip is not.

## Verification

The contrast figures above are a claim. `test-engineer` writes a test that
computes the ratio for every token and fails below 4.5:1. An accessibility
claim that is never checked drifts the first time someone tweaks a hue.
