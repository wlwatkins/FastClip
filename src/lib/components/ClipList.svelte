<script lang="ts">
  import type { Clip } from "../contract/types";
  import ClipRow from "./ClipRow.svelte";

  let {
    clips,
    oncopy,
    onedit,
    ondelete,
    onreorder,
    searchActive = false,
    emptyMessage,
  }: {
    clips: Clip[];
    oncopy: (clip: Clip) => void;
    onedit: (clip: Clip) => void;
    ondelete: (clip: Clip) => void;
    onreorder: (order: string[]) => void;
    /** WP-13: true whenever a search query is active. Drag is inert while true. */
    searchActive?: boolean;
    /** Shown in place of the list when `clips` is empty. Chosen by the caller so this component does not decide between "no clips at all" and "no clips match the search". */
    emptyMessage: string;
  } = $props();

  // Both the drag path and the keyboard path below compute a full
  // permutation from `clips` — the array the backend last sent — and hand it
  // to the same `onreorder` callback. Neither keeps an order of its own
  // (WP-06: "Do not maintain a local order that can disagree with the
  // backend's"); the next `update_clips` event is what actually moves the
  // rows, and a rejected reorder leaves this array, and therefore the
  // rendered order, exactly as it was.

  /** Keyboard path: move one clip up or down by one position. */
  function moveClip(id: string, direction: -1 | 1) {
    if (searchActive) return; // WP-13: reordering is inert while a search query is active
    const ids = clips.map((c) => c.id);
    const from = ids.indexOf(id);
    if (from === -1) return;
    const to = from + direction;
    if (to < 0 || to >= ids.length) return;
    ids.splice(from, 1);
    ids.splice(to, 0, id);
    onreorder(ids);
  }

  /** Drag path: drop `sourceId` immediately before or after `targetId`. */
  function handleDrop(sourceId: string, targetId: string, before: boolean) {
    if (searchActive) return; // WP-13: reordering is inert while a search query is active
    if (sourceId === targetId) return;
    const ids = clips.map((c) => c.id);
    const from = ids.indexOf(sourceId);
    if (from === -1) return;
    ids.splice(from, 1);
    let to = ids.indexOf(targetId);
    if (to === -1) return;
    if (!before) to += 1;
    ids.splice(to, 0, sourceId);
    onreorder(ids);
  }
</script>

{#if clips.length === 0}
  <p class="px-3 py-6 text-center text-sm text-zinc-400">{emptyMessage}</p>
{:else}
  <ul>
    {#each clips as clip (clip.id)}
      <li>
        <ClipRow
          {clip}
          {oncopy}
          {onedit}
          {ondelete}
          dragDisabled={searchActive}
          onmove={(direction) => moveClip(clip.id, direction)}
          ondrop={(sourceId, before) => handleDrop(sourceId, clip.id, before)}
        />
      </li>
    {/each}
  </ul>
{/if}
