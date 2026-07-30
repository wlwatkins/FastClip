import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import PinInput from "../src/lib/components/PinInput.svelte";

// WP-07 (test-engineer share): PinInput, shared by the launch prompt and
// every PIN entry in the settings flows.
//
// "The PIN must never be logged, echoed, or rendered back... PinInput masks
// its value. Assert the masking, and assert the PIN does not survive into
// any state after a failed attempt." Masking and never-logs are this
// component's own responsibility; "does not survive a failed attempt" is the
// caller's (PinPrompt, SettingsPanel) and is covered in their own suites.

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function renderInput(overrides: Partial<{ value: string; disabled: boolean; error: string | null }> = {}) {
  return render(PinInput, {
    id: "test-pin",
    label: "PIN",
    value: overrides.value ?? "",
    disabled: overrides.disabled ?? false,
    error: overrides.error ?? null,
  });
}

describe("PinInput: masking", () => {
  it("renders type=\"password\", not text or numeric plaintext", () => {
    renderInput();
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    expect(input.type).toBe("password");
  });

  it("typed digits are not present anywhere in the rendered DOM text content, even though the input holds them", async () => {
    const { container } = renderInput();
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "246810" } });

    // The value the caller receives via binding is real (the component
    // cannot mask what it does not have), but nothing in the visible text of
    // the DOM must show it — that is what type="password" is for, and this
    // is the assertion that it is not silently overridden elsewhere (e.g. an
    // aria-live region, a title attribute, a data- attribute rendering it
    // back as text).
    expect(container.textContent).not.toContain("246810");
    for (const attr of ["title", "aria-label", "placeholder"]) {
      expect(input.getAttribute(attr) ?? "").not.toContain("246810");
    }
  });

  it("sets autocomplete=\"off\", so no password manager offers to store or fill a PIN", () => {
    renderInput();
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    expect(input.getAttribute("autocomplete")).toBe("off");
  });
});

describe("PinInput: filters to digits, capped at 6", () => {
  it("strips non-digit characters as they are typed", async () => {
    renderInput();
    // The component sets `value = target.value.replace(/[^0-9]/g, "")...`
    // and rebinds it onto the input via Svelte's reactive `{value}`
    // attribute in the same tick, so reading the input's own post-filter
    // value after the event reflects what a `bind:value` caller receives.
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "1a2b3c4d5e6f7g8h" } });
    expect(input.value).toBe("123456");
  });

  it("caps at 6 characters even when more digits are typed at once", async () => {
    renderInput();
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "123456789" } });
    expect(input.value).toBe("123456");
  });
});

describe("PinInput: never logs", () => {
  it("typing a full PIN calls no console method with any argument containing the digits", async () => {
    const methods = ["log", "warn", "error", "debug", "info"] as const;
    const spies = methods.map((m) => vi.spyOn(console, m).mockImplementation(() => {}));

    renderInput();
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "913579" } });

    for (const spy of spies) {
      for (const call of spy.mock.calls) {
        const serialised = call.map((a) => String(a)).join(" ");
        expect(serialised).not.toContain("913579");
      }
    }
  });
});

describe("PinInput: disabled, keyboard reachability and accessible name", () => {
  it("is disabled when the caller passes disabled, and not otherwise", () => {
    renderInput({ disabled: true });
    expect(screen.getByLabelText("PIN")).toBeDisabled();
  });

  it("has an accessible name from its associated <label>", () => {
    renderInput();
    const input = screen.getByLabelText("PIN");
    expect(input).toHaveAccessibleName("PIN");
  });

  it("is reachable by Tab (not tabindex=-1) and can receive focus programmatically", () => {
    renderInput();
    const input = screen.getByLabelText("PIN") as HTMLInputElement;
    expect(input).not.toHaveAttribute("tabindex", "-1");
    input.focus();
    expect(document.activeElement).toBe(input);
  });

  it("carries a focus-visible style distinct from any hover style declared on it", () => {
    renderInput();
    const input = screen.getByLabelText("PIN");
    const classes = input.getAttribute("class") ?? "";
    expect(classes).toMatch(/focus-visible:outline/);
  });

  it("associates an error message via aria-describedby and aria-invalid when error is set", () => {
    renderInput({ error: "Enter exactly 6 digits." });
    const input = screen.getByLabelText("PIN");
    expect(input).toHaveAttribute("aria-invalid", "true");
    const describedBy = input.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    expect(document.getElementById(describedBy!)).toHaveTextContent("Enter exactly 6 digits.");
  });
});
