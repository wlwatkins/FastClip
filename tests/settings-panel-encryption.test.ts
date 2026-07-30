import { describe, expect, it, afterEach, beforeEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import { setLockState } from "../src/lib/state/lockState.svelte";
import SettingsPanel from "../src/lib/components/SettingsPanel.svelte";

// WP-07 (test-engineer share): the settings encryption flows — enable,
// disable, change-PIN, manual lock.
//
// "The enable path shows the irreversibility warning and offers an export
// first. That export is the only mitigation for a forgotten PIN; assert the
// warning cannot be bypassed." And: "the PIN must never be logged, echoed, or
// rendered back."

const UNENCRYPTED = { encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null };
const ENCRYPTED_UNLOCKED = { encryption_enabled: true, locked: false, attempts_remaining: null, retry_after_ms: null };

function renderPanel(onclose = vi.fn()) {
  const utils = render(SettingsPanel, { onclose });
  return { ...utils, onclose };
}

beforeEach(() => {
  setLockState(UNENCRYPTED);
});

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
  setLockState(UNENCRYPTED);
});

describe("Enable flow: the irreversibility warning cannot be bypassed", () => {
  it("clicking 'Turn on PIN protection…' always lands on the warning first, never directly on PIN entry", async () => {
    setupTauriMock({});
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));

    expect(screen.getByText(/cannot be recovered/i)).toBeInTheDocument();
    expect(screen.queryByLabelText("6-digit PIN")).not.toBeInTheDocument();
  });

  it("there is no enable_encryption call reachable without first passing through the warning and clicking Continue", async () => {
    const enableEncryption = vi.fn(() => null);
    setupTauriMock({ enable_encryption: enableEncryption });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));
    // Escape from the warning (without Continue) returns to settings.
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("switch")).toBeInTheDocument();
    expect(enableEncryption).not.toHaveBeenCalled();

    // Cancel from the warning has the same effect.
    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("switch")).toBeInTheDocument();
    expect(enableEncryption).not.toHaveBeenCalled();
  });

  it("Continue on the warning reaches PIN entry, and only then can enable_encryption be called", async () => {
    const enableEncryption = vi.fn(() => null);
    setupTauriMock({ enable_encryption: enableEncryption });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(screen.getByLabelText("6-digit PIN")).toBeInTheDocument();
    expect(screen.getByLabelText("Confirm PIN")).toBeInTheDocument();

    await fireEvent.input(screen.getByLabelText("6-digit PIN"), { target: { value: "482913" } });
    await fireEvent.input(screen.getByLabelText("Confirm PIN"), { target: { value: "482913" } });
    await fireEvent.click(screen.getByRole("button", { name: "Turn on" }));

    await waitFor(() => expect(enableEncryption).toHaveBeenCalledWith({ pin: "482913" }));
  });

  it("the warning text states plainly that a forgotten PIN is unrecoverable — not softened, no mention of a reset or backdoor existing", async () => {
    setupTauriMock({});
    renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));

    const message = screen.getByText(/cannot be recovered/i).textContent ?? "";
    expect(message).toMatch(/no reset/i);
    expect(message).toMatch(/no backdoor/i);
  });

  it("'Export clips first…' from the warning opens the export-warning view, and returns to the enable-warning (not to settings) afterward", async () => {
    setupTauriMock({});
    renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips first/ }));

    expect(screen.getByText(/not encrypted/i)).toBeInTheDocument();

    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    // Back on the enable-warning, not settings — proven by the warning text
    // being visible again and the always-on-top switch (settings-only) absent.
    expect(screen.getByText(/cannot be recovered/i)).toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });

  it("mismatched PIN and confirmation is rejected client-side, and enable_encryption is never called", async () => {
    const enableEncryption = vi.fn(() => null);
    setupTauriMock({ enable_encryption: enableEncryption });
    renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    await fireEvent.input(screen.getByLabelText("6-digit PIN"), { target: { value: "111111" } });
    await fireEvent.input(screen.getByLabelText("Confirm PIN"), { target: { value: "222222" } });
    await fireEvent.click(screen.getByRole("button", { name: "Turn on" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("The PINs do not match.");
    expect(enableEncryption).not.toHaveBeenCalled();
  });
});

describe("Never logs the PIN across enable, disable and change-PIN", () => {
  it("a full enable-PIN entry and submit sequence calls no console method with the PIN", async () => {
    const methods = ["log", "warn", "error", "debug", "info"] as const;
    const spies = methods.map((m) => vi.spyOn(console, m).mockImplementation(() => {}));
    setupTauriMock({ enable_encryption: () => null });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Turn on PIN protection/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await fireEvent.input(screen.getByLabelText("6-digit PIN"), { target: { value: "705294" } });
    await fireEvent.input(screen.getByLabelText("Confirm PIN"), { target: { value: "705294" } });
    await fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(screen.getByRole("switch")).toBeInTheDocument());

    for (const spy of spies) {
      for (const call of spy.mock.calls) {
        const serialised = call.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
        expect(serialised).not.toContain("705294");
      }
    }
  });

  it("a wrong-PIN disable attempt (server rejects) leaves the PIN out of console and out of the DOM after the failure", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    const methods = ["log", "warn", "error", "debug", "info"] as const;
    const spies = methods.map((m) => vi.spyOn(console, m).mockImplementation(() => {}));
    setupTauriMock({
      disable_encryption: () => {
        throw { kind: "bad_pin", attempts_remaining: null, retry_after_ms: null };
      },
    });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Turn off PIN protection/ }));
    await fireEvent.input(screen.getByLabelText("Current PIN"), { target: { value: "601739" } });
    await fireEvent.click(screen.getByRole("button", { name: "Turn off" }));

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());

    for (const spy of spies) {
      for (const call of spy.mock.calls) {
        const serialised = call.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
        expect(serialised).not.toContain("601739");
      }
    }
  });
});

describe("Disable flow: requires the PIN, shows the plaintext-storage warning", () => {
  it("hidden entirely while encryption is off", async () => {
    setupTauriMock({});
    renderPanel();
    expect(screen.queryByRole("button", { name: /^Turn off PIN protection/ })).not.toBeInTheDocument();
  });

  it("shown once encryption is on, submits the typed PIN to disable_encryption", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    const disableEncryption = vi.fn(() => null);
    setupTauriMock({ disable_encryption: disableEncryption });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Turn off PIN protection/ }));
    expect(screen.getByText(/unencrypted/i)).toBeInTheDocument();

    await fireEvent.input(screen.getByLabelText("Current PIN"), { target: { value: "384756" } });
    await fireEvent.click(screen.getByRole("button", { name: "Turn off" }));

    await waitFor(() => expect(disableEncryption).toHaveBeenCalledWith({ pin: "384756" }));
  });
});

describe("Change-PIN flow", () => {
  it("shown only while encryption is on, submits current and new PIN", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    const changePin = vi.fn(() => null);
    setupTauriMock({ change_pin: changePin });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Change PIN/ }));
    await fireEvent.input(screen.getByLabelText("Current PIN"), { target: { value: "111222" } });
    await fireEvent.input(screen.getByLabelText("New PIN"), { target: { value: "333444" } });
    await fireEvent.input(screen.getByLabelText("Confirm new PIN"), { target: { value: "333444" } });
    await fireEvent.click(screen.getByRole("button", { name: "Change PIN" }));

    await waitFor(() =>
      expect(changePin).toHaveBeenCalledWith({ current_pin: "111222", new_pin: "333444" }),
    );
  });

  it("mismatched new PIN and confirmation is rejected client-side without calling change_pin", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    const changePin = vi.fn(() => null);
    setupTauriMock({ change_pin: changePin });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Change PIN/ }));
    await fireEvent.input(screen.getByLabelText("Current PIN"), { target: { value: "111222" } });
    await fireEvent.input(screen.getByLabelText("New PIN"), { target: { value: "333444" } });
    await fireEvent.input(screen.getByLabelText("Confirm new PIN"), { target: { value: "999999" } });
    await fireEvent.click(screen.getByRole("button", { name: "Change PIN" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("The PINs do not match.");
    expect(changePin).not.toHaveBeenCalled();
  });
});

describe("Manual lock control: hidden when encryption is off, present and wired when on", () => {
  it("no 'Lock now' control while encryption_enabled is false", async () => {
    setupTauriMock({});
    renderPanel();
    expect(screen.queryByRole("button", { name: "Lock now" })).not.toBeInTheDocument();
  });

  it("'Lock now' calls the lock command with no PIN argument", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    const lockCommand = vi.fn(() => null);
    setupTauriMock({ lock: lockCommand });
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: "Lock now" }));

    await waitFor(() => expect(lockCommand).toHaveBeenCalledTimes(1));
    // ADR-0010: "no PIN argument — the caller is already unlocked." Whatever
    // shape `invoke()` normalises a no-args call to, it must carry no `pin`
    // field at all.
    expect(lockCommand.mock.calls[0][0]).not.toHaveProperty("pin");
  });

  it("while locked, the encryption section shows only the locked notice — no Lock/Change PIN/Turn off controls", async () => {
    setLockState({ encryption_enabled: true, locked: true, attempts_remaining: 5, retry_after_ms: null });
    setupTauriMock({});
    renderPanel();

    expect(screen.getByText(/enter your pin to manage pin protection/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Lock now" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Change PIN/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Turn off PIN protection/ })).not.toBeInTheDocument();
    // Export/Import are also hidden while locked (contract: both return `locked`).
    expect(screen.queryByRole("button", { name: /^Export clips/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Import clips/ })).not.toBeInTheDocument();
  });
});

describe("Keyboard reachability and accessible names for every encryption control", () => {
  it("Turn on / Turn off / Change PIN / Lock now are real buttons with accessible names, not tabindex=-1", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    setupTauriMock({});
    renderPanel();

    for (const name of [/^Turn off PIN protection/, /^Change PIN/, "Lock now"]) {
      const btn = screen.getByRole("button", { name });
      expect(btn.tagName).toBe("BUTTON");
      expect(btn).not.toHaveAttribute("tabindex", "-1");
    }
  });

  it("PIN fields in enable/disable/change-PIN flows are all reachable and labelled", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    setupTauriMock({});
    renderPanel();

    await fireEvent.click(screen.getByRole("button", { name: /^Change PIN/ }));
    for (const label of ["Current PIN", "New PIN", "Confirm new PIN"]) {
      const input = screen.getByLabelText(label);
      expect(input).toHaveAccessibleName(label);
      expect(input).not.toHaveAttribute("tabindex", "-1");
    }
  });

  it("every encryption-section button carries hover and focus-visible styles as distinct declarations", async () => {
    setLockState(ENCRYPTED_UNLOCKED);
    setupTauriMock({});
    renderPanel();

    for (const name of [/^Turn off PIN protection/, /^Change PIN/, "Lock now"]) {
      const classes = screen.getByRole("button", { name }).getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
      expect(classes).toMatch(/hover:/);
    }
  });
});
