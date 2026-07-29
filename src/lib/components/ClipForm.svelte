<script lang="ts">
  import { untrack } from "svelte";
  import type { Clip, Colour } from "../contract/types";
  import { createClip, updateClip, CommandError } from "../ipc/commands";
  import { validateLabel, validateValue } from "../validation";
  import { invalidInputMessage, describeError } from "../errorMessage";
  import { DEFAULT_COLOUR } from "../colour";
  import ColourPicker from "./ColourPicker.svelte";

  let {
    clip = null,
    onsaved,
    oncancel,
  }: {
    /** `null` creates a new clip; a `Clip` edits it in place (spec §4.2). */
    clip?: Clip | null;
    onsaved: () => void;
    oncancel: () => void;
  } = $props();

  // `clip` only ever seeds this form's initial values: a new `ClipForm`
  // instance is created each time the dialog opens (App.svelte toggles
  // `formOpen`), so there is no later `clip` change for this form to react
  // to. `untrack` says that deliberately, rather than leaving the
  // "did you mean a derived?" warning unanswered.
  const isEdit = untrack(() => clip !== null);

  let label = $state(untrack(() => clip?.label ?? ""));
  let value = $state(untrack(() => clip?.value ?? ""));
  let colour = $state<Colour>(untrack(() => clip?.colour ?? DEFAULT_COLOUR));

  let labelTouched = $state(false);
  let valueTouched = $state(false);
  let submitAttempted = $state(false);
  let submitting = $state(false);
  let formError = $state<string | null>(null);

  const labelError = $derived(validateLabel(label));
  const valueError = $derived(validateValue(value));
  const showLabelError = $derived((labelTouched || submitAttempted) && labelError !== null);
  const showValueError = $derived((valueTouched || submitAttempted) && valueError !== null);

  let labelInput: HTMLInputElement | undefined;

  $effect(() => {
    labelInput?.focus();
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      oncancel();
    }
  }

  // Click-outside-to-dismiss must not fire from a text selection dragged out
  // of the Value textarea and released over the overlay: the DOM's `click`
  // event lands on the nearest common ancestor of `mousedown` and `mouseup`,
  // which is the overlay itself in that case, so `event.target ===
  // event.currentTarget` on `click` alone cannot tell the two apart. Instead
  // the gesture is only treated as a dismiss when *both* ends — the
  // `mousedown` and the resulting `click` — land directly on the overlay.
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

  async function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    submitAttempted = true;
    if (labelError !== null || valueError !== null) {
      return;
    }

    submitting = true;
    formError = null;
    try {
      if (isEdit && clip !== null) {
        await updateClip({ id: clip.id, label, value, colour });
      } else {
        await createClip({ label, value, colour });
      }
      onsaved();
    } catch (err) {
      if (err instanceof CommandError) {
        if (err.error.kind === "invalid_input") {
          if (err.error.field === "label") {
            labelTouched = true;
            formError = invalidInputMessage(err.error.reason);
          } else if (err.error.field === "value") {
            valueTouched = true;
            formError = invalidInputMessage(err.error.reason);
          } else {
            formError = invalidInputMessage(err.error.reason);
          }
        } else {
          formError = describeError(err.error);
        }
      } else {
        formError = "An unexpected error occurred.";
      }
    } finally {
      submitting = false;
    }
  }
</script>

<!--
  The overlay is a mouse-only "click outside to dismiss" convenience;
  `role="presentation"` says it carries no semantics of its own. Escape
  (handled on the dialog below) and the Cancel button are the keyboard
  equivalents, so the overlay itself needs no keyboard handler.
-->
<div
  role="presentation"
  class="fixed inset-0 z-20 flex items-end justify-center bg-black/50"
  onmousedown={handleOverlayMouseDown}
  onclick={handleOverlayClick}
>
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby="clip-form-title"
    tabindex="-1"
    class="max-h-[90%] w-full overflow-y-auto rounded-t-lg bg-zinc-800 p-4 text-zinc-100"
    onclick={(event) => event.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <h2 id="clip-form-title" class="mb-3 text-base font-semibold">
      {isEdit ? "Edit clip" : "New clip"}
    </h2>

    <form onsubmit={handleSubmit} novalidate>
      <div class="mb-3">
        <label for="clip-label" class="mb-1 block text-sm text-zinc-300">Label</label>
        <input
          id="clip-label"
          bind:this={labelInput}
          bind:value={label}
          onblur={() => (labelTouched = true)}
          type="text"
          class="w-full rounded border border-zinc-600 bg-zinc-900 px-2 py-1.5 text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
          aria-invalid={showLabelError}
          aria-describedby={showLabelError ? "clip-label-error" : undefined}
        />
        {#if showLabelError && labelError !== null}
          <p id="clip-label-error" class="mt-1 text-xs text-red-400">{invalidInputMessage(labelError)}</p>
        {/if}
      </div>

      <div class="mb-3">
        <label for="clip-value" class="mb-1 block text-sm text-zinc-300">Value</label>
        <textarea
          id="clip-value"
          bind:value
          onblur={() => (valueTouched = true)}
          rows="5"
          class="w-full resize-y rounded border border-zinc-600 bg-zinc-900 px-2 py-1.5 text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
          aria-invalid={showValueError}
          aria-describedby={showValueError ? "clip-value-error" : undefined}
        ></textarea>
        {#if showValueError && valueError !== null}
          <p id="clip-value-error" class="mt-1 text-xs text-red-400">{invalidInputMessage(valueError)}</p>
        {/if}
      </div>

      <div class="mb-4">
        <span class="mb-1 block text-sm text-zinc-300">Colour</span>
        <ColourPicker selected={colour} onchange={(next) => (colour = next)} />
      </div>

      {#if formError !== null}
        <p role="alert" class="mb-3 text-sm text-red-400">{formError}</p>
      {/if}

      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
          onclick={oncancel}
        >
          Cancel
        </button>
        <button
          type="submit"
          class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300 disabled:opacity-60"
          disabled={submitting}
        >
          {isEdit ? "Save" : "Create"}
        </button>
      </div>
    </form>
  </div>
</div>
