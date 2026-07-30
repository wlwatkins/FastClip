<script lang="ts">
  // The launch PIN prompt (WP-07, spec §4.8): "Nothing is listed, searched or
  // copied until it is entered." Rendered in place of the clip list whenever
  // `phase === "locked"` in App.svelte — at launch and after a manual
  // `lock`, on the identical path (contract §2 `unlock`).
  //
  // The PIN is never logged: not to console, not into a toast, not through
  // errorMessage.ts's generic path for an unmodelled error.

  import { unlock, CommandError } from "../ipc/commands";
  import { lockState } from "../state/lockState.svelte";
  import PinInput from "./PinInput.svelte";
  import {
    UNLOCK_PROMPT_TITLE,
    UNLOCK_PIN_LABEL,
    UNLOCK_BUTTON_LABEL,
    UNLOCK_LANDMARK_LABEL,
    UNLOCKING_LABEL,
    WRONG_PIN_MESSAGE,
    UNEXPECTED_ERROR_MESSAGE,
    attemptsRemainingMessage,
    backoffMessage,
  } from "../copy";
  import { invalidInputMessage, describeError } from "../errorMessage";

  // contract §3 "Startup sequence", the `unlock` branch: "Rejects with
  // `crypto` or `unsupported_version` → failure screen. The PIN was correct
  // and the store still could not be opened, so leaving the user on the PIN
  // prompt would tell them to try a PIN that already worked." `phase` is
  // owned by App.svelte, so this is the caller's escape hatch rather than a
  // local error string.
  let { onfatal }: { onfatal: (error: unknown) => void } = $props();

  let pin = $state("");
  let submitting = $state(false);
  let formError = $state<string | null>(null);

  // Seeded from the module's current lock state — already populated by
  // `get_lock_state` at launch or by the `lock_state` event on a manual lock
  // — rather than reset to null, because attempts and backoff are persisted
  // server-side (contract §1 `LockState`) and this component remounts fresh
  // every time `phase` becomes "locked".
  let attemptsRemaining = $state(lockState.attempts_remaining);
  let retryAfterMs = $state(lockState.retry_after_ms);

  let pinInputRef: PinInput | undefined = $state();

  // Set true by `focusPin` below, cleared once the reassert has fired.
  // Isolated to its own effect so the listener is attached and removed by
  // one place, with a cleanup for every subscription (no leaked listener if
  // this component unmounts — a successful unlock, for instance — before
  // the window's own "focus" event arrives).
  let awaitingFocusReassert = $state(false);

  $effect(() => {
    if (!awaitingFocusReassert) return;
    function reassert() {
      pinInputRef?.focus();
      awaitingFocusReassert = false;
    }
    window.addEventListener("focus", reassert);
    return () => window.removeEventListener("focus", reassert);
  });

  /**
   * Called from App.svelte's `handleUnlockRequested` (contract §3
   * `unlock_requested`, ADR-0013). `PinInput`'s `autofocus` effect only runs
   * at mount, so if the window was minimised — moving focus to the title
   * bar's minimise button, the only way to minimise with `decorations:
   * false` — and the tray then raises it, nothing re-runs that effect. This
   * is the explicit refocus for that case.
   *
   * The window is being unminimised and brought forward as the event lands,
   * and WebView2 may restore focus to whatever held it before the minimise
   * at the moment the window itself regains native focus — which can arrive
   * either before or after this call runs (review 012 F2 path 1; the
   * ordering is a prediction the critic could not run, not a confirmed
   * fact). A single `.focus()` call is therefore not trusted alone: this
   * also arms a one-shot listener for the window's own "focus" event, which
   * reasserts the same focus if something moved it in between.
   */
  export function focusPin(): void {
    pinInputRef?.focus();
    awaitingFocusReassert = true;
  }

  const backingOff = $derived(retryAfterMs !== null && retryAfterMs > 0);
  const secondsRemaining = $derived(retryAfterMs !== null ? Math.ceil(retryAfterMs / 1000) : 0);

  // Local countdown from the snapshot the backend sent (contract §1
  // `LockState`: "The frontend counts down locally from receipt and
  // re-enables the PIN input at zero. The backend re-checks on the next
  // `unlock` call and is authoritative.").
  $effect(() => {
    if (retryAfterMs === null || retryAfterMs <= 0) return;
    const interval = setInterval(() => {
      retryAfterMs = retryAfterMs === null ? null : Math.max(0, retryAfterMs - 250);
    }, 250);
    return () => clearInterval(interval);
  });

  async function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    if (submitting || backingOff || pin.length !== 6) return;
    submitting = true;
    formError = null;
    try {
      await unlock(pin);
      pin = "";
      // Success: `lock_state` then `update_clips` arrive on the event
      // listeners in App.svelte, which move `phase` out of "locked".
    } catch (err) {
      pin = "";
      if (err instanceof CommandError) {
        const e = err.error;
        if (e.kind === "bad_pin") {
          attemptsRemaining = e.attempts_remaining;
          retryAfterMs = e.retry_after_ms;
          // copy.md "Wrong PIN": the evaluated failure gets an explicit
          // acknowledgement, not only a decrementing count.
          formError = WRONG_PIN_MESSAGE;
        } else if (e.kind === "backoff") {
          retryAfterMs = e.retry_after_ms;
        } else if (e.kind === "invalid_input") {
          formError = invalidInputMessage(e.reason);
        } else if (e.kind === "crypto" || e.kind === "unsupported_version") {
          onfatal(err);
        } else {
          formError = describeError(e);
        }
      } else {
        formError = UNEXPECTED_ERROR_MESSAGE;
      }
    } finally {
      submitting = false;
    }
  }
</script>

<main class="flex flex-1 flex-col items-center justify-center gap-4 px-6 text-center" aria-label={UNLOCK_LANDMARK_LABEL}>
  <h1 class="text-base font-semibold text-zinc-100">{UNLOCK_PROMPT_TITLE}</h1>

  <form onsubmit={handleSubmit} novalidate class="flex w-full max-w-[14rem] flex-col gap-3 text-left">
    <PinInput
      bind:this={pinInputRef}
      id="unlock-pin"
      label={UNLOCK_PIN_LABEL}
      bind:value={pin}
      disabled={submitting || backingOff}
      autofocus
    />

    {#if backingOff}
      <p role="status" class="text-xs text-amber-400">{backoffMessage(secondsRemaining)}</p>
    {:else if attemptsRemaining !== null}
      <p role="status" class="text-xs text-zinc-400">{attemptsRemainingMessage(attemptsRemaining)}</p>
    {/if}

    {#if formError !== null}
      <p role="alert" class="text-xs text-red-400">{formError}</p>
    {/if}

    <button
      type="submit"
      disabled={submitting || backingOff || pin.length !== 6}
      class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300 disabled:opacity-60"
    >
      {submitting ? UNLOCKING_LABEL : UNLOCK_BUTTON_LABEL}
    </button>
  </form>
</main>
