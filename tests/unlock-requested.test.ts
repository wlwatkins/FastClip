import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import { setLockState } from "../src/lib/state/lockState.svelte";
import type { Clip } from "../src/lib/contract/types";

// WP-08 rework (review 012 F2, path 1 only): the tray's *Unlock FastClip*
// item raises the window and emits `unlock_requested` (contract §3,
// ADR-0013). App.svelte's `handleUnlockRequested` is what turns that gesture
// into keyboard focus landing on the PIN input.
//
// What this suite proves: the listener is wired in the startup sequence's
// listener effect before its first `invoke` (criterion 1); the discard rule
// on a stale `locked: false` (criterion 2); that a `focusPin()` call actually
// moves `document.activeElement` while locked with no overlay open
// (criterion 3); that this survives a subsequent `window` "focus" event
// which would otherwise let a stale element keep focus (criterion 4); and
// that an open settings panel is left exactly as ADR-0013 says — untouched,
// not dismissed (criterion 5).
//
// What it does NOT prove: dispatching `window.dispatchEvent(new
// Event("focus"))` in jsdom is not the same event WebView2 fires when the
// real OS window regains focus after the tray's raise, and jsdom's `inert`
// does not block `.focus()` on a descendant the way a real browser would
// (already documented in `tests/app-inert.test.ts`). Whether WebView2
// actually restores stale focus at the moment this event lands, and in what
// order relative to the IPC delivery, is a manual check nobody has run
// (contract §3 `unlock_requested`; ADR-0013 "Consequences").

const CLIP: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Greeting", value: "Hello there", colour: "blue" };

const UNLOCKED = { encryption_enabled: true, locked: false, attempts_remaining: null, retry_after_ms: null };
const LOCKED = { encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null };

async function renderLockedApp() {
  const mock = setupTauriMock({
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => LOCKED,
    list_clips: () => [CLIP],
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
  return { ...utils, mock };
}

// `lockState` (src/lib/state/lockState.svelte.ts) is a module-level
// singleton App.svelte's own reactive "locked" effect reads directly — it
// outlives any one render. Reset before every test so a `locked: true` left
// by a previous test cannot make the next App instance jump to the "locked"
// phase before its own `get_lock_state` call has even run (the same reset
// `pin-prompt.test.ts` performs, for the same reason).
beforeEach(() => {
  setLockState(UNLOCKED);
});

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
  setLockState(UNLOCKED);
});

describe("unlock_requested: listener registration", () => {
  it("registers before the startup sequence's first invoke, alongside update_clips and lock_state", async () => {
    const { calls } = setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => UNLOCKED,
      list_clips: () => [CLIP],
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);
    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());

    const listenEvents = calls.filter((c) => c.cmd === "plugin:event|listen").map((c) => (c.args as { event: string }).event);
    const firstInvokeIndex = calls.findIndex((c) => c.cmd === "get_settings" || c.cmd === "get_lock_state");
    const lastListenIndex = calls.map((c) => c.cmd).lastIndexOf("plugin:event|listen");

    expect(listenEvents).toEqual(expect.arrayContaining(["update_clips", "lock_state", "unlock_requested"]));
    expect(lastListenIndex).toBeLessThan(firstInvokeIndex);
  });

  it("is torn down on unmount, the same as the other two", async () => {
    const { calls } = setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => UNLOCKED,
      list_clips: () => [CLIP],
    });
    const { default: App } = await import("../src/App.svelte");
    const { unmount } = render(App);
    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());

    const unlistenCallsBefore = calls.filter((c) => c.cmd === "plugin:event|unlisten").length;
    unmount();
    const unlistenCallsAfter = calls.filter((c) => c.cmd === "plugin:event|unlisten").length;

    // Three listeners were registered (update_clips, lock_state,
    // unlock_requested); unmounting must unlisten all three, not two.
    expect(unlistenCallsAfter - unlistenCallsBefore).toBe(3);
  });
});

describe("unlock_requested: discarded while lockState.locked is false", () => {
  it("moves focus nowhere and changes no state when the last lock_state says unlocked", async () => {
    const mock = setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => UNLOCKED,
      list_clips: () => [CLIP],
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);
    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());

    const before = document.activeElement;
    expect(() => mock.emit("unlock_requested", null)).not.toThrow();
    await new Promise((r) => setTimeout(r, 0));

    // Nothing in the ready phase is a PIN input to focus, and focus has not
    // moved from wherever it was — the discard rule left both alone.
    expect(document.activeElement).toBe(before);
    expect(screen.queryByLabelText("PIN")).not.toBeInTheDocument();
    expect(screen.getByText(CLIP.label)).toBeInTheDocument();
  });
});

describe("unlock_requested: arriving before get_lock_state resolves is discarded, not buffered", () => {
  // Contract §3 `unlock_requested`, "Before any lock state has been received
  // there is none, so the event is discarded... It is **not** buffered for
  // replay", and the amended startup-sequence step 1: an event landing in the
  // window between listener registration (step 1) and `get_lock_state`
  // returning (step 3) must be dropped, not queued and replayed once the
  // lock state arrives.
  //
  // The contract itself names the trap: a store that launches locked mounts
  // the PIN prompt, which focuses at mount regardless of this event, so
  // checking "is the PIN input focused" at the end of the test passes under
  // both discard and buffer-and-replay and proves nothing. This test does
  // not make that assertion.
  //
  // The distinguishing signal instead is PinPrompt.svelte's own
  // `awaitingFocusReassert` flag: `focusPin()` (called from
  // `handleUnlockRequested` only when the event is *not* discarded) is the
  // only thing that arms it, and only while it is armed does a later window
  // "focus" event pull focus back to the PIN input (see "focus survives a
  // subsequent window focus event" above, which exercises the live case).
  // If discard held, `focusPin()` was never called for the event fired
  // below, the flag was never armed, and a later window "focus" event has
  // nothing to reassert. If a buffer-and-replay implementation instead
  // called `focusPin()` once the lock state arrived, the flag would be
  // armed and the same window "focus" event would pull focus back — which is
  // the failure this test exists to catch.
  it("does not arm the post-raise focus reassert, so a later window focus event cannot pull focus back to the PIN input", async () => {
    let releaseLockState: (() => void) | undefined;
    const lockStateGate = new Promise<void>((resolve) => {
      releaseLockState = resolve;
    });
    const { calls, emit } = setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: async () => {
        await lockStateGate;
        return LOCKED;
      },
      list_clips: () => [CLIP],
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    // Step 1 (listener registration) completes well before step 3
    // (get_lock_state) resolves, since `lockStateGate` is still pending —
    // this is the window the contract names.
    await waitFor(() =>
      expect(
        calls.some((c) => c.cmd === "plugin:event|listen" && (c.args as { event: string }).event === "unlock_requested"),
      ).toBe(true),
    );
    // Confirms the window is still open: no lock_state has been received,
    // so the app has not left the loading phase yet.
    expect(screen.getByText("Loading…")).toBeInTheDocument();

    expect(() => emit("unlock_requested", null)).not.toThrow();

    releaseLockState!();
    const pinInput = await waitFor(() => screen.getByLabelText("PIN") as HTMLInputElement);

    // Mount-time autofocus already landed on `pinInput` here, identically
    // under either implementation — this is the coincidence the contract
    // says must not be relied on, so it is deliberately not asserted.
    const decoy = document.createElement("button");
    document.body.appendChild(decoy);
    decoy.focus();
    expect(document.activeElement).toBe(decoy);

    window.dispatchEvent(new Event("focus"));

    // Discard: nothing armed the reassert, so the window focus event above
    // found nothing to do and focus stayed on the decoy.
    expect(document.activeElement).toBe(decoy);
    expect(document.activeElement).not.toBe(pinInput);
    decoy.remove();
  });
});

describe("unlock_requested: focuses the PIN input while locked with no overlay open", () => {
  it("document.activeElement is the PIN input after the event, having been elsewhere before it", async () => {
    const { mock } = await renderLockedApp();
    const pinInput = screen.getByLabelText("PIN") as HTMLInputElement;

    // Move focus away from the field the mount-time `autofocus` effect
    // already gave it, so the assertion below proves the *event* moved
    // focus rather than observing autofocus's own effect.
    const decoy = document.createElement("button");
    document.body.appendChild(decoy);
    decoy.focus();
    expect(document.activeElement).toBe(decoy);

    mock.emit("unlock_requested", null);

    await waitFor(() => expect(document.activeElement).toBe(pinInput));
    decoy.remove();
  });
});

describe("unlock_requested: focus survives a subsequent window focus event", () => {
  it("stays on the PIN input even if the window's own focus event would otherwise let a stale element keep it", async () => {
    const { mock } = await renderLockedApp();
    const pinInput = screen.getByLabelText("PIN") as HTMLInputElement;

    const decoy = document.createElement("button");
    document.body.appendChild(decoy);
    decoy.focus();

    mock.emit("unlock_requested", null);
    await waitFor(() => expect(document.activeElement).toBe(pinInput));

    // Simulate WebView2 restoring the pre-minimise element and then the
    // window itself regaining native focus — the race ADR-0013 and review
    // 012 F2 path 1 describe. Without the reassert mechanism this leaves
    // the decoy focused.
    decoy.focus();
    expect(document.activeElement).toBe(decoy);
    window.dispatchEvent(new Event("focus"));

    expect(document.activeElement).toBe(pinInput);
    decoy.remove();
  });
});

describe("unlock_requested: the settings-panel case is left untouched, not dismissed", () => {
  it("an open settings panel stays open — the event neither closes it nor errors", async () => {
    const { mock } = await renderLockedApp();

    // "Settings while locked" (copy.md) — the settings button renders even
    // while locked.
    await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();

    expect(() => mock.emit("unlock_requested", null)).not.toThrow();
    await new Promise((r) => setTimeout(r, 0));

    // Untouched: the panel is still open. ADR-0013 leaves this as an open
    // owner decision, not something this change may resolve by guessing.
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
  });
});
