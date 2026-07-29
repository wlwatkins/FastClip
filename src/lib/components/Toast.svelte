<script lang="ts">
  import { toastState } from "../state/toast.svelte";
</script>

<!--
  spec §4.1: "clear, non-blocking confirmation" that a copy happened.
  `role="status"` + `aria-live="polite"` announce it to assistive tech
  without stealing focus; the element stays in the DOM so the live region
  is established before the first message arrives.
-->
<div
  role="status"
  aria-live="polite"
  class="pointer-events-none fixed inset-x-0 bottom-3 z-40 flex justify-center"
>
  {#if toastState.message !== null}
    <span class="rounded bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 shadow-lg">
      {toastState.message}
    </span>
  {/if}
</div>
