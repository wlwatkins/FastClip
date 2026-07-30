import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-14 (test-engineer share): contract §3 "Startup sequence" —
// `get_settings`, then `get_lock_state`, then branch, then `list_clips`.
// `App.svelte` no longer calls `list_clips` directly, and step 4's `storage`
// goes to the failure screen, never an empty list.
//
// This is the definition-of-done item the package exists for: "A store that
// will not open reaches the failure screen, not an empty list. An empty list
// is indistinguishable from a fresh install." That is only proved by the pair
// of tests below — the fault case and its converse, a genuinely empty store —
// asserted together.

const CLIP: Clip = {
  id: "11111111-1111-1111-1111-111111111111",
  label: "Greeting",
  value: "Hello there",
  colour: "blue",
};

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("startup sequence: failure screen vs. empty list", () => {
  it("a store that will not open (get_lock_state relays a recorded fault) reaches the failure screen, and renders no clip list", async () => {
    const listClips = vi.fn(() => [CLIP]); // must never be reached
    setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => {
        throw { kind: "storage" };
      },
      list_clips: listClips,
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.queryByRole("alert")).toBeInTheDocument();
    // No clip list surface at all: neither the "Clips" region nor the clip
    // itself, nor the toolbar that only renders alongside it.
    expect(screen.queryByLabelText("Clips")).not.toBeInTheDocument();
    expect(screen.queryByText(CLIP.label)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New clip" })).not.toBeInTheDocument();
    // step 4 (list_clips) is never reached once step 3 has failed.
    expect(listClips).not.toHaveBeenCalled();
  });

  it("converse: a genuinely empty store (list_clips resolves []) renders an empty list, and shows no failure screen", async () => {
    setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
      list_clips: () => [],
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    await waitFor(() => expect(screen.getByText("No clips yet.")).toBeInTheDocument());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Clips")).toBeInTheDocument();
  });

  it("a store that fails only at step 4 (list_clips rejects storage) also reaches the failure screen, not an empty list", async () => {
    setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
      list_clips: () => {
        throw { kind: "storage" };
      },
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.queryByText("No clips yet.")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Clips")).not.toBeInTheDocument();
  });
});

describe("startup sequence: order of calls", () => {
  it("invokes get_settings, then get_lock_state, then list_clips, in that order, and nothing else in between", async () => {
    const { calls } = setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
      list_clips: () => [CLIP],
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());

    const sequence = calls.filter((c) => ["get_settings", "get_lock_state", "list_clips"].includes(c.cmd)).map((c) => c.cmd);
    expect(sequence).toEqual(["get_settings", "get_lock_state", "list_clips"]);
  });

  it("does not call list_clips at all when get_lock_state has not yet resolved to locked: false (no direct list_clips call)", async () => {
    let releaseLockState: (() => void) | undefined;
    const lockStateGate = new Promise<void>((resolve) => {
      releaseLockState = resolve;
    });
    const listClips = vi.fn(() => [CLIP]);
    setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: async () => {
        await lockStateGate;
        return { encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null };
      },
      list_clips: listClips,
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    // Still on the loading phase: get_lock_state has not resolved yet, so
    // list_clips must not have been reached — proof App.svelte's sequence
    // gates step 4 on step 3, rather than firing list_clips directly and
    // independently of get_lock_state.
    await waitFor(() => expect(screen.getByText("Loading…")).toBeInTheDocument());
    expect(listClips).not.toHaveBeenCalled();

    releaseLockState!();
    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());
    expect(listClips).toHaveBeenCalledTimes(1);
  });
});

describe("startup sequence: the locked branch", () => {
  it("takes the locked branch rather than falling through to a clip list, and never calls list_clips", async () => {
    const listClips = vi.fn(() => [CLIP]);
    setupTauriMock({
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => ({ encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null }),
      list_clips: listClips,
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);

    // Wait for the sequence to leave "loading" — a neutral marker of the
    // sequence having settled, not a claim about the locked branch's own
    // (placeholder, pending WP-07) contents.
    await waitFor(() => expect(screen.queryByText("Loading…")).not.toBeInTheDocument());

    // Assert the branch is taken, not its contents: the else branch's own
    // structural markers — the "New clip" toolbar and the ClipList's clip
    // rows — must be absent, and the failure screen must not have been
    // reached either.
    expect(screen.queryByRole("button", { name: "New clip" })).not.toBeInTheDocument();
    expect(screen.queryByText(CLIP.label)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(listClips).not.toHaveBeenCalled();
  });
});
