import { describe, expect, it, afterEach, beforeEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import { setLockState } from "../src/lib/state/lockState.svelte";
import PinPrompt from "../src/lib/components/PinPrompt.svelte";

// WP-07 (test-engineer share): the launch PIN prompt.
//
// "Wrong-PIN and backoff states show attempts remaining and the wait. Flat 30
// seconds — assert the display does not imply escalation." And: "the PIN
// does not survive into any state after a failed attempt."
//
// `lockState` is a module-level singleton (src/lib/state/lockState.svelte.ts)
// that PinPrompt reads once, at mount, to seed its local `attemptsRemaining`
// and `retryAfterMs`. Every test resets it via `setLockState` before
// rendering, so no test observes a value a previous test left behind.

const UNLOCKED_DEFAULT = { encryption_enabled: true, locked: true, attempts_remaining: null, retry_after_ms: null };

function typePin(input: HTMLElement, value: string) {
  return fireEvent.input(input, { target: { value } });
}

beforeEach(() => {
  setLockState(UNLOCKED_DEFAULT);
});

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
  setLockState(UNLOCKED_DEFAULT);
});

describe("PinPrompt: submits unlock(pin) and clears the field on any outcome", () => {
  it("calls unlock with the typed 6-digit PIN on submit", async () => {
    const unlock = vi.fn(() => null);
    setupTauriMock({ unlock });
    render(PinPrompt, { onfatal: vi.fn() });

    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await typePin(input, "135791");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(unlock).toHaveBeenCalledWith({ pin: "135791" }));
  });

  it("the submit button stays disabled until exactly 6 digits are entered", async () => {
    setupTauriMock({ unlock: vi.fn(() => null) });
    render(PinPrompt, { onfatal: vi.fn() });

    const button = screen.getByRole("button", { name: "Unlock" });
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    expect(button).toBeDisabled();

    await typePin(input, "12345");
    expect(button).toBeDisabled();

    await typePin(input, "123456");
    expect(button).not.toBeDisabled();
  });
});

describe("PinPrompt: wrong PIN — attempts remaining, and the PIN does not survive a failed attempt", () => {
  it("a bad_pin rejection shows the wrong-PIN message and the attempts-remaining count from the error, and clears the input", async () => {
    const unlock = vi.fn(() => {
      throw { kind: "bad_pin", attempts_remaining: 3, retry_after_ms: null };
    });
    setupTauriMock({ unlock });
    render(PinPrompt, { onfatal: vi.fn() });

    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await typePin(input, "000000");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Wrong PIN."));
    expect(screen.getByRole("status")).toHaveTextContent("3 attempts remaining.");

    // The component is still mounted (not unmounted-and-remounted) — the
    // clearing below is the component's own state being reset, not an
    // artefact of a fresh instance replacing it.
    expect(input.value).toBe("");
    expect(screen.getByLabelText("PIN")).toBe(input);
  });

  it("singular wording at exactly 1 attempt remaining", async () => {
    setupTauriMock({
      unlock: () => {
        throw { kind: "bad_pin", attempts_remaining: 1, retry_after_ms: null };
      },
    });
    render(PinPrompt, { onfatal: vi.fn() });

    await typePin(screen.getByLabelText("PIN"), "000000");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("1 attempt remaining."));
  });

  it("an invalid_input rejection (not the PIN-specific paths) also clears the input, proving the clear is unconditional on the error kind, not special-cased to bad_pin", async () => {
    setupTauriMock({
      unlock: () => {
        throw { kind: "invalid_input", field: "pin", reason: "not_six_digits" };
      },
    });
    render(PinPrompt, { onfatal: vi.fn() });

    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await typePin(input, "000000");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Enter exactly 6 digits."));
    expect(input.value).toBe("");
  });
});

describe("PinPrompt: backoff — flat 30 seconds, no escalation implied", () => {
  it("a backoff rejection disables the input and the button, and shows a wording that does not vary by attempt count", async () => {
    setupTauriMock({
      unlock: () => {
        throw { kind: "backoff", retry_after_ms: 30000 };
      },
    });
    render(PinPrompt, { onfatal: vi.fn() });

    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await typePin(input, "000000");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Too many attempts. Try again in 30 seconds."));
    expect(input).toBeDisabled();
    expect(screen.getByRole("button", { name: "Unlock" })).toBeDisabled();

    // The wording carries only "30 seconds" — never a growing number, a
    // multiplier, or the phrase "next time" that would imply escalation.
    const text = screen.getByRole("status").textContent ?? "";
    expect(text).not.toMatch(/minute|hour|double|increase|longer/i);
  });

  it("a sixth, twentieth, and hundredth wrong PIN — modelled by three consecutive backoff rejections — all produce the identical flat wording, proving there is no escalation across repeated failures", async () => {
    const unlock = vi.fn(() => {
      throw { kind: "backoff", retry_after_ms: 30000 };
    });
    setupTauriMock({ unlock });

    // Three independent mounts, each modelling one more failed attempt past
    // the fifth (the sixth, twentieth and hundredth per ADR-0011's own
    // wording) — a fresh mount per iteration rather than waiting out the
    // 30-second countdown in real time, which would make this test slow
    // without adding anything the wording comparison below does not already
    // prove.
    const seen: string[] = [];
    for (let i = 0; i < 3; i++) {
      setLockState(UNLOCKED_DEFAULT);
      const { unmount } = render(PinPrompt, { onfatal: vi.fn() });
      const freshInput = screen.getAllByLabelText("PIN").at(-1) as HTMLInputElement;
      await typePin(freshInput, "000000");
      await fireEvent.click(screen.getAllByRole("button", { name: "Unlock" }).at(-1)!);
      await waitFor(() => expect(screen.getAllByRole("status").at(-1)).toHaveTextContent("30 seconds"));
      seen.push(screen.getAllByRole("status").at(-1)!.textContent ?? "");
      unmount();
    }
    expect(new Set(seen).size).toBe(1);
    expect(unlock).toHaveBeenCalledTimes(3);
  });
});

describe("PinPrompt: never logs the PIN", () => {
  it("a full submit-and-fail cycle calls no console method with the PIN anywhere in its arguments", async () => {
    const methods = ["log", "warn", "error", "debug", "info"] as const;
    const spies = methods.map((m) => vi.spyOn(console, m).mockImplementation(() => {}));
    setupTauriMock({
      unlock: () => {
        throw { kind: "bad_pin", attempts_remaining: 2, retry_after_ms: null };
      },
    });
    render(PinPrompt, { onfatal: vi.fn() });

    await typePin(screen.getByLabelText("PIN"), "864213");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());

    for (const spy of spies) {
      for (const call of spy.mock.calls) {
        const serialised = call.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
        expect(serialised).not.toContain("864213");
      }
    }
  });
});

describe("PinPrompt: fatal errors escape to the caller rather than being shown inline", () => {
  it("crypto rejects call onfatal, not the inline form error", async () => {
    const onfatal = vi.fn();
    setupTauriMock({
      unlock: () => {
        throw { kind: "crypto", reason: "bad_key_material" };
      },
    });
    render(PinPrompt, { onfatal });

    await typePin(screen.getByLabelText("PIN"), "000000");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(onfatal).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("unsupported_version also calls onfatal", async () => {
    const onfatal = vi.fn();
    setupTauriMock({
      unlock: () => {
        throw { kind: "unsupported_version", component: "key_material", found: 2, supported: 1 };
      },
    });
    render(PinPrompt, { onfatal });

    await typePin(screen.getByLabelText("PIN"), "000000");
    await fireEvent.click(screen.getByRole("button", { name: "Unlock" }));

    await waitFor(() => expect(onfatal).toHaveBeenCalledTimes(1));
  });
});

describe("PinPrompt: keyboard reachability and accessible name", () => {
  it("PIN field and submit button are Tab-reachable with accessible names", async () => {
    setupTauriMock({ unlock: vi.fn(() => null) });
    render(PinPrompt, { onfatal: vi.fn() });

    const input = screen.getByLabelText("PIN");
    const button = screen.getByRole("button", { name: "Unlock" });
    expect(input).not.toHaveAttribute("tabindex", "-1");
    expect(button).not.toHaveAttribute("tabindex", "-1");
    input.focus();
    expect(document.activeElement).toBe(input);
  });

  it("the submit button carries hover and focus-visible styles as distinct declarations", () => {
    setupTauriMock({ unlock: vi.fn(() => null) });
    render(PinPrompt, { onfatal: vi.fn() });
    const classes = screen.getByRole("button", { name: "Unlock" }).getAttribute("class") ?? "";
    expect(classes).toMatch(/focus-visible:outline/);
    expect(classes).toMatch(/hover:/);
  });
});
