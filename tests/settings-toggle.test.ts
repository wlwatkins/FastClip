import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-14 (test-engineer share): the always-on-top toggle.
//
// contract §1 `Settings`, §2 `get_settings`/`set_always_on_top`: the toggle
// reads its initial value from `get_settings` at startup and writes through
// `set_always_on_top`; the backend both persists and applies the setting, so
// the frontend must call no Tauri window API for it
// (src-tauri/capabilities/default.json: `core:window:deny-set-always-on-top`).
//
// wp-14-settings.md definition of done: "reachable by Tab, operable by Enter
// and Space, carries an accessible name, and shows a focus indicator distinct
// from hover."

const CLIP: Clip = {
  id: "11111111-1111-1111-1111-111111111111",
  label: "Greeting",
  value: "Hello there",
  colour: "blue",
};

async function renderAppWithSettingsOpen(alwaysOnTop: boolean, overrides: Partial<Record<string, (args: unknown) => unknown>> = {}) {
  const mock = setupTauriMock({
    list_clips: () => [CLIP],
    get_settings: () => ({ always_on_top: alwaysOnTop }),
    get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
    ...overrides,
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());
  await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  await waitFor(() => expect(screen.getByRole("switch")).toBeInTheDocument());
  return { ...utils, mock };
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

describe("always-on-top toggle: initial value from get_settings", () => {
  it("reflects true when get_settings resolves always_on_top: true", async () => {
    await renderAppWithSettingsOpen(true);
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "true");
  });

  it("reflects false when get_settings resolves always_on_top: false", async () => {
    await renderAppWithSettingsOpen(false);
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "false");
  });
});

describe("always-on-top toggle: writes through set_always_on_top", () => {
  it("clicking the toggle calls set_always_on_top with the flipped value, and aria-checked tracks the confirmed result", async () => {
    const setAlwaysOnTop = vi.fn(() => null);
    await renderAppWithSettingsOpen(false, { set_always_on_top: setAlwaysOnTop });

    const toggle = screen.getByRole("switch");
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await fireEvent.click(toggle);

    await waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalledWith({ enabled: true }));
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
  });

  it("toggling twice flips it back, and the second call carries the opposite value", async () => {
    const setAlwaysOnTop = vi.fn(() => null);
    await renderAppWithSettingsOpen(true, { set_always_on_top: setAlwaysOnTop });

    const toggle = screen.getByRole("switch");
    await fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "false"));
    await fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));

    expect(setAlwaysOnTop).toHaveBeenNthCalledWith(1, { enabled: false });
    expect(setAlwaysOnTop).toHaveBeenNthCalledWith(2, { enabled: true });
  });

  it("does not flip aria-checked when set_always_on_top rejects — the toggle reflects only a backend-confirmed value", async () => {
    const setAlwaysOnTop = vi.fn(() => {
      throw { kind: "storage" };
    });
    await renderAppWithSettingsOpen(false, { set_always_on_top: setAlwaysOnTop });

    const toggle = screen.getByRole("switch");
    await fireEvent.click(toggle);

    await waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalled());
    // Give the rejected promise's catch handler a turn.
    await new Promise((r) => setTimeout(r, 0));
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });
});

describe("always-on-top toggle: no Tauri window API", () => {
  it("never invokes the window plugin's set_always_on_top command, even after toggling", async () => {
    const setAlwaysOnTop = vi.fn(() => null);
    const { mock } = await renderAppWithSettingsOpen(false, { set_always_on_top: setAlwaysOnTop });

    const toggle = screen.getByRole("switch");
    await fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));

    // @tauri-apps/api/window's Window.setAlwaysOnTop() invokes exactly this
    // command (node_modules/@tauri-apps/api/window.js). Its absence from the
    // observed IPC traffic is what proves the frontend never called it —
    // the persisted-and-applied setting is entirely the backend's write,
    // never a second one made from the webview.
    expect(mock.calls.some((c) => c.cmd === "plugin:window|set_always_on_top")).toBe(false);
    // And the one command that is expected to have carried the change:
    expect(mock.calls.some((c) => c.cmd === "set_always_on_top")).toBe(true);
  });
});

describe("always-on-top toggle: keyboard and accessible name", () => {
  it("is a real button in the tab order (no tabindex=-1) and carries an accessible name via aria-labelledby", async () => {
    await renderAppWithSettingsOpen(false);
    const toggle = screen.getByRole("switch");
    expect(toggle.tagName).toBe("BUTTON");
    expect(toggle).not.toHaveAttribute("tabindex", "-1");
    expect(toggle).toHaveAccessibleName("Keep window on top");
  });

  it("is reachable by Tab (can receive focus)", async () => {
    await renderAppWithSettingsOpen(false);
    const toggle = screen.getByRole("switch");
    toggle.focus();
    expect(document.activeElement).toBe(toggle);
  });

  // jsdom does not turn a keydown for Enter/Space into a synthesised click on
  // a native <button> (confirmed in tests/titlebar-keyboard.test.ts's own
  // comment on the same limitation). The toggle is a plain
  // `<button type="button" role="switch">` with only an `onclick` handler and
  // no keydown/keyup listener that could intercept or preventDefault a native
  // key activation, so real Enter/Space activation is the same browser
  // platform guarantee TitleBar's buttons rely on — not demonstrated here.
  // This is recorded under "Not covered" in the test report.
  it("is operable by click (the same handler real Enter/Space activation would invoke — see comment above)", async () => {
    const setAlwaysOnTop = vi.fn(() => null);
    await renderAppWithSettingsOpen(false, { set_always_on_top: setAlwaysOnTop });
    const toggle = screen.getByRole("switch");
    toggle.focus();
    await fireEvent.click(toggle);
    await waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalled());
  });

  it("carries both a hover style and a focus-visible style, and the two are distinct declarations (wp-14-settings.md definition of done)", async () => {
    await renderAppWithSettingsOpen(false);
    const toggle = screen.getByRole("switch");
    const classes = toggle.getAttribute("class") ?? "";
    expect(classes).toMatch(/focus-visible:outline/);
    expect(classes).toMatch(/hover:/);
  });
});
