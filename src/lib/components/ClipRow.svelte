<script lang="ts">
  import type { Clip } from "../contract/types";
  import { COLOUR_META } from "../colour";
  import { searchDisabledReorderLabel, reorderLabel, editClipAriaLabel, deleteClipAriaLabel } from "../copy";

  let {
    clip,
    oncopy,
    onedit,
    ondelete,
    onmove,
    ondrop,
    dragDisabled = false,
  }: {
    clip: Clip;
    oncopy: (clip: Clip) => void;
    onedit: (clip: Clip) => void;
    ondelete: (clip: Clip) => void;
    /** Keyboard reorder: -1 moves the clip up one place, 1 moves it down. */
    onmove: (direction: -1 | 1) => void;
    /** Drag reorder: the id of the clip that was dropped on this row, and whether it landed above (true) or below (false) this row's midpoint. */
    ondrop: (sourceId: string, before: boolean) => void;
    /** WP-13: true whenever a search query is active. Spec §4.7: "Drag handles are inert whenever the query is non-empty; clearing it restores them." */
    dragDisabled?: boolean;
  } = $props();

  const tooltipId = $derived(`clip-value-${clip.id}`);

  // Drag reorder reads the source id from the native DragEvent rather than
  // from any component-local "which clip is being dragged" state — there is
  // nothing here that could disagree with `ClipList`'s computed permutation.
  function handleDragStart(event: DragEvent) {
    if (dragDisabled) return;
    // A private MIME type, not "text/plain": a plain-text payload is accepted
    // by any native drop target on the page, including the search `<input>`,
    // which would set its value to this clip's id and blank the list. Using
    // a type nothing else on the page registers for keeps the drag confined
    // to row-to-row reordering.
    event.dataTransfer?.setData("application/x-fastclip-clip-id", clip.id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function handleDragOver(event: DragEvent) {
    if (dragDisabled) return; // no preventDefault: this row refuses to become a drop target
    event.preventDefault(); // required for this element to become a drop target
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  }

  function handleDrop(event: DragEvent) {
    if (dragDisabled) return;
    event.preventDefault();
    const sourceId = event.dataTransfer?.getData("application/x-fastclip-clip-id");
    if (!sourceId) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const before = event.clientY < rect.top + rect.height / 2;
    ondrop(sourceId, before);
  }

  function handleHandleKeydown(event: KeyboardEvent) {
    if (dragDisabled) return;
    if (event.key === "ArrowUp") {
      event.preventDefault();
      onmove(-1);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      onmove(1);
    }
  }
</script>

<!--
  Rail layout (docs/src/product/palette.md "Chosen layout"), binding. Flat
  row, no fill, no border. A 3px rounded colour rail inset 4px at the left
  edge, decorative and non-interactive. Row background transparent at rest,
  `bg-row-hover` on hover. Edit/delete hidden until hover, right-aligned, on
  a scrim, 62px label clearance. Label truncates with CSS (`truncate`),
  never measured on a canvas.

  The drag handle is new surface for WP-06: a separate control to the left
  of the copy button (never nested inside it — two interactive elements
  cannot nest), draggable for the mouse and arrow-key-operable for the
  keyboard. Both paths call `onmove`/`ondrop`, which `ClipList` funnels into
  the same `onreorder` call with one full permutation.
-->
<!--
  The dragover/drop handlers below are the mouse-only half of reordering; the
  keyboard-operable half is the handle button's arrow-key handler above, so
  this row needs no role or keyboard handler of its own for this interaction.
-->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="group relative flex h-11 items-center hover:bg-row-hover"
  ondragover={handleDragOver}
  ondrop={handleDrop}
>
  <span
    aria-hidden="true"
    class="pointer-events-none absolute left-1 top-1 bottom-1 w-[3px] rounded-full {COLOUR_META[clip.colour].fillClass}"
  ></span>

  <button
    type="button"
    draggable={dragDisabled ? "false" : "true"}
    disabled={dragDisabled}
    ondragstart={handleDragStart}
    onkeydown={handleHandleKeydown}
    aria-label={dragDisabled ? searchDisabledReorderLabel(clip.label) : reorderLabel(clip.label)}
    class="relative z-10 flex h-full w-7 shrink-0 cursor-grab items-center justify-center text-zinc-500 hover:text-zinc-300 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400 active:cursor-grabbing disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:text-zinc-500"
  >
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
      <line x1="4" y1="8" x2="20" y2="8" stroke-linecap="round" />
      <line x1="4" y1="12" x2="20" y2="12" stroke-linecap="round" />
      <line x1="4" y1="16" x2="20" y2="16" stroke-linecap="round" />
    </svg>
  </button>

  <button
    type="button"
    class="relative h-full flex-1 rounded-none px-1 text-left focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
    aria-describedby={tooltipId}
    onclick={() => oncopy(clip)}
  >
    <span class="block truncate pr-[62px]">{clip.label}</span>
  </button>

  <div
    class="pointer-events-none absolute inset-y-0 right-0 flex items-center gap-0.5 bg-row-hover px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
  >
    <button
      type="button"
      class="pointer-events-auto flex h-7 w-7 items-center justify-center rounded text-zinc-300 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
      aria-label={editClipAriaLabel(clip.label)}
      onclick={() => onedit(clip)}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
        <path d="M4 20h4L18.5 9.5a2.1 2.1 0 0 0-3-3L5 17v3z" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
    <button
      type="button"
      class="pointer-events-auto flex h-7 w-7 items-center justify-center rounded text-zinc-300 hover:text-red-400 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
      aria-label={deleteClipAriaLabel(clip.label)}
      onclick={() => ondelete(clip)}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
        <path d="M5 7h14M10 11v6M14 11v6M7 7l1 13h8l1-13M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
  </div>

  <!--
    spec §4.1: "The full value is visible on hover." A custom tooltip rather
    than the native `title` attribute, so it is also reachable on keyboard
    focus (`group-focus-within`), not only mouse hover.
  -->
  <div
    role="tooltip"
    id={tooltipId}
    class="pointer-events-none absolute left-7 top-full z-10 mt-1 hidden max-w-[240px] whitespace-pre-wrap break-words rounded bg-zinc-800 px-2 py-1 text-xs text-zinc-100 shadow-lg group-hover:block group-focus-within:block"
  >
    {clip.value}
  </div>
</div>
