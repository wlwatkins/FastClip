import { describe, expect, it, afterEach, beforeEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// G2 rework validation, WP-11 F2 (docs/src/product/copy.md "Failures"
// `locked` row): "no separate sentence — the prompt itself is the answer."
// `describeError` returns `null` for `locked`; every call site must treat
// `null` as "show nothing" rather than falling back to a rendered empty
// toast. `tsc` catches a caller that ignores the `null` return entirely
// (the type is `string | null`), but it does NOT catch a caller that
// handles `null` by rendering an empty toast — that call site still
// typechecks. This file drives a real `locked` rejection through the real
// call sites (App.svelte and SettingsPanel.svelte, via a real App with
// settings open) and asserts no toast appears at all — not that the toast
// text is empty.
//
// The reachable scenario named in the review: a manual lock (ADR-0010)
// lands (a `lock_state{locked:true}` event) at the same moment a
// clip-touching command is in flight and comes back `locked`. Each mocked
// command handler below emits `lock_state` before throwing, reproducing
// that ordering rather than assuming it.

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
  open: vi.fn(),
}));

import { open as dialogOpen } from "@tauri-apps/plugin-dialog";

const CLIP: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Greeting", value: "Hello there", colour: "blue" };
const CLIP_2: Clip = { id: "22222222-2222-2222-2222-222222222222", label: "Sign-off", value: "Best, Team", colour: "amber" };

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

/**
 * The always-present-but-usually-empty live region Toast.svelte renders
 * into, found by its own class rather than `role="status"` — PinPrompt's
 * attempts-remaining line is `role="status"` too once locked, and this test
 * must not pass by reading that region's (non-empty, expected) text instead
 * of the toast's.
 */
function toastText(): string {
  const el = document.querySelector(".pointer-events-none.fixed.inset-x-0.bottom-3");
  if (el === null) throw new Error("Toast live region not found");
  return el.textContent ?? "";
}

async function assertLockedAndSilent() {
  await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
  // Give any (incorrect) toast a turn to render before asserting its absence
  // — the same pattern tests/app-lock-transition.test.ts uses for the
  // converse claim (nothing repaints after the guard).
  await new Promise((r) => setTimeout(r, 0));
  expect(toastText()).toBe("");
}

beforeEach(() => {
  vi.mocked(dialogOpen).mockReset();
});

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("App.svelte: a locked rejection shows no toast, at each of the three call sites", () => {
  it("handleCopy: copy_clip rejects locked (a manual lock landing mid-copy, ADR-0010) — no toast, only the PIN prompt", async () => {
    let mock: Awaited<ReturnType<typeof renderReadyApp>>["mock"];
    ({ mock } = await renderReadyApp([CLIP, CLIP_2], {
      copy_clip: () => {
        // The manual lock lands first (this is what makes the store return
        // `locked`), then the command itself fails on it.
        mock.emit("lock_state", LOCKED);
        throw { kind: "locked" };
      },
    }));
    const row = screen.getByRole("button", { name: CLIP.label });
    await fireEvent.click(row);

    await assertLockedAndSilent();
  });

  it("confirmDelete: delete_clip rejects locked — no toast, only the PIN prompt", async () => {
    let mock: Awaited<ReturnType<typeof renderReadyApp>>["mock"];
    ({ mock } = await renderReadyApp([CLIP], {
      delete_clip: () => {
        mock.emit("lock_state", LOCKED);
        throw { kind: "locked" };
      },
    }));
    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    await fireEvent.click(screen.getByRole("button", { name: "Delete", exact: true }));

    await assertLockedAndSilent();
  });

  it("handleReorder: reorder_clips rejects locked (keyboard-driven reorder) — no toast, only the PIN prompt", async () => {
    let mock: Awaited<ReturnType<typeof renderReadyApp>>["mock"];
    ({ mock } = await renderReadyApp([CLIP, CLIP_2], {
      reorder_clips: () => {
        mock.emit("lock_state", LOCKED);
        throw { kind: "locked" };
      },
    }));
    const handle = screen.getByRole("button", { name: `Reorder ${CLIP.label}. Press arrow up or arrow down to move, or drag.` });
    await fireEvent.keyDown(handle, { key: "ArrowDown" });

    await assertLockedAndSilent();
  });
});

describe("SettingsPanel.svelte (toastCommandError): a locked rejection shows no toast", () => {
  it("handleImport: import_clips rejects locked — no toast, only the PIN prompt", async () => {
    vi.mocked(dialogOpen).mockResolvedValue("C:\\Users\\me\\clips.json");
    let mock: Awaited<ReturnType<typeof renderReadyApp>>["mock"];
    ({ mock } = await renderReadyApp([CLIP], {
      import_clips: () => {
        mock.emit("lock_state", LOCKED);
        throw { kind: "locked" };
      },
    }));
    await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await waitFor(() => expect(screen.getByRole("switch")).toBeInTheDocument());
    await fireEvent.click(screen.getByRole("button", { name: /^Import clips/ }));

    await assertLockedAndSilent();
  });
});

// See tests/locked-suppresses-toast-redproof.test.ts for the demonstration
// that the four tests above are discriminating — i.e. that they fail if a
// call site regresses to toasting `locked`. That proof lives in its own
// file because it statically mocks `errorMessage.ts` for every import in
// the file (`vi.mock` is hoisted); sharing a file with the tests above,
// which need the real module, produced `Svelte error: effect_orphan` from
// `vi.resetModules()` disturbing Svelte's own runtime module — a tooling
// artefact of this repo's Vitest setup, not a defect in the app.
