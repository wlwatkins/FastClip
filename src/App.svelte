<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import ClipList from "./lib/components/ClipList.svelte";
  import ClipForm from "./lib/components/ClipForm.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import { clipListState, setClips } from "./lib/state/clips.svelte";
  import { showToast } from "./lib/state/toast.svelte";
  import { listClips, copyClip, deleteClip, CommandError } from "./lib/ipc/commands";
  import { parseClipList } from "./lib/contract/validate";
  import { describeError } from "./lib/errorMessage";
  import type { Clip } from "./lib/contract/types";

  let loadError = $state<string | null>(null);
  let formOpen = $state(false);
  let formClip = $state<Clip | null>(null); // null = create, otherwise the clip being edited
  let deletingClip = $state<Clip | null>(null);
  let deleteBusy = $state(false);

  const dialogOpen = $derived(formOpen || deletingClip !== null);

  // Contract §3: "The frontend registers both listeners before its first
  // `invoke`" and "an emit is never silently dropped" — the listener below
  // is wired before the initial `listClips()` call so an `update_clips`
  // that lands during that first request is not lost.
  $effect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    (async () => {
      unlisten = await listen<unknown>("update_clips", (event) => {
        setClips(parseClipList(event.payload, "update_clips"));
      });
      if (cancelled) {
        unlisten();
        return;
      }
      try {
        setClips(await listClips());
      } catch (err) {
        loadError = err instanceof CommandError ? describeError(err.error) : "An unexpected error occurred.";
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  function openCreate() {
    formClip = null;
    formOpen = true;
  }

  function openEdit(clip: Clip) {
    formClip = clip;
    formOpen = true;
  }

  function closeForm() {
    formOpen = false;
    formClip = null;
  }

  function requestDelete(clip: Clip) {
    deletingClip = clip;
  }

  function cancelDelete() {
    deletingClip = null;
  }

  // contract §4, `not_found`: "No event follows a failed command, so the
  // frontend calls list_clips to resynchronise." Used by both copy and
  // delete below, since both take a `clip_id` that can go stale between the
  // frontend's last list and the click.
  async function resync() {
    try {
      setClips(await listClips());
    } catch {
      // The resync itself failing is not this call's problem to report; the
      // toast already named the original failure.
    }
  }

  async function confirmDelete() {
    if (deletingClip === null) return;
    deleteBusy = true;
    try {
      await deleteClip(deletingClip.id);
      deletingClip = null;
    } catch (err) {
      deletingClip = null;
      if (err instanceof CommandError) {
        showToast(describeError(err.error));
        if (err.error.kind === "not_found") await resync();
      } else {
        showToast("An unexpected error occurred.");
      }
    } finally {
      deleteBusy = false;
    }
  }

  async function handleCopy(clip: Clip) {
    try {
      await copyClip(clip.id);
      showToast(`Copied "${clip.label}"`);
    } catch (err) {
      if (err instanceof CommandError) {
        showToast(describeError(err.error));
        if (err.error.kind === "not_found") await resync();
      } else {
        showToast("An unexpected error occurred.");
      }
    }
  }
</script>

<div class="flex h-screen w-screen min-w-[250px] flex-col overflow-hidden bg-zinc-900 text-zinc-100">
  <div inert={dialogOpen} class="flex min-h-0 flex-1 flex-col">
    <TitleBar />

    <div class="flex shrink-0 items-center justify-end border-b border-zinc-800 px-2 py-1">
      <button
        type="button"
        class="flex items-center gap-1 rounded px-2 py-1 text-sm text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
        aria-label="New clip"
        onclick={openCreate}
      >
        <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
          <line x1="12" y1="5" x2="12" y2="19" stroke-linecap="round" />
          <line x1="5" y1="12" x2="19" y2="12" stroke-linecap="round" />
        </svg>
        New clip
      </button>
    </div>

    <main class="flex-1 overflow-y-auto" aria-label="Clips">
      {#if loadError !== null}
        <p role="alert" class="px-3 py-3 text-sm text-red-400">{loadError}</p>
      {:else}
        <ClipList clips={clipListState.clips} oncopy={handleCopy} onedit={openEdit} ondelete={requestDelete} />
      {/if}
    </main>
  </div>

  {#if formOpen}
    <ClipForm clip={formClip} onsaved={closeForm} oncancel={closeForm} />
  {/if}

  {#if deletingClip !== null}
    <ConfirmDialog
      title="Delete clip"
      message={`Delete "${deletingClip.label}"? This cannot be undone.`}
      confirmLabel="Delete"
      danger
      busy={deleteBusy}
      onconfirm={confirmDelete}
      oncancel={cancelDelete}
    />
  {/if}

  <Toast />
</div>
