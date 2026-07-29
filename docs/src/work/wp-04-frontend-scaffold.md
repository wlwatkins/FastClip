# WP-04 — Frontend scaffold, React removed

**Objective:** stand up the Svelte 5 application shell and remove React and
Mantine. Tailwind stays and becomes the only styling system
([ADR-0006](../architecture/adr/0006-tailwind.md)). No features yet.

**Depends on:** WP-01.

**Inputs:** [contract](../architecture/contract.md),
[ADR-0001](../architecture/adr/0001-plain-svelte-not-sveltekit.md),
[ADR-0006](../architecture/adr/0006-tailwind.md),
[the chosen design](../product/palette.md).

**Precondition:** met. The layout is **Rail**, recorded on
[the palette page](../product/palette.md) with its row, action and icon rules.

## Work

### frontend-dev

Scaffold plain Svelte 5 with runes, Vite and TypeScript. Not SvelteKit — see
ADR-0001; SvelteKit idioms will not resolve.

Remove React, React DOM, `@dnd-kit`, `uuid`, and **all of Mantine** —
`@mantine/core`, `hooks`, `form`, `notifications`, `spotlight`, and
`postcss-preset-mantine`. The frontend no longer generates identity, so `uuid`
has no caller. Delete `src/Classes/FastClip.tsx` and the empty
`src/cssVariableResolver.ts`.

**Tailwind stays** and becomes the only styling system
([ADR-0006](../architecture/adr/0006-tailwind.md)). Declare the palette tokens
as a theme extension so a raw hex in a class is visible in review.

Everything Mantine supplied is now hand-built — modal, tooltip, notification,
colour selection. That is behaviour as well as looks, and its accessibility is
yours to get right rather than inherited.

Generate TypeScript types from the contract. Write the boundary validator that
every IPC payload passes through. The React code cast payloads with
`as Array<FastClip>`; that unchecked cast is the specific defect this package
exists to prevent recurring.

Preserve window behaviour: 300×600, undecorated, transparent, legible at 250px.

### devops

Update the CI frontend job for the new toolchain. Confirm the checks still
gate.

### test-engineer

One test that the boundary validator rejects a malformed payload loudly rather
than rendering it.

### critic

Check that no React or Mantine remains, that Tailwind is the only styling
system, that no raw hex appears in a class, and that no unchecked cast crosses
the IPC boundary.

## Definition of done

- `npm ls` shows no React and no Mantine. Tailwind remains, alone.
- The Rail layout is recognisable in the shell: flat rows, 3px colour rail,
  actions hidden until hover.
- Icons are bundled inline SVG. No request leaves the application
  ([spec §2](../product/spec.md#2-users)).
- The app builds, runs, and shows an empty shell.
- Every IPC payload is validated, never cast.

## Risks

Porting the React component structure verbatim carries its bugs across. Rebuild
from the specification, not from the old components.
