# ADR 0001 — Plain Svelte + Vite, not SvelteKit

**Status:** Accepted
**Deciders:** owner, orchestrator

## Context

The frontend is being rewritten from React 18 + Mantine to Svelte 5 with runes.
SvelteKit is the Svelte ecosystem's default entry point and Tauri documents a
SvelteKit path, so choosing plain Svelte is a deviation worth recording.

## Decision

Plain Svelte 5 + Vite + TypeScript. No SvelteKit.

## Rationale

FastClip is a single-view 300×600 undecorated window. It has no routing, no
server, no server-side rendering, and no data-loading layer — all state arrives
over Tauri IPC.

SvelteKit's value sits entirely in features this app does not use. Adopting it
would mean configuring `adapter-static`, disabling SSR, and maintaining a router
for one route: machinery whose only purpose is being switched off. Vite is
already the build tool and Tauri already provides the shell.

## Consequences

- The dev server stays a plain Vite server, so `devUrl` and `frontendDist` in
  `tauri.conf.json` need minimal change.
- No `adapter-static` or `export const ssr = false` boilerplate.
- Frontend agents must not use SvelteKit-only idioms (`$app/*` imports,
  `+page.svelte`, `load` functions). They will not resolve.
- If FastClip grows separate screens — a settings page rather than a modal, an
  import wizard — revisit this. A hand-rolled view switch is fine for two or
  three views. Beyond that, migrating to SvelteKit is about a day's work and
  this ADR should be superseded rather than worked around.
