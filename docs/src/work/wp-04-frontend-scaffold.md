# WP-04 — Frontend scaffold, React removed

**Objective:** stand up the Svelte 5 application shell and remove React,
Mantine and Tailwind. No features yet.

**Depends on:** WP-01.

**Inputs:** contract, [ADR-0001](../architecture/adr/0001-plain-svelte-not-sveltekit.md).

## Work

### frontend-dev

Scaffold plain Svelte 5 with runes, Vite and TypeScript. Not SvelteKit — see
ADR-0001; SvelteKit idioms will not resolve.

Remove React, React DOM, Mantine, Tailwind, `@dnd-kit` and `uuid`. The frontend
no longer generates identity, so `uuid` has no caller. Delete
`src/Classes/FastClip.tsx` and the empty `src/cssVariableResolver.ts`.

Generate TypeScript types from the contract. Write the boundary validator that
every IPC payload passes through. The React code cast payloads with
`as Array<FastClip>`; that unchecked cast is the specific defect this package
exists to prevent recurring.

Pick the styling system the architect specified and use only that one.

Preserve window behaviour: 300×600, undecorated, transparent, legible at 250px.

### devops

Update the CI frontend job for the new toolchain. Confirm the checks still
gate.

### test-engineer

One test that the boundary validator rejects a malformed payload loudly rather
than rendering it.

### critic

Check that no React or Mantine remains, and that no unchecked cast crosses the
IPC boundary.

## Definition of done

- `npm ls` shows no React, Mantine or Tailwind.
- The app builds, runs, and shows an empty shell.
- Every IPC payload is validated, never cast.

## Risks

Porting the React component structure verbatim carries its bugs across. Rebuild
from the specification, not from the old components.
