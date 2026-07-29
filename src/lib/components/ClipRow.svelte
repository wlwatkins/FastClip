<script lang="ts">
  import type { Clip } from "../contract/types";
  import { COLOUR_META } from "../colour";

  let {
    clip,
    oncopy,
    onedit,
    ondelete,
  }: {
    clip: Clip;
    oncopy: (clip: Clip) => void;
    onedit: (clip: Clip) => void;
    ondelete: (clip: Clip) => void;
  } = $props();

  const tooltipId = $derived(`clip-value-${clip.id}`);
</script>

<!--
  Rail layout (docs/src/product/palette.md "Chosen layout"), binding.
  Flat row, no fill, no border. A 3px rounded colour rail inset 4px at the
  left edge. Row background transparent at rest, `bg-row-hover` on hover.
  Edit/delete hidden until hover, right-aligned, on a scrim, 62px label
  clearance. Label truncates with CSS (`truncate`), never measured on a
  canvas.
-->
<div class="group relative flex h-11 items-center hover:bg-row-hover">
  <button
    type="button"
    class="relative h-full w-full rounded-none px-3 text-left focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
    aria-describedby={tooltipId}
    onclick={() => oncopy(clip)}
  >
    <span aria-hidden="true" class="absolute left-1 top-1 bottom-1 w-[3px] rounded-full {COLOUR_META[clip.colour].fillClass}"></span>
    <span class="block truncate pl-3 pr-[62px]">{clip.label}</span>
  </button>

  <div
    class="pointer-events-none absolute inset-y-0 right-0 flex items-center gap-0.5 bg-row-hover px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
  >
    <button
      type="button"
      class="pointer-events-auto flex h-7 w-7 items-center justify-center rounded text-zinc-300 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
      aria-label={`Edit ${clip.label}`}
      onclick={() => onedit(clip)}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
        <path d="M4 20h4L18.5 9.5a2.1 2.1 0 0 0-3-3L5 17v3z" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
    <button
      type="button"
      class="pointer-events-auto flex h-7 w-7 items-center justify-center rounded text-zinc-300 hover:text-red-400 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
      aria-label={`Delete ${clip.label}`}
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
    class="pointer-events-none absolute left-3 top-full z-10 mt-1 hidden max-w-[240px] whitespace-pre-wrap break-words rounded bg-zinc-800 px-2 py-1 text-xs text-zinc-100 shadow-lg group-hover:block group-focus-within:block"
  >
    {clip.value}
  </div>
</div>
