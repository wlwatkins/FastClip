<script lang="ts">
  let {
    title,
    message,
    confirmLabel,
    danger = false,
    busy = false,
    onconfirm,
    oncancel,
  }: {
    title: string;
    message: string;
    confirmLabel: string;
    danger?: boolean;
    busy?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();

  let cancelButton: HTMLButtonElement | undefined;

  $effect(() => {
    // Delete is irreversible and sits beside a control the user clicks all
    // day (spec §4.2), so the safer default gets initial focus rather than
    // the destructive action.
    cancelButton?.focus();
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      oncancel();
    }
  }

  // Same drag-out-of-dialog guard as ClipForm.svelte: a `click` synthesised
  // from a `mousedown` inside the dialog and a `mouseup` over the overlay
  // targets the overlay either way, so both ends of the gesture must land on
  // the overlay before it counts as a dismiss.
  let overlayMouseDown = false;

  function handleOverlayMouseDown(event: MouseEvent) {
    overlayMouseDown = event.target === event.currentTarget;
  }

  function handleOverlayClick(event: MouseEvent) {
    if (overlayMouseDown && event.target === event.currentTarget) {
      oncancel();
    }
    overlayMouseDown = false;
  }
</script>

<!-- The overlay is a mouse-only dismiss convenience; see ClipForm.svelte for the same pattern. -->
<div
  role="presentation"
  class="fixed inset-0 z-30 flex items-center justify-center bg-black/50"
  onmousedown={handleOverlayMouseDown}
  onclick={handleOverlayClick}
>
  <div
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-dialog-title"
    aria-describedby="confirm-dialog-message"
    tabindex="-1"
    class="w-[min(90%,20rem)] rounded-lg bg-zinc-800 p-4 text-zinc-100"
    onclick={(event) => event.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <h2 id="confirm-dialog-title" class="mb-2 text-base font-semibold">{title}</h2>
    <p id="confirm-dialog-message" class="mb-4 text-sm text-zinc-300">{message}</p>
    <div class="flex justify-end gap-2">
      <button
        bind:this={cancelButton}
        type="button"
        class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
        onclick={oncancel}
      >
        Cancel
      </button>
      <button
        type="button"
        class="rounded px-3 py-1.5 text-sm font-medium text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 disabled:opacity-60 {danger
          ? 'bg-red-600 hover:bg-red-500 focus-visible:outline-red-300'
          : 'bg-sky-600 hover:bg-sky-500 focus-visible:outline-sky-300'}"
        disabled={busy}
        onclick={onconfirm}
      >
        {confirmLabel}
      </button>
    </div>
  </div>
</div>
