// Test-only harness around the real `@tauri-apps/api/mocks` module (the
// module WP-02 wired the app against). This is not a substitute IPC layer:
// `mockIPC` replaces `window.__TAURI_INTERNALS__.invoke`, so every call this
// harness observes went through the real `invoke()` in
// `src/lib/ipc/commands.ts` and the real `listen()` in
// `@tauri-apps/api/event`, exactly as the app calls them.
//
// Event delivery uses the same mechanism the real Tauri IPC uses: `listen()`
// registers a callback via `transformCallback`, which `mocks.cjs` (as of
// @tauri-apps/api 2.11.1) stores in an internal `Map` keyed by the id and
// exposes only through `window.__TAURI_INTERNALS__.runCallback(id, data)` —
// there is no `window["_" + id]` global to call directly any more. `emit()`
// below calls `runCallback` with the same `{ event, id, payload }` shape the
// real Tauri core delivers, which is what `mocks.cjs`'s own `runCallback`
// passes straight through to the registered callback.

import { mockIPC, mockWindows, clearMocks } from "@tauri-apps/api/mocks";

// `@tauri-apps/api` does not publish a type for `window.__TAURI_INTERNALS__`
// (see mocks.d.ts, which documents the mock functions but not the global
// they patch). This is the minimal shape this file relies on, confirmed
// against the installed `node_modules/@tauri-apps/api/mocks.cjs`.
interface TauriInternalsForEmit {
  runCallback: (id: number, data: { event: string; id: number; payload: unknown }) => void;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__: TauriInternalsForEmit;
  }
}

export type InvokeCall = { cmd: string; args: Record<string, unknown> | undefined };

export type CommandHandlers = Record<string, (args: Record<string, unknown> | undefined) => unknown>;

export function setupTauriMock(handlers: CommandHandlers): {
  calls: InvokeCall[];
  emit: (eventName: string, payload: unknown) => void;
} {
  const calls: InvokeCall[] = [];
  const eventHandlers = new Map<string, number>();

  mockWindows("main");

  mockIPC((cmd, args) => {
    const a = args as Record<string, unknown> | undefined;
    calls.push({ cmd, args: a });

    if (cmd === "plugin:event|listen") {
      // `a.handler` is the id `transformCallback` registered as
      // `window["_" + id]`; that is what `emit()` below must call, so it is
      // stored verbatim rather than one this harness invents.
      eventHandlers.set(a!.event as string, a!.handler as number);
      return Math.floor(Math.random() * 1_000_000_000); // the listen() call's own eventId, used only for unlisten
    }
    if (cmd === "plugin:event|unlisten") {
      return null;
    }

    if (cmd in handlers) {
      return handlers[cmd](a);
    }

    throw new Error(`mockTauri: unhandled command "${cmd}"`);
  });

  function emit(eventName: string, payload: unknown): void {
    const handlerId = eventHandlers.get(eventName);
    if (handlerId === undefined) {
      throw new Error(`mockTauri: no listener registered for event "${eventName}"`);
    }
    window.__TAURI_INTERNALS__.runCallback(handlerId, { event: eventName, id: 0, payload });
  }

  return { calls, emit };
}

export function teardownTauriMock(): void {
  clearMocks();
}
