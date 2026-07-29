import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-05 (test-engineer share): "Delete confirms first." (spec §4.2, contract:
// "Confirmation is a frontend concern. The backend deletes when asked.")

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

describe("delete: confirmation gate", () => {
  it("clicking the row's delete action does not call delete_clip until the dialog is confirmed", async () => {
    const deleteClip = vi.fn(() => null);
    await renderApp([CLIP], { delete_clip: deleteClip });

    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    expect(deleteClip).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "Delete", exact: true }));

    await waitFor(() => expect(deleteClip).toHaveBeenCalledTimes(1));
    expect(deleteClip).toHaveBeenCalledWith({ clip_id: CLIP.id });
  });

  it("cancelling the confirmation issues no delete_clip invoke and closes the dialog", async () => {
    const deleteClip = vi.fn(() => null);
    const { mock } = await renderApp([CLIP], { delete_clip: deleteClip });

    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    const callsBeforeCancel = mock.calls.length;
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(deleteClip).not.toHaveBeenCalled();
    expect(mock.calls.length).toBe(callsBeforeCancel);
    // The clip is still there; nothing was applied.
    expect(screen.getByText(CLIP.label)).toBeInTheDocument();
  });

  it("Escape on the confirmation dialog cancels without deleting", async () => {
    const deleteClip = vi.fn(() => null);
    await renderApp([CLIP], { delete_clip: deleteClip });

    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    const dialog = screen.getByRole("alertdialog");
    await fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(deleteClip).not.toHaveBeenCalled();
  });

  it("a failed delete (not_found) shows an error and resyncs, without crashing the dialog state", async () => {
    let listCallCount = 0;
    await renderApp([CLIP], {
      list_clips: () => {
        listCallCount += 1;
        return listCallCount === 1 ? [CLIP] : [];
      },
      delete_clip: () => {
        throw { kind: "not_found", clip_id: CLIP.id };
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    await fireEvent.click(screen.getByRole("button", { name: "Delete", exact: true }));

    await waitFor(() => expect(screen.getByText("That clip no longer exists.")).toBeInTheDocument());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });
});

describe("keyboard reachability — clip row, its actions, and the dialogs", () => {
  it("the clip row (copy target), edit action and delete action are all real buttons with accessible names, none removed from the tab order", async () => {
    await renderApp([CLIP]);
    const copyBtn = screen.getByRole("button", { name: CLIP.label });
    const editBtn = screen.getByRole("button", { name: `Edit ${CLIP.label}` });
    const deleteBtn = screen.getByRole("button", { name: `Delete ${CLIP.label}` });

    for (const btn of [copyBtn, editBtn, deleteBtn]) {
      expect(btn.tagName).toBe("BUTTON");
      expect(btn).not.toHaveAttribute("tabindex", "-1");
      btn.focus();
      expect(document.activeElement).toBe(btn);
    }
  });

  it("row actions stay revealed by group-hover/group-focus-within opacity, not display:none — so they remain focusable while visually hidden", async () => {
    await renderApp([CLIP]);
    const editBtn = screen.getByRole("button", { name: `Edit ${CLIP.label}` });
    const actionsWrapper = editBtn.parentElement!;
    const classes = actionsWrapper.getAttribute("class") ?? "";
    expect(classes).toMatch(/opacity-0/);
    expect(classes).toMatch(/group-hover:opacity-100/);
    expect(classes).toMatch(/group-focus-within:opacity-100/);
    expect(classes).not.toMatch(/\bhidden\b/);
    // The mechanism this proves matters: focusing the (visually hidden) button
    // still works, because opacity does not remove an element from the tab
    // order the way display:none or visibility:hidden would.
    editBtn.focus();
    expect(document.activeElement).toBe(editBtn);
  });

  it("both dialogs (create/edit form, delete confirmation) carry a focus-visible style distinct from hover on every control", async () => {
    await renderApp([CLIP]);
    await fireEvent.click(screen.getByRole("button", { name: `Edit ${CLIP.label}` }));
    for (const btn of [screen.getByRole("button", { name: "Cancel" }), screen.getByRole("button", { name: "Save" })]) {
      const classes = btn.getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
    }
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    for (const btn of [screen.getByRole("button", { name: "Cancel" }), screen.getByRole("button", { name: "Delete", exact: true })]) {
      const classes = btn.getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
    }
  });

  it("the delete confirmation dialog moves initial focus to Cancel, the non-destructive control", async () => {
    await renderApp([CLIP]);
    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" })));
  });
});
