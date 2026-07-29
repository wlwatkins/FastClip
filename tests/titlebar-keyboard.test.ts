import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";
import { afterEach } from "vitest";

// WP-10 (docs/src/work/wp-10-palette.md, test-engineer share): "Keyboard
// operation of every control." The current surface is narrow — WP-05 has
// not landed, so TitleBar.svelte's minimise and close buttons are the only
// interactive elements in the app (see src/App.svelte, which otherwise has
// an empty <main>). This suite covers exactly those two controls and
// invents nothing beyond them.

const minimize = vi.fn().mockResolvedValue(undefined);
const close = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ minimize, close }),
}));

// Imported after the mock is registered so the component picks it up.
const { default: TitleBar } = await import("../src/lib/components/TitleBar.svelte");

beforeEach(() => {
  minimize.mockClear();
  close.mockClear();
});

afterEach(() => {
  cleanup();
});

describe("TitleBar keyboard operation", () => {
  it("exposes exactly the minimise and close buttons as tab stops, in DOM order", () => {
    const { container } = render(TitleBar);
    const buttons = container.querySelectorAll("button");
    expect(buttons).toHaveLength(2);
    expect(buttons[0]).toHaveAccessibleName("Minimise window");
    expect(buttons[1]).toHaveAccessibleName("Close window");
  });

  it("does not remove either button from the tab order (no explicit tabindex=-1)", () => {
    const { getByRole } = render(TitleBar);
    const minimiseBtn = getByRole("button", { name: "Minimise window" });
    const closeBtn = getByRole("button", { name: "Close window" });
    expect(minimiseBtn).not.toHaveAttribute("tabindex", "-1");
    expect(closeBtn).not.toHaveAttribute("tabindex", "-1");
  });

  it("is reachable by Tab: focusing each button in turn works", async () => {
    const { getByRole } = render(TitleBar);
    const minimiseBtn = getByRole("button", { name: "Minimise window" });
    const closeBtn = getByRole("button", { name: "Close window" });

    minimiseBtn.focus();
    expect(document.activeElement).toBe(minimiseBtn);

    closeBtn.focus();
    expect(document.activeElement).toBe(closeBtn);
  });

  // jsdom does not implement native keyboard activation of a <button> — it
  // does not turn a keydown for Enter or Space into a click, unlike a real
  // browser. So there is no way, from jsdom alone, to fire a real Enter or
  // Space keystroke and observe the browser's own activation behaviour; the
  // two tests below can only assert that the handler fires on a click event,
  // which is a check of the handler wiring, not of Enter/Space activation.
  // Real keyboard activation of a native, unmodified <button type="button">
  // (no keydown/keyup handler intercepts or preventDefaults it — confirmed
  // by reading TitleBar.svelte, which attaches only `onclick`) is a browser
  // platform guarantee, not something this suite demonstrates. A regression
  // this cannot catch: adding a keydown handler on either button that calls
  // preventDefault() for "Enter" would silently break real Enter activation
  // while leaving both tests below green.
  it("invokes minimize() when the minimise button's onclick handler fires (does not test real Enter/Space keyboard dispatch — see comment above)", async () => {
    const { getByRole } = render(TitleBar);
    const minimiseBtn = getByRole("button", { name: "Minimise window" });
    await fireEvent.click(minimiseBtn);
    expect(minimize).toHaveBeenCalled();
  });

  it("invokes close() when the close button's onclick handler fires (does not test real Enter/Space keyboard dispatch — see comment above)", async () => {
    const { getByRole } = render(TitleBar);
    const closeBtn = getByRole("button", { name: "Close window" });
    await fireEvent.click(closeBtn);
    expect(close).toHaveBeenCalled();
  });

  it("both buttons carry an unambiguous accessible name (aria-label), not reliant on visible text", () => {
    const { getByRole } = render(TitleBar);
    expect(getByRole("button", { name: "Minimise window" })).toHaveAttribute(
      "aria-label",
      "Minimise window",
    );
    expect(getByRole("button", { name: "Close window" })).toHaveAttribute(
      "aria-label",
      "Close window",
    );
  });

  it("both buttons carry a focus-visible style distinct from the hover style", () => {
    // Behavioural proxy, not a rendered-pixel check (jsdom does not run
    // layout/paint): asserts the class list declares a `focus-visible:`
    // treatment separate from `hover:`, so keyboard focus is not relying on
    // the hover style alone for its indicator. This does not prove the
    // colours differ visually — see "Not covered" in the test report.
    const { getByRole } = render(TitleBar);
    for (const name of ["Minimise window", "Close window"]) {
      const btn = getByRole("button", { name });
      const classes = btn.getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
      expect(classes).toMatch(/hover:/);
    }
  });
});
