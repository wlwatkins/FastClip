// User-facing copy. Implements docs/src/product/copy.md — that page is the
// source; a call site quotes a constant or function from here (or from
// errorMessage.ts for a ClipError), never a literal string.
//
// The export warning is a security-sensitive string
// (docs/src/product/copy.md "Security-sensitive strings"): it must say
// plainly that the exported file is not encrypted, and must not be softened.

export const EXPORT_BUTTON_LABEL = "Export clips…";
export const EXPORT_CONFIRM_TITLE = "Export clips";
export const EXPORT_CONFIRM_LABEL = "Export";
export const EXPORT_WARNING =
  "The exported file is not encrypted. Anyone who can open it can read every clip in it, even if FastClip's PIN protection is on. Choose where you save it, and delete it once you no longer need it.";

export const IMPORT_BUTTON_LABEL = "Import clips…";
export const IMPORT_DIALOG_TITLE = "Import clips";

/** The file-type label Windows shows in the export/import file dialog's filter dropdown. Both dialogs name the same file type. */
export const EXPORT_FILE_FILTER_NAME = "FastClip export";

export function exportSuccessMessage(count: number): string {
  return count === 1 ? "Exported 1 clip." : `Exported ${count} clips.`;
}

export function importSuccessMessage(count: number): string {
  return count === 1 ? "Imported 1 clip." : `Imported ${count} clips.`;
}

// Search (spec §4.7).
//
// SEARCH_EMPTY_RESULT_MESSAGE must never echo the typed query — the query is
// a substring of a clip's `value`, which ADR-0002 keeps out of anything the
// user did not explicitly export.

export const SEARCH_TOGGLE_LABEL = "Search clips";
export const SEARCH_CLOSE_LABEL = "Close search";
export const SEARCH_PLACEHOLDER = "Search label or value…";
export const SEARCH_EMPTY_RESULT_MESSAGE = "No clips match your search.";
export const NO_CLIPS_MESSAGE = "No clips yet.";

export function searchDisabledReorderLabel(label: string): string {
  return `Reorder ${label}. Disabled while a search is active — clear the search to reorder.`;
}

export function reorderLabel(label: string): string {
  return `Reorder ${label}. Press arrow up or arrow down to move, or drag.`;
}

// Encryption and PIN (spec §4.8, ADR-0004, ADR-0010, ADR-0011).
//
// ENCRYPTION_WARNING is a security-sensitive string: it must say plainly that
// a forgotten PIN means the clips cannot be recovered (ADR-0004), and it must
// not be softened. Nothing below ever interpolates a PIN value.

export const ENCRYPTION_SECTION_LABEL = "PIN protection";
export const ENCRYPTION_STATE_ON_LABEL = "On";
export const ENCRYPTION_STATE_OFF_LABEL = "Off";
export const ENCRYPTION_ENABLE_BUTTON_LABEL = "Turn on PIN protection…";
export const ENCRYPTION_DISABLE_BUTTON_LABEL = "Turn off PIN protection…";
export const CHANGE_PIN_BUTTON_LABEL = "Change PIN…";
export const LOCK_BUTTON_LABEL = "Lock now";

export const ENCRYPTION_WARNING_TITLE = "Turn on PIN protection";
export const ENCRYPTION_WARNING =
  "If you forget this PIN, your clips cannot be recovered. There is no reset and no backdoor. Export a copy first if you want a fallback.";
export const ENCRYPTION_EXPORT_FIRST_BUTTON_LABEL = "Export clips first…";
export const ENCRYPTION_CONTINUE_BUTTON_LABEL = "Continue";
export const ENCRYPTION_PIN_LABEL = "6-digit PIN";
export const ENCRYPTION_PIN_CONFIRM_LABEL = "Confirm PIN";
export const ENCRYPTION_ENABLE_SUBMIT_LABEL = "Turn on";
export const PIN_MISMATCH_MESSAGE = "The PINs do not match.";

export const UNLOCK_PROMPT_TITLE = "FastClip is locked";
export const UNLOCK_PIN_LABEL = "PIN";
export const UNLOCK_BUTTON_LABEL = "Unlock";

/**
 * Landmark `aria-label` on the PIN prompt (copy.md "Tray item while locked").
 * Same string as the Rust tray item's label — both name the same action, so
 * they carry the identical text. The tray item lives in `tray.rs` and cannot
 * share this constant across the seam; this is the one place the frontend's
 * copy of it is written.
 */
export const UNLOCK_LANDMARK_LABEL = "Unlock FastClip";

/** Shown alongside the attempts-remaining status line on a failed unlock (copy.md "Wrong PIN"). Never shown pre-emptively — only after an evaluated failure. */
export const WRONG_PIN_MESSAGE = "Wrong PIN.";

export function attemptsRemainingMessage(remaining: number): string {
  return remaining === 1 ? "1 attempt remaining." : `${remaining} attempts remaining.`;
}

// Flat 30-second wait, no escalation, no ceiling (ADR-0011). The wording
// must never imply the wait grows with further attempts.
export function backoffMessage(secondsRemaining: number): string {
  const seconds = Math.max(1, secondsRemaining);
  return seconds === 1 ? "Too many attempts. Try again in 1 second." : `Too many attempts. Try again in ${seconds} seconds.`;
}

export const DISABLE_ENCRYPTION_TITLE = "Turn off PIN protection";
export const DISABLE_ENCRYPTION_WARNING = "Your clips will be stored unencrypted on this computer.";
export const DISABLE_ENCRYPTION_PIN_LABEL = "Current PIN";
export const DISABLE_ENCRYPTION_SUBMIT_LABEL = "Turn off";

export const CHANGE_PIN_TITLE = "Change PIN";
export const CHANGE_PIN_CURRENT_LABEL = "Current PIN";
export const CHANGE_PIN_NEW_LABEL = "New PIN";
export const CHANGE_PIN_CONFIRM_LABEL = "Confirm new PIN";
export const CHANGE_PIN_SUBMIT_LABEL = "Change PIN";

export const LOCKED_SETTINGS_NOTICE = "FastClip is locked. Enter your PIN to manage PIN protection, export or import.";

export function encryptionEnabledSuccessMessage(): string {
  return "PIN protection is on.";
}
export function encryptionDisabledSuccessMessage(): string {
  return "PIN protection is off.";
}
export function pinChangedSuccessMessage(): string {
  return "PIN changed.";
}

// Clip create/edit/delete (spec §4.2).

export const NEW_CLIP_TITLE = "New clip";
export const EDIT_CLIP_TITLE = "Edit clip";
export const CLIP_LABEL_FIELD_LABEL = "Label";
export const CLIP_VALUE_FIELD_LABEL = "Value";
export const CLIP_COLOUR_FIELD_LABEL = "Colour";
export const SAVE_BUTTON_LABEL = "Save";
export const CREATE_BUTTON_LABEL = "Create";
export const CANCEL_BUTTON_LABEL = "Cancel";
export const NEW_CLIP_BUTTON_LABEL = "New clip";

export const DELETE_CLIP_TITLE = "Delete clip";
export const DELETE_CLIP_CONFIRM_LABEL = "Delete";

export function deleteClipMessage(label: string): string {
  return `Delete "${label}"? This cannot be undone.`;
}

export function copiedMessage(label: string): string {
  return `Copied "${label}"`;
}

export function editClipAriaLabel(label: string): string {
  return `Edit ${label}`;
}

export function deleteClipAriaLabel(label: string): string {
  return `Delete ${label}`;
}

// Chrome, loading and failure states.

/** Landmark `aria-label` on the scrollable clip list region. */
export const CLIP_LIST_LANDMARK_LABEL = "Clips";

export const LOADING_MESSAGE = "Loading…";
export const TRY_AGAIN_LABEL = "Try again";
export const SETTINGS_TITLE = "Settings";
export const SETTINGS_BUTTON_LABEL = "Settings";
export const CLOSE_SETTINGS_LABEL = "Close settings";
export const ALWAYS_ON_TOP_LABEL = "Keep window on top";
export const MINIMISE_WINDOW_LABEL = "Minimise window";
export const CLOSE_WINDOW_LABEL = "Close window";

/** The fallback for a rejection that is not a `CommandError` — a promise rejection `errorMessage.ts` never sees, not a modelled `ClipError`. Identical wording to `ClipError.internal` (errorMessage.ts): both describe the same unmodelled-failure case to the user. */
export const UNEXPECTED_ERROR_MESSAGE = "An unexpected error occurred.";

/** Shown when `@tauri-apps/plugin-dialog`'s `save`/`open` itself rejects, before any path is chosen. Not a `ClipError` — there is no `invoke` here to fail. */
export const FILE_DIALOG_UNAVAILABLE_MESSAGE = "The file dialog could not be opened.";

// Busy-state button text. Each pairs with the button label it temporarily
// replaces while the corresponding command is in flight.
export const IMPORTING_LABEL = "Importing…";
export const EXPORTING_LABEL = "Exporting…";
export const ENCRYPTION_TURNING_ON_LABEL = "Turning on…";
export const ENCRYPTION_TURNING_OFF_LABEL = "Turning off…";
export const CHANGE_PIN_SUBMITTING_LABEL = "Changing…";
export const LOCKING_LABEL = "Locking…";
export const UNLOCKING_LABEL = "Unlocking…";
