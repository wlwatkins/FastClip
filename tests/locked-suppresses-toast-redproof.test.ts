import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// Proof that tests/locked-suppresses-toast.test.ts's four assertions are
// discriminating — i.e. that they would fail if a call site regressed to
// toasting `locked` — without editing App.svelte or errorMessage.ts.
// `describeError` is mocked back to the pre-F2 shape (a sentence for
// `locked` instead of `null`) and the exact same handleCopy scenario as
// that file's first test is driven through the real, unmodified App.svelte.
// App.svelte's own code — `const message = describeError(err.error); if
// (message !== null) showToast(message);` — is untouched; only the value
// `describeError` hands back is swapped, reproducing what that line saw
// before F2 landed.
//
// This lives in its own file because `vi.mock` is hoisted and applies to
// every import in the file it is written in; a copy of this mock in the
// same file as the real-code assertions would poison them.

vi.mock("../src/lib/errorMessage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../src/lib/errorMessage")>();
  return {
    ...actual,
    describeError: (error: { kind: string }) =>
      error.kind === "locked"
        ? "That action is not available right now." // the pre-F2 shape: a real sentence instead of null
        : actual.describeError(error as never),
  };
});

const CLIP: Clip = { id: "11111111-1111-1111-1111-111111111111", label: "Greeting", value: "Hello there", colour: "blue" };
const CLIP_2: Clip = { id: "22222222-2222-2222-2222-222222222222", label: "Sign-off", value: "Best, Team", colour: "amber" };
const UNLOCKED = { encryption_enabled: true, locked: false, attempts_remaining: null, retry_after_ms: null };
const LOCKED = { encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null };

function toastText(): string {
  const el = document.querySelector(".pointer-events-none.fixed.inset-x-0.bottom-3");
  if (el === null) throw new Error("Toast live region not found");
  return el.textContent ?? "";
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("RED proof: with describeError('locked') returning a sentence (pre-F2 shape), the real handleCopy call site toasts it", () => {
  it("a toast with the leaked sentence appears — the assertion tests/locked-suppresses-toast.test.ts makes (toastText() === '') would fail here", async () => {
    let mock: ReturnType<typeof setupTauriMock>;
    mock = setupTauriMock({
      list_clips: () => [CLIP, CLIP_2],
      get_settings: () => ({ always_on_top: false }),
      get_lock_state: () => UNLOCKED,
      copy_clip: () => {
        mock.emit("lock_state", LOCKED);
        throw { kind: "locked" };
      },
    });
    const { default: App } = await import("../src/App.svelte");
    render(App);
    await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());

    const row = screen.getByRole("button", { name: CLIP.label });
    await fireEvent.click(row);

    await waitFor(() => expect(screen.getByRole("heading", { name: "FastClip is locked" })).toBeInTheDocument());
    // Deliberately not `waitFor`: waitFor would report success on its very
    // first (immediate) poll, before the rejected copyClip promise's catch
    // handler has run — a false pass, not a real wait. A fixed settle delay
    // after an awaited state transition is used instead, same technique as
    // tests/app-lock-transition.test.ts's "give any (incorrect) reactive
    // repaint a turn to happen before asserting its absence".
    await new Promise((r) => setTimeout(r, 50));
    expect(toastText()).toBe("That action is not available right now.");
  });
});
