import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-07 (test-engineer share), ADR-0010 "The frontend's universal `locked`
// handling becomes load-bearing": the live mid-session lock transition.
//
// "While locked, no command returns a label or a value... assert the locked
// phase renders no clip content, and that an `update_clips` arriving after
// `locked: true` is ignored rather than rendered." And ADR-0010's own list:
// "discard the clip list, close any open create or edit form, clear the
// search query, and ignore any `update_clips` that arrives afterwards."
//
// This suite starts every test from a genuinely "ready" App (real clips
// rendered, not a mocked-out phase), then fires the `lock_state` event the
// same way a manual `lock` from another surface (the tray, per this
// package's dispatch) or a concurrent backend fault would — proving the
// transition is driven by the event, not by a call this component itself made.

const A: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Bank PIN reminder", value: "do not share", colour: "blue" };
const B: Clip = { id: "22222222-2222-2222-2222-222222222222", label: "Bravo Notes", value: "b", colour: "teal" };

const UNLOCKED = { encryption_enabled: true, locked: false, attempts_remaining: null, retry_after_ms: null };
const LOCKED = { encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null };

async function renderReadyApp(clips: Clip[], overrides: Partial<Record<string, (args: unknown) => unknown>> = {}) {
  const mock = setupTauriMock({
    list_clips: () => clips,
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => UNLOCKED,
    ...overrides,
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByText(clips[0].label)).toBeInTheDocument());
  return { ...utils, mock };
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("A lock_state{locked:true} event discards everything the locked phase must not disclose", () => {
  it("the clip list is discarded — no label and no value text remain anywhere in the document", async () => {
    const { mock } = await renderReadyApp([A, B]);
    expect(screen.getByText(A.label)).toBeInTheDocument();

    mock.emit("lock_state", LOCKED);

    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
    expect(screen.queryByText(A.label)).not.toBeInTheDocument();
    expect(screen.queryByText(A.value)).not.toBeInTheDocument();
    expect(screen.queryByText(B.label)).not.toBeInTheDocument();
    expect(document.body.textContent ?? "").not.toContain(A.value);
  });

  it("no clip-list surface (the \"Clips\" region, New clip, Search) is present while locked", async () => {
    const { mock } = await renderReadyApp([A, B]);
    mock.emit("lock_state", LOCKED);

    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
    expect(screen.queryByLabelText("Clips")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New clip" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Search clips" })).not.toBeInTheDocument();
  });

  it("an open edit form is closed, and its clip's label/value do not leak into the locked phase", async () => {
    const { mock } = await renderReadyApp([A, B]);
    await fireEvent.click(screen.getByRole("button", { name: `Edit ${A.label}` }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    mock.emit("lock_state", LOCKED);

    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue(A.label)).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue(A.value)).not.toBeInTheDocument();
  });

  it("an open delete-confirmation dialog is closed, and does not name the clip once locked", async () => {
    const { mock } = await renderReadyApp([A, B]);
    await fireEvent.click(screen.getByRole("button", { name: `Delete ${A.label}` }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();

    mock.emit("lock_state", LOCKED);

    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.queryByText(A.label)).not.toBeInTheDocument();
  });

  it("an active search query is cleared, and the search box is gone", async () => {
    const { mock } = await renderReadyApp([A, B]);
    await fireEvent.click(screen.getByRole("button", { name: "Search clips" }));
    const input = await screen.findByRole("searchbox");
    await fireEvent.input(input, { target: { value: "Bravo" } });
    expect(screen.queryByText(A.label)).not.toBeInTheDocument(); // filtered out

    mock.emit("lock_state", LOCKED);

    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  });
});

describe("An update_clips that arrives after lock_state{locked:true} is ignored, not rendered", () => {
  it("a full clip payload emitted after the lock is discarded — no label appears, the PIN prompt stays up", async () => {
    const { mock } = await renderReadyApp([A]);
    mock.emit("lock_state", LOCKED);
    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());

    // Exactly the shape a real update_clips carries — proves the guard is
    // "discard while locked", not "discard because the payload is empty".
    mock.emit("update_clips", [A, B]);
    // Give any (incorrect) reactive repaint a turn to happen before asserting its absence.
    await new Promise((r) => setTimeout(r, 0));

    expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument();
    expect(screen.queryByText(A.label)).not.toBeInTheDocument();
    expect(screen.queryByText(B.label)).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Clips")).not.toBeInTheDocument();
  });

  it("converse: an update_clips emitted BEFORE the lock is rendered normally — the guard is locked-state-specific, not a general drop of the event", async () => {
    const { mock } = await renderReadyApp([A]);
    mock.emit("update_clips", [A, B]);
    await waitFor(() => expect(screen.getByText(B.label)).toBeInTheDocument());
  });
});

describe("The way back: unlock's lock_state then update_clips restores the ready phase", () => {
  it("lock_state{locked:false} followed by update_clips returns to the clip list with the new payload rendered", async () => {
    const { mock } = await renderReadyApp([A]);
    mock.emit("lock_state", LOCKED);
    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());

    mock.emit("lock_state", UNLOCKED);
    mock.emit("update_clips", [A, B]);

    await waitFor(() => expect(screen.getByText(A.label)).toBeInTheDocument());
    expect(screen.queryByRole("heading", { name: "FastClip is locked" })).not.toBeInTheDocument();
    expect(screen.getByText(B.label)).toBeInTheDocument();
  });
});
