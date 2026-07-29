# WP-10 — Palette and accessibility

**Objective:** a fixed colour palette that is legible for everyone, and a UI
that works without a mouse.

**Depends on:** WP-04.

**Inputs:** [palette](../product/palette.md), spec §7.

## Work

### frontend-dev

Define eight to ten tokens, each with its own fill and foreground. Fill in the
token table and the Mantine migration map on the palette page — that page is
the deliverable, not a side effect.

Requirements: WCAG AA (4.5:1) for label text on its own fill, distinguishable
at 250px, distinguishable under common colour-vision deficiency, legible
against the dark background.

Vary lightness as well as hue. Red against green alone fails roughly 8% of male
users.

Complete the keyboard and focus pass across every interactive element.

### backend-dev

Provide the Mantine-to-token mapping used by the WP-07 migration, taken from
the palette page. An unmappable value falls back to a default token rather than
failing the migration.

### test-engineer

Compute the contrast ratio for every token and fail below 4.5:1. The figures on
the palette page are a claim until a test checks them.

Keyboard operation of every control.

### critic

Verify the contrast claims against the test, not against the table. Check that
no token is stored as hex.

## Definition of done

- The token table and migration map are filled in.
- A contrast test passes for every token.
- Every control is reachable and operable from the keyboard.

## Risks

An accessibility claim that is asserted in prose and never tested drifts the
first time someone adjusts a hue.
