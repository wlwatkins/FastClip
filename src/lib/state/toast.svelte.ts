// Non-blocking copy confirmation (spec §4.1: "the user receives clear,
// non-blocking confirmation"). One message at a time, auto-dismissed.
// Module-level because the toast is triggered from ClipRow and rendered
// from App.svelte, which are not in an ancestor/descendant relationship
// with each other's props.

export const toastState: { message: string | null } = $state({ message: null });

let dismissTimer: ReturnType<typeof setTimeout> | undefined;

export function showToast(message: string, durationMs = 2000): void {
  if (dismissTimer !== undefined) {
    clearTimeout(dismissTimer);
  }
  toastState.message = message;
  dismissTimer = setTimeout(() => {
    toastState.message = null;
    dismissTimer = undefined;
  }, durationMs);
}
