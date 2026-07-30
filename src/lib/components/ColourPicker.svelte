<script lang="ts">
  import { COLOUR_TOKENS, type Colour } from "../contract/types";
  import { COLOUR_META } from "../colour";

  let {
    selected,
    onchange,
    legend,
  }: {
    selected: Colour;
    onchange: (colour: Colour) => void;
    // No default: the caller's copy-deck constant is the one spelling
    // (docs/src/product/copy.md CLIP_COLOUR_FIELD_LABEL). A fallback string
    // here would be a second, silently-agreeing copy of it.
    legend: string;
  } = $props();
</script>

<!--
  WP-05 definition of done: "each swatch renders its token's foreground
  colour on its own fill, not `currentColor`." Every swatch below applies
  its own `fgClass` (e.g. `text-clip-red-fg`) directly, so the checkmark's
  `stroke="currentColor"` resolves to that token's own foreground value —
  never a colour inherited from something outside the swatch.

  Native radio inputs, visually hidden but focusable (`sr-only`), so Tab,
  arrow-key and Space/Enter selection all come from the browser's own radio
  group behaviour rather than a hand-rolled keyboard handler.
-->
<fieldset class="border-0 p-0 m-0">
  <legend class="sr-only">{legend}</legend>
  <div class="flex flex-wrap gap-2" role="radiogroup" aria-label={legend}>
    {#each COLOUR_TOKENS as token (token)}
      {@const meta = COLOUR_META[token]}
      <label class="relative block cursor-pointer">
        <input
          type="radio"
          name="clip-colour"
          value={token}
          class="peer sr-only"
          checked={selected === token}
          onchange={() => onchange(token)}
        />
        <span
          class="flex h-8 w-8 items-center justify-center rounded-full {meta.fillClass} {meta.fgClass} ring-2 ring-offset-2 ring-offset-zinc-800 ring-transparent peer-checked:ring-sky-400 peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-sky-400"
          aria-hidden="true"
        >
          {#if selected === token}
            <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2.2">
              <polyline points="5 13 10 18 19 7" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          {/if}
        </span>
        <span class="sr-only">{meta.label}</span>
      </label>
    {/each}
  </div>
</fieldset>
