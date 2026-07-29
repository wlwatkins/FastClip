import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-05 (test-engineer share): "Create, edit and delete flows with inline
// validation per spec §3, including validation failure and cancellation."
// backend-dev's ipc.rs already proves the backend rejects a client-supplied
// id / use_count / icon / visible / clear_time by name; that is not repeated
// here.

const CLIP: Clip = {
  id: "11111111-1111-1111-1111-111111111111",
  label: "Greeting",
  value: "Hello there",
  colour: "blue",
};

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

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("create flow", () => {
  it("submitting a valid form calls create_clip with exactly { clip: { label, value, colour } } — no id, no use_count", async () => {
    const createClip = vi.fn(() => null);
    await renderApp([], { create_clip: createClip });

    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    const dialog = await screen.findByRole("dialog");
    await fireEvent.input(screen.getByLabelText("Label"), { target: { value: "New label" } });
    await fireEvent.input(screen.getByLabelText("Value"), { target: { value: "New value" } });
    await fireEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => expect(createClip).toHaveBeenCalledTimes(1));
    const call = createClip.mock.calls[0][0] as { clip: Record<string, unknown> };
    expect(Object.keys(call)).toEqual(["clip"]);
    expect(Object.keys(call.clip).sort()).toEqual(["colour", "label", "value"]);
    expect(call.clip).toMatchObject({ label: "New label", value: "New value" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(dialog).not.toBeInTheDocument();
  });

  it("does not call create_clip when the label is empty, and shows the inline error instead", async () => {
    const createClip = vi.fn(() => null);
    await renderApp([], { create_clip: createClip });

    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    await fireEvent.input(screen.getByLabelText("Value"), { target: { value: "value only" } });
    await fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(createClip).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("This field is required.")).toBeInTheDocument();
  });

  it("a backend invalid_input rejection surfaces the same field-level message and does not close the form", async () => {
    await renderApp([], {
      create_clip: () => {
        throw { kind: "invalid_input", field: "label", reason: "too_long" };
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    await fireEvent.input(screen.getByLabelText("Label"), { target: { value: "x" } });
    await fireEvent.input(screen.getByLabelText("Value"), { target: { value: "value" } });
    await fireEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => expect(screen.getByText("This is too long.")).toBeInTheDocument());
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("Cancel closes the form and issues no invoke at all", async () => {
    const { mock } = await renderApp([]);
    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    const callsBeforeCancel = mock.calls.length;
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mock.calls.length).toBe(callsBeforeCancel);
  });

  it("Escape cancels the form the same way as the Cancel button", async () => {
    await renderApp([]);
    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    const dialog = screen.getByRole("dialog");

    await fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});

describe("edit flow", () => {
  it("opening edit seeds the form with the clip's current values, and submitting calls update_clip with the unchanged id", async () => {
    const updateClip = vi.fn(() => null);
    await renderApp([CLIP], { update_clip: updateClip });

    await fireEvent.click(screen.getByRole("button", { name: `Edit ${CLIP.label}` }));
    const labelInput = await screen.findByLabelText<HTMLInputElement>("Label");
    expect(labelInput.value).toBe(CLIP.label);

    await fireEvent.input(labelInput, { target: { value: "Renamed" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(updateClip).toHaveBeenCalledTimes(1));
    const call = updateClip.mock.calls[0][0] as { clip: Clip };
    expect(call.clip.id).toBe(CLIP.id);
    expect(call.clip.label).toBe("Renamed");
  });

  it("does not call update_clip when the value is cleared to whitespace only", async () => {
    const updateClip = vi.fn(() => null);
    await renderApp([CLIP], { update_clip: updateClip });

    await fireEvent.click(screen.getByRole("button", { name: `Edit ${CLIP.label}` }));
    const valueInput = await screen.findByLabelText("Value");
    await fireEvent.input(valueInput, { target: { value: "   " } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(updateClip).not.toHaveBeenCalled();
    expect(screen.getAllByText("This field is required.").length).toBeGreaterThan(0);
  });
});
