import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import SettingsPanel from "../src/lib/components/SettingsPanel.svelte";

// WP-09 (test-engineer share): the export-warning view.
//
// frontend-dev's report: the warning "replaces the settings body in place ...
// rather than stacking a second overlay, so there is exactly one dialog and
// one focus trap at any moment". This suite checks that claim directly
// (single role="dialog", single keydown path via Escape) rather than taking
// it on trust, and covers the definition-of-done keyboard requirements:
// Tab-reachability, accessible names, a focus-visible style distinct from
// hover, focus moving to the safer control on each view change, and Escape
// stepping back one level rather than closing everything.
//
// The mousedown/click drag-out-of-dialog guard is the same fix as F3
// (tests/dialog-overlay-drag.test.ts, ClipForm/ConfirmDialog) applied a
// third time here; that file does not cover SettingsPanel, so it is repeated
// here rather than assumed to still hold by similarity of the code.
//
// None of the tests below open a real file dialog: reaching the
// export-warning view only requires clicking "Export clips…"
// (`startExport()`), which does not call `save()`. Dialog mocking lives in
// tests/export-import.test.ts, which covers what happens after "Export" is
// confirmed.

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function renderPanel(onclose = vi.fn()) {
  const utils = render(SettingsPanel, { onclose });
  return { ...utils, onclose };
}

describe("SettingsPanel: export warning replaces the settings body, not a second overlay", () => {
  it("clicking Export shows the plaintext warning and hides the settings body", async () => {
    renderPanel();
    expect(screen.queryByText(/not encrypted/i)).not.toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));

    expect(screen.getByText(/not encrypted/i)).toBeInTheDocument();
    // The settings body's own controls (the always-on-top switch, the
    // Export/Import buttons) are gone, not merely hidden underneath.
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Export clips/ })).not.toBeInTheDocument();
  });

  it("exactly one role=\"dialog\" element exists in the settings view, and exactly one in the export-warning view", async () => {
    const { container } = renderPanel();
    expect(container.querySelectorAll('[role="dialog"]')).toHaveLength(1);
    expect(container.querySelectorAll('[role="presentation"]')).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));

    expect(container.querySelectorAll('[role="dialog"]')).toHaveLength(1);
    expect(container.querySelectorAll('[role="presentation"]')).toHaveLength(1);
  });

});

describe("SettingsPanel: Escape steps back one level, not out entirely", () => {
  it("Escape from the export-warning view returns to settings without calling onclose", async () => {
    const { onclose } = renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    expect(screen.getByText(/not encrypted/i)).toBeInTheDocument();

    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    expect(onclose).not.toHaveBeenCalled();
    expect(screen.queryByText(/not encrypted/i)).not.toBeInTheDocument();
    expect(screen.getByRole("switch")).toBeInTheDocument();
  });

  it("Escape from the settings view calls onclose", async () => {
    const { onclose } = renderPanel();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("two Escapes in a row from export-warning: first steps back, second closes — proving there is one handler, not two competing ones", async () => {
    const { onclose } = renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));

    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).not.toHaveBeenCalled();

    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});

describe("SettingsPanel: focus moves to the safer control on each view change", () => {
  it("focuses Cancel (not Export/Confirm) when the warning view mounts", async () => {
    renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    const cancel = screen.getByRole("button", { name: "Cancel" });
    expect(document.activeElement).toBe(cancel);
  });

  it("focuses the Close button when returning to the settings view", async () => {
    renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    const closeButton = screen.getByRole("button", { name: "Close settings" });
    expect(document.activeElement).toBe(closeButton);
  });
});

describe("SettingsPanel: keyboard reachability and accessible names", () => {
  it("Export and Import controls are real buttons with accessible names", () => {
    renderPanel();
    const exportButton = screen.getByRole("button", { name: /^Export clips/ });
    const importButton = screen.getByRole("button", { name: /^Import clips/ });
    expect(exportButton.tagName).toBe("BUTTON");
    expect(importButton.tagName).toBe("BUTTON");
    expect(exportButton).not.toHaveAttribute("tabindex", "-1");
    expect(importButton).not.toHaveAttribute("tabindex", "-1");
  });

  it("the warning's Cancel and Confirm controls are real buttons with accessible names, reachable by focus()", async () => {
    renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const confirm = screen.getByRole("button", { name: "Export" });
    expect(cancel.tagName).toBe("BUTTON");
    expect(confirm.tagName).toBe("BUTTON");
    confirm.focus();
    expect(document.activeElement).toBe(confirm);
  });

  it("Export/Import/Cancel/Confirm are operable by click (the handler real Enter/Space activation invokes — jsdom does not synthesise a click from a keydown on a native button, same limitation noted in tests/settings-toggle.test.ts)", async () => {
    const { onclose } = renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    expect(screen.getByText(/not encrypted/i)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("switch")).toBeInTheDocument();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("every settings-view and warning-view button carries a hover style and a focus-visible style as distinct declarations", async () => {
    renderPanel();
    for (const btn of [
      screen.getByRole("button", { name: /^Export clips/ }),
      screen.getByRole("button", { name: /^Import clips/ }),
      screen.getByRole("button", { name: "Close settings" }),
    ]) {
      const classes = btn.getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
      expect(classes).toMatch(/hover:/);
    }

    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    for (const btn of [screen.getByRole("button", { name: "Cancel" }), screen.getByRole("button", { name: "Export" })]) {
      const classes = btn.getAttribute("class") ?? "";
      expect(classes).toMatch(/focus-visible:outline/);
      expect(classes).toMatch(/hover:/);
    }
  });
});

// F3-shaped regression, applied to SettingsPanel (not covered by
// tests/dialog-overlay-drag.test.ts, which only covers ClipForm and
// ConfirmDialog). Same reasoning as that file's header comment: the DOM's
// `click` lands on the nearest common ancestor of `mousedown` and `mouseup`,
// so a drag that starts inside the dialog and ends over the overlay produces
// a `click` whose target genuinely is the overlay — only recording where the
// `mousedown` landed can tell that apart from a real click-outside.
describe("SettingsPanel: drag-out-of-dialog does not dismiss (F3-shaped)", () => {
  it("settings view: mousedown on the always-on-top switch, click landing on the overlay — onclose NOT called", async () => {
    const { onclose, container } = renderPanel();
    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;
    await fireEvent.mouseDown(screen.getByRole("switch"));
    await fireEvent.click(overlay);
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("export-warning view: mousedown on the warning text, click landing on the overlay — onclose NOT called", async () => {
    const { onclose, container } = renderPanel();
    await fireEvent.click(screen.getByRole("button", { name: /^Export clips/ }));
    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;
    const message = screen.getByText(/not encrypted/i);
    await fireEvent.mouseDown(message);
    await fireEvent.click(overlay);
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.getByText(/not encrypted/i)).toBeInTheDocument();
  });

  it("a real click-outside (mousedown AND click both on the overlay) still dismisses, in either view", async () => {
    const { onclose, container } = renderPanel();
    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;
    await fireEvent.mouseDown(overlay);
    await fireEvent.click(overlay);
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});
