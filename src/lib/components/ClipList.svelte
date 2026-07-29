<script lang="ts">
  import type { Clip } from "../contract/types";
  import ClipRow from "./ClipRow.svelte";

  let {
    clips,
    oncopy,
    onedit,
    ondelete,
  }: {
    clips: Clip[];
    oncopy: (clip: Clip) => void;
    onedit: (clip: Clip) => void;
    ondelete: (clip: Clip) => void;
  } = $props();
</script>

{#if clips.length === 0}
  <p class="px-3 py-6 text-center text-sm text-zinc-400">No clips yet.</p>
{:else}
  <ul>
    {#each clips as clip (clip.id)}
      <li>
        <ClipRow {clip} {oncopy} {onedit} {ondelete} />
      </li>
    {/each}
  </ul>
{/if}
