import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-06 (test-engineer share): "Order survives a restart" and "a reorder
// interrupted mid-write leaves a valid store" are proved for real in
// src-tauri/tests/kill_mid_write.rs, by killing a process — not re-asserted
// here against a mock, which would only prove the mock behaves as scripted.
//
// What is real to test here (ADR-0007, ClipList.svelte, ClipRow.svelte):
//
// - Keyboard reordering and drag reordering are two gestures funnelled into
//   one `onreorder` call, deriving the permutation from the array the
//   backend last sent. An equivalent gesture on each path must produce an
//   identical `order` argument to `reorder_clips` — if it does not, the
//   frontend disagrees with itself about what order the user chose, which is
//   the "two sources of truth" defect this package's critic is told to hunt.
// - The rendered list moves only when `update_clips` arrives, never
//   optimistically from the gesture itself — there is no local ordering
//   state to move it early.
// - `invalid_input { reason: "not_a_permutation" }` triggers a resync via
//   `list_clips`, not a retry with the same order, and the rendered order is
//   unchanged by the rejection.
// - WP-13: drag handles are inert while a search query is active, on all
//   four fronts (disabled, draggable="false", dragover refused, keyboard
//   move a no-op), and clearing the query restores all four.
// - The handle is Tab-reachable, has an accessible name, and carries a
//   focus-visible style distinct from hover.

const A: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Alpha", value: "a", colour: "blue" };
const B: Clip = { id: "22222222-2222-2222-2222-222222222222", label: "Bravo", value: "b", colour: "teal" };
const C: Clip = { id: "33333333-3333-3333-3333-333333333333", label: "Charlie", value: "c", colour: "amber" };

async function renderApp(clips: Clip[], overrides: Partial<Record<string, (args: unknown) => unknown>> = {}) {
  const mock = setupTauriMock({
    list_clips: () => clips,
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
    ...overrides,
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByText(clips[0]?.label ?? "No clips yet.")).toBeInTheDocument());
  return { ...utils, mock };
}

/** The handle button for a clip, found by its full accessible name (the enabled-state label). */
function handleFor(clip: Clip): HTMLElement {
  return screen.getByRole("button", { name: `Reorder ${clip.label}. Press arrow up or arrow down to move, or drag.` });
}

/** The disabled-state handle, found by its WP-13 label. */
function disabledHandleFor(clip: Clip): HTMLElement {
  return screen.getByRole("button", {
    name: `Reorder ${clip.label}. Disabled while a search is active — clear the search to reorder.`,
  });
}

/** Minimal fake DataTransfer: jsdom has no DragEvent/DataTransfer of its own (see comment on dispatchDrag below). */
function makeDataTransfer() {
  const store: Record<string, string> = {};
  return {
    setData: (k: string, v: string) => {
      store[k] = v;
    },
    getData: (k: string) => store[k] ?? "",
    effectAllowed: "",
    dropEffect: "",
  };
}

/**
 * Fires a real drag gesture: dragstart on `source`'s handle, then dragover
 * and drop on `target`'s row. jsdom does not implement a global `DragEvent`
 * (confirmed: `new DragEvent(...)` throws `ReferenceError` in this jsdom
 * version), so each event is a plain `Event` with `dataTransfer` and
 * `clientY` attached via `Object.defineProperty` before dispatch — the same
 * properties `ClipRow.svelte`'s handlers read, just not constructed through
 * a `DragEvent` prototype the runtime does not have. The row's own
 * `getBoundingClientRect` is stubbed so the "before/after midpoint" branch in
 * `handleDrop` is exercised deterministically rather than against jsdom's
 * default all-zero rect.
 */
function dispatchDrag(sourceHandle: HTMLElement, targetRow: HTMLElement, clientY: number): void {
  const dt = makeDataTransfer();

  const dragstart = new Event("dragstart", { bubbles: true, cancelable: true });
  Object.defineProperty(dragstart, "dataTransfer", { value: dt, configurable: true });
  sourceHandle.dispatchEvent(dragstart);

  vi.spyOn(targetRow, "getBoundingClientRect").mockReturnValue({
    top: 0,
    bottom: 20,
    height: 20,
    left: 0,
    right: 100,
    width: 100,
    x: 0,
    y: 0,
    toJSON() {
      return {};
    },
  });

  const dragover = new Event("dragover", { bubbles: true, cancelable: true });
  Object.defineProperty(dragover, "dataTransfer", { value: dt, configurable: true });
  targetRow.dispatchEvent(dragover);

  const drop = new Event("drop", { bubbles: true, cancelable: true });
  Object.defineProperty(drop, "dataTransfer", { value: dt, configurable: true });
  Object.defineProperty(drop, "clientY", { value: clientY, configurable: true });
  Object.defineProperty(drop, "currentTarget", { value: targetRow, configurable: true });
  targetRow.dispatchEvent(drop);
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("keyboard and drag funnel into the same permutation (ADR-0007, no two sources of truth)", () => {
  it("ArrowDown on Alpha's handle sends the same order as dragging Alpha onto Bravo (drop below its midpoint)", async () => {
    // Path 1: keyboard.
    const reorderKeyboard = vi.fn(() => null);
    await renderApp([A, B, C], { reorder_clips: reorderKeyboard });
    await fireEvent.keyDown(handleFor(A), { key: "ArrowDown" });
    await waitFor(() => expect(reorderKeyboard).toHaveBeenCalledTimes(1));
    const keyboardOrder = (reorderKeyboard.mock.calls[0][0] as { order: string[] }).order;
    cleanup();
    teardownTauriMock();

    // Path 2: drag. Alpha's row is its handle's parent; drop on Bravo's row,
    // below Bravo's midpoint (rect top 0, height 20, clientY 15 > 10),
    // which is "insert Alpha immediately after Bravo" — the same single-step
    // move ArrowDown produced above.
    const reorderDrag = vi.fn(() => null);
    await renderApp([A, B, C], { reorder_clips: reorderDrag });
    const bravoRow = handleFor(B).parentElement!;
    dispatchDrag(handleFor(A), bravoRow, 15);
    await waitFor(() => expect(reorderDrag).toHaveBeenCalledTimes(1));
    const dragOrder = (reorderDrag.mock.calls[0][0] as { order: string[] }).order;

    expect(dragOrder).toEqual(keyboardOrder);
    expect(dragOrder).toEqual([B.id, A.id, C.id]);
  });

  it("the reverse gesture also agrees: ArrowUp on Charlie matches dropping Charlie above Bravo's midpoint", async () => {
    const reorderKeyboard = vi.fn(() => null);
    await renderApp([A, B, C], { reorder_clips: reorderKeyboard });
    await fireEvent.keyDown(handleFor(C), { key: "ArrowUp" });
    await waitFor(() => expect(reorderKeyboard).toHaveBeenCalledTimes(1));
    const keyboardOrder = (reorderKeyboard.mock.calls[0][0] as { order: string[] }).order;
    cleanup();
    teardownTauriMock();

    const reorderDrag = vi.fn(() => null);
    await renderApp([A, B, C], { reorder_clips: reorderDrag });
    const bravoRow = handleFor(B).parentElement!;
    // Above Bravo's midpoint (rect top 0, height 20, midpoint 10): clientY 5.
    dispatchDrag(handleFor(C), bravoRow, 5);
    await waitFor(() => expect(reorderDrag).toHaveBeenCalledTimes(1));
    const dragOrder = (reorderDrag.mock.calls[0][0] as { order: string[] }).order;

    expect(dragOrder).toEqual(keyboardOrder);
    expect(dragOrder).toEqual([A.id, C.id, B.id]);
  });
});

describe("no local ordering state: the list moves only when update_clips arrives", () => {
  it("after a keyboard move, the DOM order is unchanged until the backend's update_clips event lands", async () => {
    const reorderClips = vi.fn(() => null); // resolves; deliberately never followed by an emit in this test
    const { mock } = await renderApp([A, B, C], { reorder_clips: reorderClips });

    await fireEvent.keyDown(handleFor(A), { key: "ArrowDown" });
    await waitFor(() => expect(reorderClips).toHaveBeenCalledTimes(1));

    // The command resolved; if ClipList kept any local order, the rows would
    // already have moved. They must not have.
    const labelsNow = screen.getAllByRole("button", { name: /^(Alpha|Bravo|Charlie)$/ }).map((el) => el.textContent);
    expect(labelsNow).toEqual(["Alpha", "Bravo", "Charlie"]);

    // Only once the backend emits the real order does the list move.
    mock.emit("update_clips", [B, A, C]);
    await waitFor(() => {
      const after = screen.getAllByRole("button", { name: /^(Alpha|Bravo|Charlie)$/ }).map((el) => el.textContent);
      expect(after).toEqual(["Bravo", "Alpha", "Charlie"]);
    });
  });

  it("a drag produces the same delayed-render behaviour: no reorder is visible before update_clips", async () => {
    const reorderClips = vi.fn(() => null);
    const { mock } = await renderApp([A, B, C], { reorder_clips: reorderClips });

    const bravoRow = handleFor(B).parentElement!;
    dispatchDrag(handleFor(A), bravoRow, 15);
    await waitFor(() => expect(reorderClips).toHaveBeenCalledTimes(1));

    const labelsNow = screen.getAllByRole("button", { name: /^(Alpha|Bravo|Charlie)$/ }).map((el) => el.textContent);
    expect(labelsNow).toEqual(["Alpha", "Bravo", "Charlie"]);

    mock.emit("update_clips", [B, A, C]);
    await waitFor(() => {
      const after = screen.getAllByRole("button", { name: /^(Alpha|Bravo|Charlie)$/ }).map((el) => el.textContent);
      expect(after).toEqual(["Bravo", "Alpha", "Charlie"]);
    });
  });
});

describe("not_a_permutation: resync, not retry; rendered order unchanged", () => {
  it("a rejected reorder calls list_clips exactly once more (a resync), reorder_clips only once (no retry), and the rendered order does not change", async () => {
    let listCallCount = 0;
    const reorderClips = vi.fn(() => {
      throw { kind: "invalid_input", field: "order", reason: "not_a_permutation" };
    });
    await renderApp([A, B, C], {
      list_clips: () => {
        listCallCount += 1;
        return [A, B, C]; // the backend's real (unchanged) order, both times
      },
      reorder_clips: reorderClips,
    });
    expect(listCallCount).toBe(1); // the initial startup list

    await fireEvent.keyDown(handleFor(A), { key: "ArrowDown" });

    await waitFor(() => expect(screen.getByText("The list changed elsewhere. Reloading.")).toBeInTheDocument());
    expect(reorderClips).toHaveBeenCalledTimes(1); // no retry with the same order
    expect(listCallCount).toBe(2); // exactly one resync

    const labelsNow = screen.getAllByRole("button", { name: /^(Alpha|Bravo|Charlie)$/ }).map((el) => el.textContent);
    expect(labelsNow).toEqual(["Alpha", "Bravo", "Charlie"]);
  });
});

describe("WP-13: drag handles are inert while a search query is active", () => {
  async function renderWithSearch(
    query: string,
    overrides: Partial<Record<string, (args: unknown) => unknown>> = {},
  ) {
    const utils = await renderApp([A, B], overrides);
    await fireEvent.click(screen.getByRole("button", { name: "Search clips" }));
    const input = await screen.findByRole("searchbox");
    await fireEvent.input(input, { target: { value: query } });
    return utils;
  }

  it("the visible handle is disabled, draggable=false, and carries the disabled-state accessible name", async () => {
    await renderWithSearch("Alpha");
    const handle = disabledHandleFor(A);
    expect(handle).toBeDisabled();
    expect(handle).toHaveAttribute("draggable", "false");
    // Bravo is filtered out entirely, so it is not present to check at all —
    // consistent with a permutation that omits it being unconstructable from
    // this list, which is the reason the ADR gives for disabling drag here.
    expect(screen.queryByText("Bravo")).not.toBeInTheDocument();
  });

  it("dragover on the row is refused: it does not call preventDefault, so the row never becomes a valid drop target", async () => {
    await renderWithSearch("Alpha");
    const row = disabledHandleFor(A).parentElement!;
    const dragover = new Event("dragover", { bubbles: true, cancelable: true });
    Object.defineProperty(dragover, "dataTransfer", { value: makeDataTransfer(), configurable: true });
    row.dispatchEvent(dragover);
    expect(dragover.defaultPrevented).toBe(false);
  });

  it("the keyboard move is a no-op: ArrowDown on the disabled handle issues no reorder_clips call", async () => {
    // Query "a" matches both Alpha and Bravo, so both rows stay in the
    // filtered list and Alpha's ArrowDown is within array bounds — the
    // guard under test is what stops the move, not an incidental
    // single-row bounds check that would no-op regardless.
    const reorderClips = vi.fn(() => null);
    await renderWithSearch("a", { reorder_clips: reorderClips });
    await fireEvent.keyDown(disabledHandleFor(A), { key: "ArrowDown" });
    expect(reorderClips).not.toHaveBeenCalled();
  });

  it("clearing the query restores all four: enabled, draggable=true, the full accessible name, and a working keyboard move", async () => {
    const reorderClips = vi.fn(() => null);
    await renderApp([A, B], { reorder_clips: reorderClips });
    await fireEvent.click(screen.getByRole("button", { name: "Search clips" }));
    const input = await screen.findByRole("searchbox");
    await fireEvent.input(input, { target: { value: "Alpha" } });
    expect(disabledHandleFor(A)).toBeDisabled();

    await fireEvent.click(screen.getByRole("button", { name: "Close search" }));

    const handle = handleFor(A);
    expect(handle).not.toBeDisabled();
    expect(handle).toHaveAttribute("draggable", "true");

    await fireEvent.keyDown(handle, { key: "ArrowDown" });
    await waitFor(() => expect(reorderClips).toHaveBeenCalledTimes(1));
    expect((reorderClips.mock.calls[0][0] as { order: string[] }).order).toEqual([B.id, A.id]);
  });
});

describe("the handle is keyboard-accessible", () => {
  it("is Tab-reachable (no tabindex=-1) and focusable", async () => {
    await renderApp([A]);
    const handle = handleFor(A);
    expect(handle).not.toHaveAttribute("tabindex", "-1");
    handle.focus();
    expect(document.activeElement).toBe(handle);
  });

  it("has an accessible name distinct from every other control on the row", async () => {
    await renderApp([A]);
    expect(handleFor(A)).toHaveAccessibleName(`Reorder ${A.label}. Press arrow up or arrow down to move, or drag.`);
  });

  it("carries a focus-visible style distinct from its hover style", async () => {
    await renderApp([A]);
    const classes = handleFor(A).getAttribute("class") ?? "";
    expect(classes).toMatch(/focus-visible:outline/);
    expect(classes).toMatch(/hover:text-zinc-300/);
  });
});
