import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import ClipForm from "../src/lib/components/ClipForm.svelte";
import ConfirmDialog from "../src/lib/components/ConfirmDialog.svelte";

// F3 (critic finding at WP-05's G3 review, now fixed by frontend-dev):
// "drag-selecting text out of a dialog used to dismiss it and discard the
// edit." The DOM dispatches `click` on the nearest common ancestor of
// `mousedown` and `mouseup` — the overlay itself when the drag starts inside
// the dialog and ends outside it — so `event.target === event.currentTarget`
// on the `click` handler alone cannot distinguish that gesture from a real
// click on the overlay: in both cases the click's target genuinely *is* the
// overlay. The fix (`handleOverlayMouseDown` / `handleOverlayClick` in both
// components) additionally records where the `mousedown` landed and only
// treats the gesture as a dismiss when *both* ends were on the overlay.
//
// A test that only fires `click` on the overlay cannot tell the fixed
// implementation apart from the broken one — both dismiss on that single
// event. Every case below fires the `mousedown`/`click` pair the real
// gesture produces, exactly as the two handlers observe it (neither handler
// looks at `mouseup`), which is what makes this a regression test rather
// than a test of the click handler in isolation.

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ClipForm: drag-out-of-dialog does not discard the edit (F3)", () => {
  it("mousedown starting on the Value textarea, click landing on the overlay: oncancel is NOT called and the form stays mounted", async () => {
    const oncancel = vi.fn();
    const { container } = render(ClipForm, { onsaved: vi.fn(), oncancel });

    const value = screen.getByLabelText("Value");
    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;

    await fireEvent.mouseDown(value);
    await fireEvent.click(overlay);

    expect(oncancel).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("mousedown starting on the Label input, click landing on the overlay: oncancel is NOT called either", async () => {
    const oncancel = vi.fn();
    const { container } = render(ClipForm, { onsaved: vi.fn(), oncancel });

    const label = screen.getByLabelText("Label");
    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;

    await fireEvent.mouseDown(label);
    await fireEvent.click(overlay);

    expect(oncancel).not.toHaveBeenCalled();
  });

  it("a real click-outside (mousedown AND click both on the overlay) still dismisses — the affordance is not broken by the fix", async () => {
    const oncancel = vi.fn();
    const { container } = render(ClipForm, { onsaved: vi.fn(), oncancel });

    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;

    await fireEvent.mouseDown(overlay);
    await fireEvent.click(overlay);

    expect(oncancel).toHaveBeenCalledTimes(1);
  });
});

describe("ConfirmDialog: drag-out-of-dialog does not discard the confirmation (F3)", () => {
  const props = {
    title: "Delete clip",
    message: 'Delete "Greeting"? This cannot be undone.',
    confirmLabel: "Delete",
    onconfirm: vi.fn(),
  };

  it("mousedown starting inside the dialog, click landing on the overlay: oncancel is NOT called", async () => {
    const oncancel = vi.fn();
    const { container } = render(ConfirmDialog, { ...props, oncancel });

    const messageEl = screen.getByText(props.message);
    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;

    await fireEvent.mouseDown(messageEl);
    await fireEvent.click(overlay);

    expect(oncancel).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
  });

  it("a real click-outside (mousedown AND click both on the overlay) still dismisses", async () => {
    const oncancel = vi.fn();
    const { container } = render(ConfirmDialog, { ...props, oncancel });

    const overlay = container.querySelector('[role="presentation"]') as HTMLElement;

    await fireEvent.mouseDown(overlay);
    await fireEvent.click(overlay);

    expect(oncancel).toHaveBeenCalledTimes(1);
  });
});
