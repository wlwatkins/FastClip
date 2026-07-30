<script lang="ts">
  import { tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import ClipList from "./lib/components/ClipList.svelte";
  import ClipForm from "./lib/components/ClipForm.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import SettingsPanel from "./lib/components/SettingsPanel.svelte";
  import FailureScreen from "./lib/components/FailureScreen.svelte";
  import PinPrompt from "./lib/components/PinPrompt.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import { clipListState, setClips } from "./lib/state/clips.svelte";
  import { showToast } from "./lib/state/toast.svelte";
  import { setAlwaysOnTopState } from "./lib/state/settings.svelte";
  import { lockState, setLockState } from "./lib/state/lockState.svelte";
  import {
    listClips,
    copyClip,
    deleteClip,
    reorderClips,
    getSettings,
    getLockState,
    CommandError,
  } from "./lib/ipc/commands";
  import { parseClipList, parseLockState } from "./lib/contract/validate";
  import { describeError } from "./lib/errorMessage";
  import { filterClips } from "./lib/search";
  import {
    SEARCH_TOGGLE_LABEL,
    SEARCH_CLOSE_LABEL,
    SEARCH_PLACEHOLDER,
    SEARCH_EMPTY_RESULT_MESSAGE,
    NO_CLIPS_MESSAGE,
    NEW_CLIP_BUTTON_LABEL,
    LOADING_MESSAGE,
    UNEXPECTED_ERROR_MESSAGE,
    DELETE_CLIP_TITLE,
    DELETE_CLIP_CONFIRM_LABEL,
    CLIP_LIST_LANDMARK_LABEL,
    deleteClipMessage,
    copiedMessage,
  } from "./lib/copy";
  import type { Clip, LockState } from "./lib/contract/types";

  // contract §3 "Startup sequence": one of these four, in this order, and
  // once "failure" or "locked" is reached the frontend stops there.
  //
  // "locked" is reached at launch (step 5) and, live, whenever
  // `lockState.locked` becomes true from any source — a manual `lock` chief
  // among them (ADR-0010). The `$effect` below is what makes the second case
  // load-bearing rather than insurance: it is the one place that discards
  // the clip list, closes any open form and clears the search query,
  // regardless of which call site observed `locked` first.
  let phase = $state<"loading" | "ready" | "locked" | "failure">("loading");
  let failureMessage = $state("");

  let formOpen = $state(false);
  let formClip = $state<Clip | null>(null); // null = create, otherwise the clip being edited
  let deletingClip = $state<Clip | null>(null);
  let deleteBusy = $state(false);
  let settingsOpen = $state(false);

  let pinPromptRef: PinPrompt | undefined = $state();

  const dialogOpen = $derived(formOpen || deletingClip !== null || settingsOpen);

  // WP-13 search (spec §4.7): hidden by default, revealed and focused by the
  // toolbar button or Ctrl+F, cleared and hidden by Escape. Filtering is
  // client-side over `clipListState.clips`, which is already in memory —
  // there is no search command (contract §2). `searchQuery` is never logged,
  // put in a toast or passed to `errorMessage.ts`: it is a substring of a
  // clip's `value`, which ADR-0002 keeps out of logs.
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let searchToggleEl = $state<HTMLButtonElement | null>(null);

  const searchActive = $derived(searchQuery !== "");
  const filteredClips = $derived(filterClips(clipListState.clips, searchQuery));
  const emptyMessage = $derived(searchActive ? SEARCH_EMPTY_RESULT_MESSAGE : NO_CLIPS_MESSAGE);

  async function openSearch() {
    searchOpen = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    searchQuery = "";
  }

  function handleSearchToggleClick() {
    if (searchOpen) {
      closeSearch();
    } else {
      void openSearch();
    }
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
      searchToggleEl?.focus();
    }
  }

  // contract §3 `unlock_requested` / ADR-0013: the tray's *Unlock FastClip*
  // item raises the window and emits this event afterward. It carries no
  // data — everything this handler needs is already in `lockState`. Gated
  // on the last received `lock_state`, the same discard rule `update_clips`
  // already carries (contract §3 `unlock_requested`, "Receiver's gate"):
  // the tray is rebuilt on every lock-state change, so an event fired over
  // a store that has since unlocked must not pull focus into a window the
  // user is already working in.
  //
  // Owner decision pending (review 012 F2 path 2, ADR-0013 "Consequences"):
  // whether *Unlock FastClip* should also dismiss an open settings panel.
  // Left unhandled here. With the panel open, the `inert` subtree below
  // still covers `PinPrompt`, so `focusPin()` has nothing focusable to
  // reach — that is not a bug and not a limitation of this change, it is
  // the open question, undecided.
  function handleUnlockRequested(): void {
    if (!lockState.locked) return;
    pinPromptRef?.focusPin();
  }

  function goToFailure(error: unknown) {
    phase = "failure";
    // `describeError` returns `null` only for `locked`, which never reaches
    // the failure screen (the `lock_state` listener and the startup sequence
    // both route it to the "locked" phase instead); the fallback below is
    // defensive, not an expected path.
    failureMessage =
      error instanceof CommandError ? (describeError(error.error) ?? UNEXPECTED_ERROR_MESSAGE) : UNEXPECTED_ERROR_MESSAGE;
  }

  // Contract §3: "The frontend registers all three listeners before its
  // first `invoke`" and "an emit is never silently dropped" — all three
  // listeners below are wired before the sequence's first call, so an event
  // that lands during the sequence is not lost. `unlock_requested` in
  // particular "can arrive during the sequence, because a store that
  // launches locked has a tray showing the *Unlock FastClip* item from the
  // moment the tray is installed" (contract §3 "Startup sequence", step 1).
  $effect(() => {
    let cancelled = false;
    let unlistenClips: (() => void) | undefined;
    let unlistenLockState: (() => void) | undefined;
    let unlistenUnlockRequested: (() => void) | undefined;

    (async () => {
      unlistenClips = await listen<unknown>("update_clips", (event) => {
        // contract §3 `update_clips`: discard a payload that arrives while
        // the last received `lock_state` says the store is locked, so it
        // cannot repaint the clip list underneath the PIN prompt.
        if (lockState.locked) return;
        setClips(parseClipList(event.payload, "update_clips"));
      });
      unlistenLockState = await listen<unknown>("lock_state", (event) => {
        setLockState(parseLockState(event.payload, "lock_state"));
      });
      unlistenUnlockRequested = await listen<unknown>("unlock_requested", () => {
        handleUnlockRequested();
      });
      if (cancelled) {
        unlistenClips();
        unlistenLockState();
        unlistenUnlockRequested();
        return;
      }

      // Step 2: get_settings. Its only declared failure is `storage`, which
      // this step does not report — step 3 (get_lock_state) reports the same
      // fault through the failure screen ("One failure surface, not two.").
      let alwaysOnTop = false;
      try {
        alwaysOnTop = (await getSettings()).always_on_top;
      } catch {
        // fall through with the documented default
      }
      setAlwaysOnTopState(alwaysOnTop);

      // Step 3: get_lock_state.
      let lock: LockState;
      try {
        lock = await getLockState();
      } catch (err) {
        goToFailure(err);
        return;
      }
      setLockState(lock);

      if (lock.locked) {
        // The `$effect` above reacts to `lockState.locked` and sets
        // `phase = "locked"`; this `return` only needs to skip `list_clips`.
        return;
      }

      // Step 4: locked: false → list_clips, render.
      try {
        setClips(await listClips());
        phase = "ready";
      } catch (err) {
        // A concurrent `lock` between step 3 and here is a genuine race
        // (contract §4 "When `locked` is reachable"): the `lock_state`
        // listener above has already or will shortly record it, so this
        // becomes the locked phase rather than the failure screen. Every
        // other declared rejection here — `storage`, `crypto`,
        // `unsupported_version` — is the failure screen; `list_clips` must
        // not render an empty list in their place.
        if (err instanceof CommandError && err.error.kind === "locked") {
          // `lockState.locked` is already true by construction of this
          // error (contract §4 "locked": "Encryption is on and the store is
          // not open"), so the `$effect` above has already moved `phase` to
          // "locked" or is about to.
        } else {
          goToFailure(err);
        }
      }
    })();

    return () => {
      cancelled = true;
      unlistenClips?.();
      unlistenLockState?.();
      unlistenUnlockRequested?.();
    };
  });

  // WP-13: `Ctrl+F` reveals and focuses search from anywhere in the window.
  // Suppressed while a dialog is open (search sits underneath `inert`, but a
  // window-level listener bypasses `inert`) and outside the "ready" phase,
  // where there is no clip list to search.
  $effect(() => {
    function handleGlobalKeydown(event: KeyboardEvent) {
      if (event.ctrlKey && !event.metaKey && !event.altKey && event.key.toLowerCase() === "f") {
        if (phase !== "ready" || dialogOpen) return;
        event.preventDefault();
        void openSearch();
      }
    }
    window.addEventListener("keydown", handleGlobalKeydown);
    return () => window.removeEventListener("keydown", handleGlobalKeydown);
  });

  // The load-bearing effect ADR-0010 requires: whenever `lockState.locked`
  // is true, for any reason, discard the in-memory clip list, close any open
  // create or edit form and the delete confirmation (both hold a clip's
  // content), and clear the search query — then show the PIN prompt. This
  // fires for the startup "locked" branch below and for a manual `lock`
  // arriving mid-session alike, so no call site needs to repeat it. Running
  // again while already locked is harmless: the list is already empty and
  // the forms are already closed.
  $effect(() => {
    if (!lockState.locked) return;
    setClips([]);
    formOpen = false;
    formClip = null;
    deletingClip = null;
    closeSearch();
    if (phase === "ready" || phase === "loading") phase = "locked";
  });

  // The way back. `unlock` (launch or after a manual `lock`) emits
  // `lock_state` then `update_clips` on success (contract §2 `unlock`); the
  // frontend does not call `list_clips` itself. This effect is what returns
  // `phase` to "ready" once the store reports itself unlocked, for both
  // origins alike.
  $effect(() => {
    if (phase === "locked" && !lockState.locked) {
      phase = "ready";
    }
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
        // `locked` (a manual lock landing mid-request) shows no toast: the
        // `lock_state` listener has already moved `phase` to "locked" and
        // the PIN prompt is the answer (copy.md "Failures" `locked` row).
        const message = describeError(err.error);
        if (message !== null) showToast(message);
        if (err.error.kind === "not_found") await resync();
      } else {
        showToast(UNEXPECTED_ERROR_MESSAGE);
      }
    } finally {
      deleteBusy = false;
    }
  }

  async function handleCopy(clip: Clip) {
    try {
      await copyClip(clip.id);
      showToast(copiedMessage(clip.label));
    } catch (err) {
      if (err instanceof CommandError) {
        // Same `locked` exception as confirmDelete above.
        const message = describeError(err.error);
        if (message !== null) showToast(message);
        if (err.error.kind === "not_found") await resync();
      } else {
        showToast(UNEXPECTED_ERROR_MESSAGE);
      }
    }
  }

  // contract `reorder_clips` / ADR-0007: `order` is the complete permutation,
  // computed by `ClipList` from the array this component already holds. On
  // success the authoritative order arrives on the next `update_clips` event.
  // On `not_a_permutation` the drag is discarded and the frontend resyncs to
  // the backend's current list rather than retrying with the same order.
  async function handleReorder(order: string[]) {
    try {
      await reorderClips(order);
    } catch (err) {
      if (err instanceof CommandError) {
        // Same `locked` exception as confirmDelete above.
        const message = describeError(err.error);
        if (message !== null) showToast(message);
        if (err.error.kind === "invalid_input" && err.error.reason === "not_a_permutation") {
          await resync();
        }
      } else {
        showToast(UNEXPECTED_ERROR_MESSAGE);
      }
    }
  }
</script>

<div class="flex h-screen w-screen min-w-[250px] flex-col overflow-hidden bg-zinc-900 text-zinc-100">
  <div inert={dialogOpen} class="flex min-h-0 flex-1 flex-col">
    <TitleBar onopensettings={() => (settingsOpen = true)} />

    {#if phase === "failure"}
      <FailureScreen message={failureMessage} />
    {:else if phase === "locked"}
      <PinPrompt bind:this={pinPromptRef} onfatal={goToFailure} />
    {:else}
      <div class="flex shrink-0 items-center justify-between gap-1 border-b border-zinc-800 px-2 py-1">
        <button
          type="button"
          bind:this={searchToggleEl}
          class="flex items-center gap-1 rounded px-2 py-1 text-sm text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
          aria-label={SEARCH_TOGGLE_LABEL}
          aria-pressed={searchOpen}
          onclick={handleSearchToggleClick}
        >
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
            <circle cx="10.5" cy="10.5" r="6.5" />
            <line x1="20" y1="20" x2="15.3" y2="15.3" stroke-linecap="round" />
          </svg>
        </button>
        <button
          type="button"
          class="flex items-center gap-1 rounded px-2 py-1 text-sm text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
          aria-label={NEW_CLIP_BUTTON_LABEL}
          onclick={openCreate}
        >
          <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
            <line x1="12" y1="5" x2="12" y2="19" stroke-linecap="round" />
            <line x1="5" y1="12" x2="19" y2="12" stroke-linecap="round" />
          </svg>
          {NEW_CLIP_BUTTON_LABEL}
        </button>
      </div>

      {#if searchOpen}
        <div class="flex shrink-0 items-center gap-1 border-b border-zinc-800 px-2 py-1">
          <input
            bind:this={searchInputEl}
            bind:value={searchQuery}
            type="text"
            role="searchbox"
            aria-label={SEARCH_TOGGLE_LABEL}
            placeholder={SEARCH_PLACEHOLDER}
            onkeydown={handleSearchKeydown}
            class="min-w-0 flex-1 rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100 placeholder-zinc-500 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
          />
          <button
            type="button"
            class="flex h-7 w-7 shrink-0 items-center justify-center rounded text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-sky-400"
            aria-label={SEARCH_CLOSE_LABEL}
            onclick={() => {
              closeSearch();
              searchToggleEl?.focus();
            }}
          >
            <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
              <line x1="6" y1="6" x2="18" y2="18" stroke-linecap="round" />
              <line x1="18" y1="6" x2="6" y2="18" stroke-linecap="round" />
            </svg>
          </button>
        </div>
      {/if}

      <main class="flex-1 overflow-y-auto" aria-label={CLIP_LIST_LANDMARK_LABEL}>
        {#if phase === "loading"}
          <p aria-busy="true" class="px-3 py-6 text-center text-sm text-zinc-400">{LOADING_MESSAGE}</p>
        {:else}
          <ClipList
            clips={filteredClips}
            oncopy={handleCopy}
            onedit={openEdit}
            ondelete={requestDelete}
            onreorder={handleReorder}
            {searchActive}
            {emptyMessage}
          />
        {/if}
      </main>
    {/if}
  </div>

  {#if formOpen}
    <ClipForm clip={formClip} onsaved={closeForm} oncancel={closeForm} />
  {/if}

  {#if deletingClip !== null}
    <ConfirmDialog
      title={DELETE_CLIP_TITLE}
      message={deleteClipMessage(deletingClip.label)}
      confirmLabel={DELETE_CLIP_CONFIRM_LABEL}
      danger
      busy={deleteBusy}
      onconfirm={confirmDelete}
      oncancel={cancelDelete}
    />
  {/if}

  {#if settingsOpen}
    <SettingsPanel onclose={() => (settingsOpen = false)} />
  {/if}

  <Toast />
</div>
