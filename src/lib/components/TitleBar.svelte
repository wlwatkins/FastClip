<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const appWindow = getCurrentWindow();

  function minimise() {
    void appWindow.minimize();
  }

  function close() {
    void appWindow.close();
  }
</script>

<!--
  Window chrome. `decorations: false` (tauri.conf.json) removes every OS-level
  control, so the drag region and the minimise/close buttons below are the
  only way to move or close the window — baseline operability, not a spec
  feature. `core:window:allow-start-dragging`, `allow-minimize` and
  `allow-close` are granted in src-tauri/capabilities/default.json;
  always-on-top is deliberately not called here (WP-14 owns it, backend-side).
-->
<header
  data-tauri-drag-region
  class="flex h-8 w-full shrink-0 items-center justify-end bg-zinc-800"
>
  <button
    type="button"
    class="flex h-8 w-10 items-center justify-center text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
    aria-label="Minimise window"
    onclick={minimise}
  >
    <svg
      viewBox="0 0 24 24"
      width="15"
      height="15"
      fill="none"
      stroke="currentColor"
      stroke-width="1.7"
      aria-hidden="true"
    >
      <line x1="5" y1="12" x2="19" y2="12" stroke-linecap="round" />
    </svg>
  </button>
  <button
    type="button"
    class="flex h-8 w-10 items-center justify-center text-zinc-300 hover:bg-red-600 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
    aria-label="Close window"
    onclick={close}
  >
    <svg
      viewBox="0 0 24 24"
      width="15"
      height="15"
      fill="none"
      stroke="currentColor"
      stroke-width="1.7"
      aria-hidden="true"
    >
      <line x1="6" y1="6" x2="18" y2="18" stroke-linecap="round" />
      <line x1="18" y1="6" x2="6" y2="18" stroke-linecap="round" />
    </svg>
  </button>
</header>
