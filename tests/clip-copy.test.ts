import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-05 (docs/src/work/wp-05-crud.md, test-engineer share): "Clicking a clip
// issues the copy command with the right argument, and an `update_clips`
// event re-renders." backend-dev's src-tauri/tests/ipc.rs already proves,
// through a real invoke_handler, that copy_clip writes the clipboard before
// incrementing use_count exactly once and that a locked/unknown-id copy
// touches nothing — that is not repeated here against a mock, which would
// prove less. What this file covers is the frontend's own contribution: it
// calls the command with the contract's exact wire shape, and it reacts to
// the events the backend promises to send.

const CLIP: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Greeting", value: "Hello there", colour: "blue" };
const CLIP_2: Clip = { id: "22222222-2222-2222-2222-222222222222", label: "Sign-off", value: "Best, Team", colour: "amber" };

async function renderApp(clips: Clip[], overrides: Partial<Record<string, (args: unknown) => unknown>> = {}) {
  const mock = setupTauriMock({
    list_clips: () => clips,
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
    ...overrides,
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  // Startup sequence (contract §3) is async: get_settings -> get_lock_state -> list_clips.
  await waitFor(() => expect(screen.getByText(clips[0]?.label ?? "No clips yet.")).toBeInTheDocument());
  return { ...utils, mock };
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("copy: the command wire shape", () => {
  it("clicking a clip row invokes copy_clip with { clip_id: <that clip's id> } — the exact key the contract names", async () => {
    const copyClip = vi.fn(() => null);
    const { mock } = await renderApp([CLIP], { copy_clip: copyClip });

    const row = screen.getByRole("button", { name: CLIP.label });
    await fireEvent.click(row);

    await waitFor(() => expect(copyClip).toHaveBeenCalledTimes(1));
    // The assertion that catches drift: `clipId` or a wrapped `{ args: {...} }`
    // would pass a looser check but fail this one (contract §0 argument shape).
    expect(copyClip).toHaveBeenCalledWith({ clip_id: CLIP.id });

    const copyCalls = mock.calls.filter((c) => c.cmd === "copy_clip");
    expect(copyCalls).toHaveLength(1);
    expect(Object.keys(copyCalls[0].args ?? {})).toEqual(["clip_id"]);
  });

  it("shows a non-blocking confirmation naming the clip after a successful copy (spec §4.1)", async () => {
    await renderApp([CLIP], { copy_clip: () => null });
    const row = screen.getByRole("button", { name: CLIP.label });
    await fireEvent.click(row);

    const status = await screen.findByRole("status");
    await waitFor(() => expect(status).toHaveTextContent(`Copied "${CLIP.label}"`));
  });

  it("a copy that rejects not_found resyncs the list via list_clips rather than silently doing nothing", async () => {
    let listCallCount = 0;
    const { mock } = await renderApp([CLIP], {
      list_clips: () => {
        listCallCount += 1;
        // Second call (the resync) returns the clip already gone.
        return listCallCount === 1 ? [CLIP] : [];
      },
      copy_clip: () => {
        throw { kind: "not_found", clip_id: CLIP.id };
      },
    });

    const row = screen.getByRole("button", { name: CLIP.label });
    await fireEvent.click(row);

    await waitFor(() => expect(screen.getByText("No clips yet.")).toBeInTheDocument());
    const listCalls = mock.calls.filter((c) => c.cmd === "list_clips");
    expect(listCalls.length).toBeGreaterThanOrEqual(2);
  });
});

describe("update_clips: event re-render", () => {
  it("an update_clips event replaces the rendered list with its full payload", async () => {
    const { mock } = await renderApp([CLIP]);
    expect(screen.getByText(CLIP.label)).toBeInTheDocument();
    expect(screen.queryByText(CLIP_2.label)).not.toBeInTheDocument();

    mock.emit("update_clips", [CLIP_2]);

    await waitFor(() => expect(screen.getByText(CLIP_2.label)).toBeInTheDocument());
    expect(screen.queryByText(CLIP.label)).not.toBeInTheDocument();
  });

  it("an update_clips event carrying a malformed entry throws at the boundary rather than rendering it (contract §0 validation)", async () => {
    const { mock } = await renderApp([CLIP]);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    // `label` is missing — a payload that would once have passed an `as Clip[]`
    // cast unnoticed. parseClipList must throw, and since App.svelte's
    // listener does not catch, the error surfaces as an unhandled exception
    // rather than a silently rendered malformed row.
    expect(() => mock.emit("update_clips", [{ id: "x", value: "v", colour: "blue" }])).toThrow(
      /Malformed IPC payload/,
    );

    // The list is not corrupted by the attempt: the last valid state stands.
    expect(screen.getByText(CLIP.label)).toBeInTheDocument();
    errorSpy.mockRestore();
  });
});
