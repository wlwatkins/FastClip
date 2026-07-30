import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";
import { afterEach } from "vitest";

// WP-10 (docs/src/work/wp-10-palette.md, test-engineer share): "Keyboard
// operation of every control." WP-14 added a Settings button to this
// component (contract §2 `get_settings`/`set_always_on_top` work while
// locked, so the affordance lives in the title bar rather than the
// phase-dependent body — see src/lib/components/TitleBar.svelte). This
// suite covers the settings, minimise and close buttons and invents nothing
// beyond them.

const minimize = vi.fn().mockResolvedValue(undefined);
const close = vi.fn().mockResolvedValue(undefined);
const onopensettings = vi.fn();

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ minimize, close }),
}));

// Imported after the mock is registered so the component picks it up.
const { default: TitleBar } = await import("../src/lib/components/TitleBar.svelte");

beforeEach(() => {
  minimize.mockClear();
  close.mockClear();
  onopensettings.mockClear();
});

afterEach(() => {
  cleanup();
});

describe("TitleBar keyboard operation", () => {
  it("exposes exactly the settings, minimise and close buttons as tab stops, in DOM order", () => {
    const { container } = render(TitleBar, { onopensettings });
    const buttons = container.querySelectorAll("button");
    expect(buttons).toHaveLength(3);
    expect(buttons[0]).toHaveAccessibleName("Settings");
    expect(buttons[1]).toHaveAccessibleName("Minimise window");
    expect(buttons[2]).toHaveAccessibleName("Close window");
  });

  it("does not remove any of the three buttons from the tab order (no explicit tabindex=-1)", () => {
    // WP-14 (test-engineer share): the original version of this test checked
    // only the minimise and close buttons, despite its title claiming
    // "either button" for what is now three. The settings button gets the
    // same check the other two already had, closing that gap.
    const { getByRole } = render(TitleBar, { onopensettings });
    const settingsBtn = getByRole("button", { name: "Settings" });
    const minimiseBtn = getByRole("button", { name: "Minimise window" });
    const closeBtn = getByRole("button", { name: "Close window" });
    expect(settingsBtn).not.toHaveAttribute("tabindex", "-1");
    expect(minimiseBtn).not.toHaveAttribute("tabindex", "-1");
    expect(closeBtn).not.toHaveAttribute("tabindex", "-1");
  });

  it("is reachable by Tab: focusing each button in turn works", async () => {
    // WP-14 (test-engineer share): extended to the settings button, which
    // the original version of this test skipped.
    const { getByRole } = render(TitleBar, { onopensettings });
    const settingsBtn = getByRole("button", { name: "Settings" });
    const minimiseBtn = getByRole("button", { name: "Minimise window" });
    const closeBtn = getByRole("button", { name: "Close window" });

    settingsBtn.focus();
    expect(document.activeElement).toBe(settingsBtn);

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
    const { getByRole } = render(TitleBar, { onopensettings });
    const minimiseBtn = getByRole("button", { name: "Minimise window" });
    await fireEvent.click(minimiseBtn);
    expect(minimize).toHaveBeenCalled();
  });

  it("invokes close() when the close button's onclick handler fires (does not test real Enter/Space keyboard dispatch — see comment above)", async () => {
    const { getByRole } = render(TitleBar, { onopensettings });
    const closeBtn = getByRole("button", { name: "Close window" });
    await fireEvent.click(closeBtn);
    expect(close).toHaveBeenCalled();
  });

  it("invokes onopensettings when the settings button's onclick handler fires (does not test real Enter/Space keyboard dispatch — see comment above)", async () => {
    const { getByRole } = render(TitleBar, { onopensettings });
    const settingsBtn = getByRole("button", { name: "Settings" });
    await fireEvent.click(settingsBtn);
    expect(onopensettings).toHaveBeenCalled();
  });

  it("all three buttons carry an unambiguous accessible name (aria-label), not reliant on visible text", () => {
    const { getByRole } = render(TitleBar, { onopensettings });
    for (const name of ["Settings", "Minimise window", "Close window"]) {
      expect(getByRole("button", { name })).toHaveAttribute("aria-label", name);
    }
  });

  it("all three buttons carry a focus-visible style distinct from the hover style", () => {
    // Behavioural proxy, not a rendered-pixel check (jsdom does not run
    // layout/paint): asserts the class list declares a `focus-visible:`
    // treatment separate from `hover:`, so keyboard focus is not relying on
    // the hover style alone for its indicator. This does not prove the
    // colours differ visually — see "Not covered" in the test report.
    const { getByRole } = render(TitleBar, { onopensettings });
    for (const name of ["Settings", "Minimise window", "Close window"]) {
      const btn = getByRole("button", { name });
      const classes = btn.getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
      expect(classes).toMatch(/hover:/);
    }
  });
});
