# ADR 0006 — Tailwind as the only styling system

**Status:** Accepted
**Deciders:** owner

## Context

The pre-refactor build runs **Mantine and Tailwind together**. Mantine supplies
the components and its own theming; Tailwind supplies utility classes used
alongside them. Two systems means two sources of truth for spacing, colour and
typography, and no way to tell which one owns a given rule.

[ADR-0001](./0001-plain-svelte-not-sveltekit.md) removes React, which removes
Mantine with it. That leaves the question of what replaces it.

## Decision

**Tailwind, and nothing else.** No component library, no CSS modules, no
Svelte `<style>` blocks carrying layout or colour.

Svelte scoped styles remain available for the rare case that Tailwind cannot
express — a complex keyframe animation, a `::-webkit-scrollbar` rule — and each
such use is a deliberate exception, not a parallel system.

## Rationale

The window is one 300×600 view with perhaps a dozen distinct elements. A
component library would supply modals, tables, date pickers, autocomplete and a
theming layer, of which this app uses close to none — and it would bring its own
opinions about colour, which fights the fixed [palette](../../product/palette.md).

Tailwind is already a dependency, so this removes one rather than adding one.

The [palette tokens](../../product/palette.md) map cleanly onto Tailwind theme
extensions, which keeps "stored by token name, never hex" enforceable in one
place.

## Consequences

- **Mantine goes entirely**, including `@mantine/core`, `hooks`, `form`,
  `notifications`, `spotlight`, and `postcss-preset-mantine`. Anything Mantine
  provided is now hand-built: the modal, the colour picker replacement, the
  tooltip, the notification.
- **That is more frontend work than swapping a stylesheet.** The old build
  leaned on Mantine for behaviour as well as looks. Accessibility of the
  hand-built modal and tooltip is now `frontend-dev`'s to get right rather than
  inherited.
- Palette tokens are declared once as a Tailwind theme extension. A raw hex in
  a class is a review finding.
- One system means a rule that looks wrong has exactly one place it can come
  from.

## The design must be chosen before it is built

Tailwind decides *how* styles are written, not *what the app looks like*. Six
layout options were mocked up at the real window size and the owner selects one
before [WP-04](../../work/wp-04-frontend-scaffold.md) begins.

The chosen design is recorded on the [palette](../../product/palette.md) page
alongside the tokens. `frontend-dev` implements that design and does not
redesign it mid-package; a change of layout after WP-05 means rebuilding work
that already passed review.

## Rejected

**A Svelte component library** (Skeleton, Flowbite-Svelte, shadcn-svelte). Each
brings a theming layer this app does not need and colour opinions that conflict
with the fixed palette.

**Plain CSS or CSS modules.** Defensible at this size, but it gives up the
constraint Tailwind provides — utilities make ad-hoc values visible in review,
where a stylesheet hides them.
