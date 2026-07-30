import type { Action } from "svelte/action";

// F3 (critic finding at WP-05's G3 review): a drag that starts inside the
// dialog (e.g. selecting text in the Value textarea) and releases over the
// overlay produces a `click` whose target is the overlay, because the DOM
// fires `click` on the nearest common ancestor of `mousedown` and `mouseup`.
// `event.target === event.currentTarget` on that `click` alone cannot tell
// the drag apart from a real click on the overlay — in both cases the
// click's target genuinely is the overlay. The gesture only counts as a
// dismiss when *both* the `mousedown` and the `click` land directly on the
// overlay element, so the `mousedown` origin has to be recorded too.
//
// This is the only place that condition is written. Every dialog overlay
// attaches it with `use:overlayDismiss={onDismiss}` instead of pairing its
// own `onmousedown`/`onclick` handlers.
export const overlayDismiss: Action<HTMLElement, () => void> = (node, onDismiss) => {
  let dismiss = onDismiss;
  let mouseDownOnOverlay = false;

  function handleMouseDown(event: MouseEvent) {
    mouseDownOnOverlay = event.target === node;
  }

  function handleClick(event: MouseEvent) {
    if (mouseDownOnOverlay && event.target === node) {
      dismiss();
    }
    mouseDownOnOverlay = false;
  }

  node.addEventListener("mousedown", handleMouseDown);
  node.addEventListener("click", handleClick);

  return {
    update(next) {
      dismiss = next;
    },
    destroy() {
      node.removeEventListener("mousedown", handleMouseDown);
      node.removeEventListener("click", handleClick);
    },
  };
};
