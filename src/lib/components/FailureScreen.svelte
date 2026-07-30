<script lang="ts">
  // contract §3 "Startup sequence": "The failure screen ... is one state: the
  // mapped sentence for the variant, plus the recovery path, and no clip
  // list. Every branch that reaches it stops there." Reached from three
  // places in App.svelte and rendered once, here.
  import { TRY_AGAIN_LABEL } from "../copy";

  let { message }: { message: string } = $props();

  function reload() {
    location.reload();
  }
</script>

<div role="alert" class="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
  <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="1.7" class="text-red-400" aria-hidden="true">
    <circle cx="12" cy="12" r="9.3" />
    <line x1="12" y1="7.5" x2="12" y2="13" stroke-linecap="round" />
    <circle cx="12" cy="16.5" r="0.4" fill="currentColor" stroke="none" />
  </svg>
  <p class="max-w-xs text-sm text-zinc-200">{message}</p>
  <button
    type="button"
    class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300"
    onclick={reload}
  >
    {TRY_AGAIN_LABEL}
  </button>
</div>
