import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor, fireEvent } from "@testing-library/svelte";
import { setupTauriMock, teardownTauriMock } from "./helpers/mockTauri";
import type { Clip } from "../src/lib/contract/types";

// WP-05 task brief: "the background is marked inert while a dialog is open,
// so background controls should drop out of the tab order entirely."
//
// This checks the wiring App.svelte controls: the element's `inert` IDL
// property is set exactly when a dialog is open, and cleared when it closes.
// Svelte's compiled output assigns `inert` as a DOM property rather than an
// HTML attribute whenever the target's setter is available (Svelte
// `set_attribute`, `get_setters` path) — jsdom exposes no such setter, so
// `element.inert = true` lands as a plain, non-reflected JS property and
// `toHaveAttribute("inert")` never sees it; the property read below is what
// this test suite actually needs to check the same fact real Chromium would
// reflect as the attribute.
//
// It does NOT check that inert actually removes focusability in the running
// app: confirmed by direct probe, jsdom neither reflects `el.inert = true`
// to the `inert` attribute nor blocks `.focus()` on a descendant of an inert
// element, in either assignment form. That part of the platform's inert
// behaviour is unavailable to this suite. See "Not covered" in the test
// report.

const CLIP: Clip = {
  id: "11111111-1111-1111-1111-111111111111",
  label: "Greeting",
  value: "Hello there",
  colour: "blue",
};

async function renderApp() {
  setupTauriMock({
    list_clips: () => [CLIP],
    get_settings: () => ({ always_on_top: false }),
    get_lock_state: () => ({ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }),
  });
  const { default: App } = await import("../src/App.svelte");
  const utils = render(App);
  await waitFor(() => expect(screen.getByText(CLIP.label)).toBeInTheDocument());
  return utils;
}

afterEach(() => {
  cleanup();
  teardownTauriMock();
  vi.restoreAllMocks();
});

function backgroundWrapper(container: HTMLElement): HTMLElement {
  // App.svelte: <div inert={dialogOpen} class="flex min-h-0 flex-1 flex-col"> wraps
  // TitleBar + the "New clip" button + the clip list — everything but the dialogs and toast.
  const el = container.querySelector('[aria-label="Clips"]')!.closest("div.flex.min-h-0");
  expect(el).not.toBeNull();
  return el as HTMLElement;
}

describe("background inert state while a dialog is open", () => {
  it("is not inert at rest", async () => {
    const { container } = await renderApp();
    expect((backgroundWrapper(container) as any).inert).not.toBe(true);
  });

  it("becomes inert the instant the create form opens", async () => {
    const { container } = await renderApp();
    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect((backgroundWrapper(container) as any).inert).toBe(true);
  });

  it("stops being inert again once the form is cancelled", async () => {
    const { container } = await renderApp();
    await fireEvent.click(screen.getByRole("button", { name: "New clip" }));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect((backgroundWrapper(container) as any).inert).not.toBe(true);
  });

  it("becomes inert while the delete confirmation is open", async () => {
    const { container } = await renderApp();
    await fireEvent.click(screen.getByRole("button", { name: `Delete ${CLIP.label}` }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeInTheDocument());
    expect((backgroundWrapper(container) as any).inert).toBe(true);
  });
});
