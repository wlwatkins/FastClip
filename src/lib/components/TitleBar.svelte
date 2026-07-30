<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { SETTINGS_BUTTON_LABEL, MINIMISE_WINDOW_LABEL, CLOSE_WINDOW_LABEL } from "../copy";

  let { onopensettings }: { onopensettings: () => void } = $props();

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
  always-on-top is deliberately not called here — the backend both persists
  and applies it (contract §2 `set_always_on_top`), and the capability set
  does not grant `core:window:set_always_on_top`.

  The settings button sits here, outside the phase-dependent body, because
  `get_settings`/`set_always_on_top` work while the store is locked or
  unreachable (contract §2) and so must stay reachable in every startup phase.
-->
<header
  data-tauri-drag-region
  class="flex h-8 w-full shrink-0 items-center justify-end bg-zinc-800"
>
  <button
    type="button"
    class="flex h-8 w-10 items-center justify-center text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
    aria-label={SETTINGS_BUTTON_LABEL}
    onclick={onopensettings}
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
      <circle cx="12" cy="12" r="3" />
      <path
        d="M19.4 13.5a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.04 1.56V19.7a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.56-1.04H2.3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.56-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H8.3a1.7 1.7 0 0 0 1.04-1.56V2.3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.04 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V8.3a1.7 1.7 0 0 0 1.56 1.04h.09a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.04Z"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </button>
  <button
    type="button"
    class="flex h-8 w-10 items-center justify-center text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
    aria-label={MINIMISE_WINDOW_LABEL}
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
    aria-label={CLOSE_WINDOW_LABEL}
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
