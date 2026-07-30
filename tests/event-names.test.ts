import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import { setLockState } from "../src/lib/state/lockState.svelte";
import type { Clip } from "../src/lib/contract/types";

// Review 012 F2 / ADR-0013, closing the silent-drift path between the two
// halves of `unlock_requested`.
//
// Correction (review 013, WP-08 rework, finding F2): an earlier version of
// this comment claimed `tests/unlock-requested.test.ts` "drives the mock
// with whatever name App.svelte actually asked for, so it cannot notice the
// app asked for the wrong one." That is false and has been checked directly
// against `tests/helpers/mockTauri.ts`: `mock.emit(eventName, …)` looks the
// registered handler up by the literal `eventName` *the test* passes, not by
// whatever name `listen()` was actually called with, and
// `tests/unlock-requested.test.ts`'s own captured-names assertion
// (`expect.arrayContaining(["update_clips", "lock_state", "unlock_requested"])`)
// compares against the same three literals this file does. A misspelling of
// any of the three fails that test too.
//
// The Rust side pins its wire literal in a test —
// `the_event_name_is_the_literal_the_contract_fixes` in
// `src-tauri/tests/tray.rs`, which asserts `UNLOCK_REQUESTED == "unlock_requested"`.
// `App.svelte` passes bare string literals straight to `listen()` for all
// three contract §3 events, with no constant. What this file's sorted
// `toEqual` catches that nothing else in the suite does: an **extra**
// listener — a fourth event registered alongside the correct three, or one
// of the three registered twice under different spellings — which
// `arrayContaining` in `tests/unlock-requested.test.ts` does not notice,
// because a superset still contains the required subset. That is the whole
// of this test's distinct reason to exist; it is not the only place a
// misspelling of the three names would be caught.
//
// This test renders the real App.svelte against a mocked IPC layer, captures
// every `plugin:event|listen` call the real `listen()` from
// `@tauri-apps/api/event` made, and compares the resulting event names
// against the three literal strings contract §3 fixes — typed here from the
// contract, not read out of App.svelte or out of any other test file.

const CLIP: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Greeting", value: "Hello there", colour: "blue" };

const UNLOCKED = { encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null };

// contract §3 "Events": the exact wire spellings, snake_case per contract §0.
const CONTRACT_EVENT_NAMES = ["update_clips", "lock_state", "unlock_requested"];

beforeEach(() => {
  setLockState(UNLOCKED);
});

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
  setLockState(UNLOCKED);
});

describe("contract §3: App.svelte listens for exactly the three fixed event names", () => {
  it("registers update_clips, lock_state and unlock_requested, spelled exactly as the contract fixes them", async () => {
    const { calls } = setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => UNLOCKED,
      list_clips: () => [CLIP],
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);
    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());

    const listenedEvents = calls
      .filter((c) => c.cmd === "plugin:event|listen")
      .map((c) => (c.args as { event: string }).event)
      .sort();

    // Exactly the three names, exactly as spelled — not a superset, not a
    // subset, not a near miss. `toEqual` on the sorted arrays fails on a
    // missing name, an extra name, or a differently-spelled one.
    expect(listenedEvents).toEqual([...CONTRACT_EVENT_NAMES].sort());
  });
});
