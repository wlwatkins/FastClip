<script lang="ts">
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { settingsState, setAlwaysOnTopState } from "../state/settings.svelte";
  import { lockState } from "../state/lockState.svelte";
  import {
    setAlwaysOnTop,
    exportClips,
    importClips,
    lock as lockCommand,
    enableEncryption,
    disableEncryption,
    changePin,
    CommandError,
  } from "../ipc/commands";
  import { describeError, invalidInputMessage } from "../errorMessage";
  import { showToast } from "../state/toast.svelte";
  import { overlayDismiss } from "../actions/overlayDismiss";
  import PinInput from "./PinInput.svelte";
  import {
    EXPORT_BUTTON_LABEL,
    EXPORT_CONFIRM_LABEL,
    EXPORT_CONFIRM_TITLE,
    EXPORT_WARNING,
    IMPORT_BUTTON_LABEL,
    EXPORT_FILE_FILTER_NAME,
    exportSuccessMessage,
    importSuccessMessage,
    ENCRYPTION_SECTION_LABEL,
    ENCRYPTION_ENABLE_BUTTON_LABEL,
    ENCRYPTION_DISABLE_BUTTON_LABEL,
    CHANGE_PIN_BUTTON_LABEL,
    LOCK_BUTTON_LABEL,
    ENCRYPTION_WARNING_TITLE,
    ENCRYPTION_WARNING,
    ENCRYPTION_EXPORT_FIRST_BUTTON_LABEL,
    ENCRYPTION_CONTINUE_BUTTON_LABEL,
    ENCRYPTION_PIN_LABEL,
    ENCRYPTION_PIN_CONFIRM_LABEL,
    ENCRYPTION_ENABLE_SUBMIT_LABEL,
    PIN_MISMATCH_MESSAGE,
    DISABLE_ENCRYPTION_TITLE,
    DISABLE_ENCRYPTION_WARNING,
    DISABLE_ENCRYPTION_PIN_LABEL,
    DISABLE_ENCRYPTION_SUBMIT_LABEL,
    CHANGE_PIN_TITLE,
    CHANGE_PIN_CURRENT_LABEL,
    CHANGE_PIN_NEW_LABEL,
    CHANGE_PIN_CONFIRM_LABEL,
    CHANGE_PIN_SUBMIT_LABEL,
    LOCKED_SETTINGS_NOTICE,
    encryptionEnabledSuccessMessage,
    encryptionDisabledSuccessMessage,
    pinChangedSuccessMessage,
    SETTINGS_TITLE,
    CLOSE_SETTINGS_LABEL,
    ALWAYS_ON_TOP_LABEL,
    CANCEL_BUTTON_LABEL,
    ENCRYPTION_STATE_ON_LABEL,
    ENCRYPTION_STATE_OFF_LABEL,
    ENCRYPTION_TURNING_ON_LABEL,
    ENCRYPTION_TURNING_OFF_LABEL,
    CHANGE_PIN_SUBMITTING_LABEL,
    LOCKING_LABEL,
    IMPORTING_LABEL,
    EXPORTING_LABEL,
    IMPORT_DIALOG_TITLE,
    UNEXPECTED_ERROR_MESSAGE,
    FILE_DIALOG_UNAVAILABLE_MESSAGE,
  } from "../copy";

  let { onclose }: { onclose: () => void } = $props();

  let pending = $state(false);

  // "settings" is the panel's normal content. Every other value replaces it
  // in place, rather than stacking a second overlay, so there is exactly one
  // dialog and one focus trap at any moment (WP-09: the disclosure is shown
  // "at the moment of export", not as a separate surface layered on top; the
  // same reasoning applies to the encryption irreversibility warning and
  // every PIN entry below).
  let view = $state<
    "settings" | "export-warning" | "enable-warning" | "enable-pin" | "disable-pin" | "change-pin"
  >("settings");
  // Where "export-warning" returns to when it is entered from the enable
  // flow's "export first" offer rather than from the top-level Export
  // button.
  let exportReturnView = $state<"settings" | "enable-warning">("settings");
  let exportBusy = $state(false);
  let importBusy = $state(false);

  // Enable-encryption fields.
  let enablePin = $state("");
  let enablePinConfirm = $state("");
  let enableError = $state<string | null>(null);
  let enableBusy = $state(false);

  // Disable-encryption fields.
  let disablePin = $state("");
  let disableError = $state<string | null>(null);
  let disableBusy = $state(false);

  // Change-PIN fields.
  let changeCurrentPin = $state("");
  let changeNewPin = $state("");
  let changeNewPinConfirm = $state("");
  let changeError = $state<string | null>(null);
  let changeBusy = $state(false);

  let lockBusy = $state(false);

  // `$state` rather than a plain binding: every focus target mounts
  // conditionally on `view`, and the reassignment on mount/unmount must
  // itself be reactive for svelte-check's non_reactive_update check to be
  // satisfied.
  let closeButton = $state<HTMLButtonElement | undefined>(undefined);
  let exportCancelButton = $state<HTMLButtonElement | undefined>(undefined);
  let enableWarningFocusEl = $state<HTMLButtonElement | undefined>(undefined);

  $effect(() => {
    if (view === "settings") {
      closeButton?.focus();
    } else if (view === "export-warning") {
      exportCancelButton?.focus();
    } else if (view === "enable-warning") {
      enableWarningFocusEl?.focus();
    }
    // "enable-pin", "disable-pin" and "change-pin" focus their own first PIN
    // field via `PinInput`'s `autofocus`.
  });

  function resetEnableForm() {
    enablePin = "";
    enablePinConfirm = "";
    enableError = null;
  }

  function resetDisableForm() {
    disablePin = "";
    disableError = null;
  }

  function resetChangeForm() {
    changeCurrentPin = "";
    changeNewPin = "";
    changeNewPinConfirm = "";
    changeError = null;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (view === "settings") {
      onclose();
      return;
    }
    if (view === "export-warning") {
      view = exportReturnView;
      return;
    }
    resetEnableForm();
    resetDisableForm();
    resetChangeForm();
    view = "settings";
  }

  function startExport() {
    exportReturnView = "settings";
    view = "export-warning";
  }

  function cancelExport() {
    view = exportReturnView;
  }

  function startEnable() {
    resetEnableForm();
    view = "enable-warning";
  }

  function exportFirstFromEnable() {
    exportReturnView = "enable-warning";
    view = "export-warning";
  }

  function continueToEnablePin() {
    view = "enable-pin";
  }

  // contract §2 `enable_encryption`: the PIN is entered twice in the UI and
  // sent once — confirming the two entries match is this function's job, not
  // the backend's. The irreversibility warning was already shown in
  // "enable-warning" before this view was ever reached.
  async function submitEnable(event: SubmitEvent) {
    event.preventDefault();
    if (enableBusy) return;
    if (enablePin.length !== 6) {
      enableError = invalidInputMessage("not_six_digits");
      return;
    }
    if (enablePin !== enablePinConfirm) {
      enableError = PIN_MISMATCH_MESSAGE;
      return;
    }
    enableBusy = true;
    enableError = null;
    try {
      await enableEncryption(enablePin);
      resetEnableForm();
      showToast(encryptionEnabledSuccessMessage());
      view = "settings";
    } catch (err) {
      if (err instanceof CommandError) {
        enableError =
          err.error.kind === "invalid_input" ? invalidInputMessage(err.error.reason) : describeError(err.error);
      } else {
        enableError = UNEXPECTED_ERROR_MESSAGE;
      }
    } finally {
      enableBusy = false;
    }
  }

  function startDisable() {
    resetDisableForm();
    view = "disable-pin";
  }

  // contract §2 `disable_encryption`: requires the PIN. Not subject to
  // backoff — a wrong PIN here always reports `attempts_remaining: null`.
  async function submitDisable(event: SubmitEvent) {
    event.preventDefault();
    if (disableBusy) return;
    if (disablePin.length !== 6) {
      disableError = invalidInputMessage("not_six_digits");
      return;
    }
    disableBusy = true;
    disableError = null;
    try {
      await disableEncryption(disablePin);
      resetDisableForm();
      showToast(encryptionDisabledSuccessMessage());
      view = "settings";
    } catch (err) {
      if (err instanceof CommandError) {
        disableError =
          err.error.kind === "invalid_input" ? invalidInputMessage(err.error.reason) : describeError(err.error);
      } else {
        disableError = UNEXPECTED_ERROR_MESSAGE;
      }
    } finally {
      disableBusy = false;
    }
  }

  function startChangePin() {
    resetChangeForm();
    view = "change-pin";
  }

  // contract §2 `change_pin`: re-wraps the DEK; the store is not
  // re-encrypted, so this is instant. Not subject to backoff, for the same
  // reason as `disable_encryption`.
  async function submitChangePin(event: SubmitEvent) {
    event.preventDefault();
    if (changeBusy) return;
    if (changeCurrentPin.length !== 6 || changeNewPin.length !== 6) {
      changeError = invalidInputMessage("not_six_digits");
      return;
    }
    if (changeNewPin !== changeNewPinConfirm) {
      changeError = PIN_MISMATCH_MESSAGE;
      return;
    }
    changeBusy = true;
    changeError = null;
    try {
      await changePin(changeCurrentPin, changeNewPin);
      resetChangeForm();
      showToast(pinChangedSuccessMessage());
      view = "settings";
    } catch (err) {
      if (err instanceof CommandError) {
        changeError =
          err.error.kind === "invalid_input" ? invalidInputMessage(err.error.reason) : describeError(err.error);
      } else {
        changeError = UNEXPECTED_ERROR_MESSAGE;
      }
    } finally {
      changeBusy = false;
    }
  }

  // Toasts a single-sentence failure for the actions below. `describeError`
  // returns `null` only for `locked`, which shows no toast here: this panel
  // already replaces its controls with LOCKED_SETTINGS_NOTICE once locked
  // (copy.md "Failures" `locked` row — the notice is the answer).
  function toastCommandError(err: unknown): void {
    if (err instanceof CommandError) {
      const message = describeError(err.error);
      if (message !== null) showToast(message);
    } else {
      showToast(UNEXPECTED_ERROR_MESSAGE);
    }
  }

  // contract §2 `lock`: no PIN argument, and it cannot fail on this store's
  // own merits once encryption is on (`wrong_state` is unreachable here
  // because this control is hidden whenever `encryption_enabled` is false).
  // The lock itself is observed through the `lock_state` event and
  // App.svelte's effect, not through this function's return.
  async function handleLock() {
    if (lockBusy) return;
    lockBusy = true;
    try {
      await lockCommand();
    } catch (err) {
      toastCommandError(err);
    } finally {
      lockBusy = false;
    }
  }

  // contract §2 `export_clips`: the frontend opens the file dialog and
  // passes the chosen absolute path; a cancelled dialog produces no `invoke`
  // and no error. No autosave and no path this can be reached from besides
  // this button, behind the warning above.
  async function confirmExport() {
    if (exportBusy) return;
    let path: string | null;
    try {
      path = await save({
        title: EXPORT_CONFIRM_TITLE,
        defaultPath: "fastclip-export.json",
        filters: [{ name: EXPORT_FILE_FILTER_NAME, extensions: ["json"] }],
      });
    } catch {
      // Return to wherever this warning was entered from — the top-level
      // Export button (exportReturnView === "settings") or the enable-PIN
      // flow's "export first" offer (exportReturnView === "enable-warning").
      // Hardcoding "settings" here would drop a user who took the offer back
      // to the root instead of to the warning they were part-way through.
      view = exportReturnView;
      showToast(FILE_DIALOG_UNAVAILABLE_MESSAGE);
      return;
    }
    if (path === null) {
      // User cancelled the dialog; stay on the warning rather than closing
      // the whole panel out from under them.
      return;
    }
    exportBusy = true;
    try {
      const result = await exportClips(path);
      showToast(exportSuccessMessage(result.exported));
      // Same reasoning as the catch above: the success path is the one a
      // user who took "export first" is more likely to take, and it is the
      // one where losing their place would matter most — restarting from
      // "settings" would silently skip the irreversibility warning they were
      // in the middle of reading.
      view = exportReturnView;
    } catch (err) {
      toastCommandError(err);
    } finally {
      exportBusy = false;
    }
  }

  // contract §2 `import_clips`: merge-only, every imported clip gets a fresh
  // id, and a malformed or partially-valid file is rejected whole with the
  // store left untouched. There is no confirmation step here — import never
  // deletes or overwrites anything the disclosure above would need to cover.
  async function handleImport() {
    if (importBusy) return;
    let path: string | null;
    try {
      path = await open({
        title: IMPORT_DIALOG_TITLE,
        multiple: false,
        directory: false,
        filters: [{ name: EXPORT_FILE_FILTER_NAME, extensions: ["json"] }],
      });
    } catch {
      showToast(FILE_DIALOG_UNAVAILABLE_MESSAGE);
      return;
    }
    if (path === null || Array.isArray(path)) return;
    importBusy = true;
    try {
      const result = await importClips(path);
      showToast(importSuccessMessage(result.imported));
    } catch (err) {
      toastCommandError(err);
    } finally {
      importBusy = false;
    }
  }

  // contract §2 `set_always_on_top`: the backend both persists and applies
  // the flag. This never calls Tauri's window API directly, and the toggle
  // reflects only a value the backend has confirmed — it does not flip
  // optimistically and roll back on failure.
  async function handleToggle() {
    if (pending) return;
    const next = !settingsState.alwaysOnTop;
    pending = true;
    try {
      await setAlwaysOnTop(next);
      setAlwaysOnTopState(next);
    } catch (err) {
      toastCommandError(err);
    } finally {
      pending = false;
    }
  }
</script>

<!-- The overlay is a mouse-only "click outside to dismiss" convenience; Escape and the Close button are the keyboard equivalents. See overlayDismiss.ts for the drag-out-of-dialog guard. -->
<div
  role="presentation"
  class="fixed inset-0 z-30 flex items-center justify-center bg-black/50"
  use:overlayDismiss={onclose}
>
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-panel-title"
    aria-describedby={view === "export-warning"
      ? "export-warning-message"
      : view === "enable-warning"
        ? "enable-warning-message"
        : view === "disable-pin"
          ? "disable-warning-message"
          : undefined}
    tabindex="-1"
    class="w-[min(90%,20rem)] rounded-lg bg-zinc-800 p-4 text-zinc-100"
    onclick={(event) => event.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <div class="mb-4 flex items-center justify-between">
      <h2 id="settings-panel-title" class="text-base font-semibold">
        {#if view === "export-warning"}
          {EXPORT_CONFIRM_TITLE}
        {:else if view === "enable-warning" || view === "enable-pin"}
          {ENCRYPTION_WARNING_TITLE}
        {:else if view === "disable-pin"}
          {DISABLE_ENCRYPTION_TITLE}
        {:else if view === "change-pin"}
          {CHANGE_PIN_TITLE}
        {:else}
          {SETTINGS_TITLE}
        {/if}
      </h2>
      {#if view === "settings"}
        <button
          bind:this={closeButton}
          type="button"
          aria-label={CLOSE_SETTINGS_LABEL}
          class="rounded p-1 text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
          onclick={onclose}
        >
          <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true">
            <line x1="6" y1="6" x2="18" y2="18" stroke-linecap="round" />
            <line x1="18" y1="6" x2="6" y2="18" stroke-linecap="round" />
          </svg>
        </button>
      {/if}
    </div>

    {#if view === "settings"}
      <div class="flex items-center justify-between gap-3">
        <span id="always-on-top-label" class="text-sm text-zinc-300">{ALWAYS_ON_TOP_LABEL}</span>
        <button
          type="button"
          role="switch"
          aria-checked={settingsState.alwaysOnTop}
          aria-labelledby="always-on-top-label"
          disabled={pending}
          class="group relative inline-flex h-5 w-9 shrink-0 items-center rounded-full bg-zinc-600 transition-colors hover:bg-zinc-500 aria-checked:bg-sky-600 aria-checked:hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-400 disabled:opacity-60"
          onclick={handleToggle}
        >
          <span
            class="inline-block h-3.5 w-3.5 translate-x-1 rounded-full bg-white transition-transform group-aria-checked:translate-x-4"
          ></span>
        </button>
      </div>

      <!-- contract §2: export_clips and import_clips both return `locked`
           while the store is locked, and spec §4.8 says export is
           unavailable while locked outright — so both are hidden rather than
           left to fail. -->
      {#if !lockState.locked}
        <div class="mt-4 flex flex-col gap-2 border-t border-zinc-700 pt-4">
          <button
            type="button"
            class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 disabled:opacity-60"
            disabled={importBusy}
            onclick={startExport}
          >
            {EXPORT_BUTTON_LABEL}
          </button>
          <button
            type="button"
            class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 disabled:opacity-60"
            disabled={importBusy}
            onclick={handleImport}
          >
            {importBusy ? IMPORTING_LABEL : IMPORT_BUTTON_LABEL}
          </button>
        </div>
      {/if}

      <!-- Encryption and PIN (WP-07, spec §4.8). The "Turn on" control is
           unreachable while `encryption_enabled` is false implies
           `locked` is false too, so there is no case here where a locked
           store hides a live "Turn on" button. -->
      <div class="mt-4 flex flex-col gap-2 border-t border-zinc-700 pt-4">
        <div class="flex items-center justify-between gap-3">
          <span class="text-sm text-zinc-300">{ENCRYPTION_SECTION_LABEL}</span>
          <span class="text-xs text-zinc-400">{lockState.encryption_enabled ? ENCRYPTION_STATE_ON_LABEL : ENCRYPTION_STATE_OFF_LABEL}</span>
        </div>

        {#if lockState.locked}
          <p class="text-xs text-zinc-400">{LOCKED_SETTINGS_NOTICE}</p>
        {:else if !lockState.encryption_enabled}
          <button
            type="button"
            class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            onclick={startEnable}
          >
            {ENCRYPTION_ENABLE_BUTTON_LABEL}
          </button>
        {:else}
          <button
            type="button"
            class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 disabled:opacity-60"
            disabled={lockBusy}
            onclick={handleLock}
          >
            {lockBusy ? LOCKING_LABEL : LOCK_BUTTON_LABEL}
          </button>
          <button
            type="button"
            class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            onclick={startChangePin}
          >
            {CHANGE_PIN_BUTTON_LABEL}
          </button>
          <button
            type="button"
            class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            onclick={startDisable}
          >
            {ENCRYPTION_DISABLE_BUTTON_LABEL}
          </button>
        {/if}
      </div>
    {:else if view === "export-warning"}
      <p id="export-warning-message" class="mb-4 text-sm text-zinc-300">{EXPORT_WARNING}</p>
      <div class="flex justify-end gap-2">
        <button
          bind:this={exportCancelButton}
          type="button"
          class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
          disabled={exportBusy}
          onclick={cancelExport}
        >
          {CANCEL_BUTTON_LABEL}
        </button>
        <button
          type="button"
          class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300 disabled:opacity-60"
          disabled={exportBusy}
          onclick={confirmExport}
        >
          {exportBusy ? EXPORTING_LABEL : EXPORT_CONFIRM_LABEL}
        </button>
      </div>
    {:else if view === "enable-warning"}
      <!-- ADR-0004: the wording must describe what is actually true. A
           forgotten PIN is unrecoverable by design — there is no reset and
           no backdoor — and this text says so without softening it. -->
      <p id="enable-warning-message" class="mb-4 text-sm text-zinc-300">{ENCRYPTION_WARNING}</p>
      <div class="flex flex-col gap-2">
        <button
          bind:this={enableWarningFocusEl}
          type="button"
          class="rounded px-2 py-1.5 text-left text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
          onclick={exportFirstFromEnable}
        >
          {ENCRYPTION_EXPORT_FIRST_BUTTON_LABEL}
        </button>
        <div class="mt-2 flex justify-end gap-2">
          <button
            type="button"
            class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            onclick={() => (view = "settings")}
          >
            {CANCEL_BUTTON_LABEL}
          </button>
          <button
            type="button"
            class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300"
            onclick={continueToEnablePin}
          >
            {ENCRYPTION_CONTINUE_BUTTON_LABEL}
          </button>
        </div>
      </div>
    {:else if view === "enable-pin"}
      <form onsubmit={submitEnable} novalidate class="flex flex-col gap-3">
        <PinInput id="enable-pin" label={ENCRYPTION_PIN_LABEL} bind:value={enablePin} disabled={enableBusy} autofocus />
        <PinInput
          id="enable-pin-confirm"
          label={ENCRYPTION_PIN_CONFIRM_LABEL}
          bind:value={enablePinConfirm}
          disabled={enableBusy}
        />
        {#if enableError !== null}
          <p role="alert" class="text-xs text-red-400">{enableError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            disabled={enableBusy}
            onclick={() => {
              resetEnableForm();
              view = "settings";
            }}
          >
            {CANCEL_BUTTON_LABEL}
          </button>
          <button
            type="submit"
            class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300 disabled:opacity-60"
            disabled={enableBusy}
          >
            {enableBusy ? ENCRYPTION_TURNING_ON_LABEL : ENCRYPTION_ENABLE_SUBMIT_LABEL}
          </button>
        </div>
      </form>
    {:else if view === "disable-pin"}
      <p id="disable-warning-message" class="mb-3 text-sm text-zinc-300">{DISABLE_ENCRYPTION_WARNING}</p>
      <form onsubmit={submitDisable} novalidate class="flex flex-col gap-3">
        <PinInput
          id="disable-pin"
          label={DISABLE_ENCRYPTION_PIN_LABEL}
          bind:value={disablePin}
          disabled={disableBusy}
          autofocus
        />
        {#if disableError !== null}
          <p role="alert" class="text-xs text-red-400">{disableError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            disabled={disableBusy}
            onclick={() => {
              resetDisableForm();
              view = "settings";
            }}
          >
            {CANCEL_BUTTON_LABEL}
          </button>
          <button
            type="submit"
            class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300 disabled:opacity-60"
            disabled={disableBusy}
          >
            {disableBusy ? ENCRYPTION_TURNING_OFF_LABEL : DISABLE_ENCRYPTION_SUBMIT_LABEL}
          </button>
        </div>
      </form>
    {:else}
      <form onsubmit={submitChangePin} novalidate class="flex flex-col gap-3">
        <PinInput
          id="change-pin-current"
          label={CHANGE_PIN_CURRENT_LABEL}
          bind:value={changeCurrentPin}
          disabled={changeBusy}
          autofocus
        />
        <PinInput id="change-pin-new" label={CHANGE_PIN_NEW_LABEL} bind:value={changeNewPin} disabled={changeBusy} />
        <PinInput
          id="change-pin-confirm"
          label={CHANGE_PIN_CONFIRM_LABEL}
          bind:value={changeNewPinConfirm}
          disabled={changeBusy}
        />
        {#if changeError !== null}
          <p role="alert" class="text-xs text-red-400">{changeError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="rounded px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-700 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400"
            disabled={changeBusy}
            onclick={() => {
              resetChangeForm();
              view = "settings";
            }}
          >
            {CANCEL_BUTTON_LABEL}
          </button>
          <button
            type="submit"
            class="rounded bg-sky-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-sky-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300 disabled:opacity-60"
            disabled={changeBusy}
          >
            {changeBusy ? CHANGE_PIN_SUBMITTING_LABEL : CHANGE_PIN_SUBMIT_LABEL}
          </button>
        </div>
      </form>
    {/if}
  </div>
</div>
