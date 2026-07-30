// Thin wrapper over `invoke` for the clip commands this package uses
// (contract §2: list_clips, create_clip, update_clip, delete_clip,
// copy_clip). Every rejection is normalised to a `ClipError` via
// `toClipError` (contract §0: "A rejection that is not a `ClipError`
// object is a backend defect; the frontend maps it to `internal`.") and
// every success payload that carries data is run through the boundary
// validator — nothing crossing the seam is cast.

import { invoke } from "@tauri-apps/api/core";
import type { Clip, ClipDraft, ExportResult, ImportResult, LockState, Settings } from "../contract/types";
import {
  parseClipList,
  parseExportResult,
  parseImportResult,
  parseLockState,
  parseSettings,
  toClipError,
} from "../contract/validate";
import type { ClipError } from "../contract/types";

/** Thrown by every function below. Carries the normalised `ClipError`. */
export class CommandError extends Error {
  constructor(public readonly error: ClipError) {
    super(`Command failed: ${error.kind}`);
    this.name = "CommandError";
  }
}

async function invokeCommand(command: string, args?: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke(command, args);
  } catch (reason) {
    throw new CommandError(toClipError(reason));
  }
}

/** contract §2 `list_clips`. */
export async function listClips(): Promise<Clip[]> {
  const result = await invokeCommand("list_clips");
  return parseClipList(result, "list_clips");
}

/** contract §2 `create_clip`. The backend mints `id`; never send one. */
export async function createClip(clip: ClipDraft): Promise<void> {
  await invokeCommand("create_clip", { clip });
}

/** contract §2 `update_clip`. `id`, position and `use_count` are unchanged by the backend. */
export async function updateClip(clip: Clip): Promise<void> {
  await invokeCommand("update_clip", { clip });
}

/** contract §2 `delete_clip`. Confirmation is a frontend concern; the backend deletes when asked. */
export async function deleteClip(clipId: string): Promise<void> {
  await invokeCommand("delete_clip", { clip_id: clipId });
}

/** contract §2 `copy_clip`. Writes the clipboard and increments `use_count` server-side. */
export async function copyClip(clipId: string): Promise<void> {
  await invokeCommand("copy_clip", { clip_id: clipId });
}

/**
 * contract §2 `reorder_clips`. `order` is every clip id, in the new display
 * order — a full permutation of the stored set, never a delta
 * (ADR-0007). Rejected whole with `invalid_input { reason: "not_a_permutation" }`
 * if it does not match the stored id set exactly.
 */
export async function reorderClips(order: string[]): Promise<void> {
  await invokeCommand("reorder_clips", { order });
}

/**
 * contract §2 `get_lock_state`. Called once at startup, never polled — later
 * changes arrive on the `lock_state` event.
 */
export async function getLockState(): Promise<LockState> {
  const result = await invokeCommand("get_lock_state");
  return parseLockState(result, "get_lock_state");
}

/**
 * contract §2 `get_settings`. Available while locked; only ever rejects with
 * `storage`, and even that is a fatal condition the startup sequence reports
 * through `get_lock_state` rather than here.
 */
export async function getSettings(): Promise<Settings> {
  const result = await invokeCommand("get_settings");
  return parseSettings(result, "get_settings");
}

/**
 * contract §2 `set_always_on_top`. The backend both persists and applies the
 * setting; this frontend never calls Tauri's window API for it.
 */
export async function setAlwaysOnTop(enabled: boolean): Promise<void> {
  await invokeCommand("set_always_on_top", { enabled });
}

/**
 * contract §2 `export_clips`. `path` is an absolute path the frontend already
 * obtained from a file-save dialog the user opened — this call never opens
 * one itself, and there is no path at which it can be reached without a user
 * action requesting it (WP-09: "no code path writes an export the user did
 * not request").
 */
export async function exportClips(path: string): Promise<ExportResult> {
  const result = await invokeCommand("export_clips", { path });
  return parseExportResult(result, "export_clips");
}

/**
 * contract §2 `import_clips`. Merge-only: every imported clip gets a freshly
 * minted `id`, nothing existing is deleted or overwritten, and a malformed or
 * partially-valid file is rejected whole (`import` error) with the store left
 * untouched.
 */
export async function importClips(path: string): Promise<ImportResult> {
  const result = await invokeCommand("import_clips", { path });
  return parseImportResult(result, "import_clips");
}

/**
 * contract §2 `unlock`. The launch path and the way back from a manual
 * `lock`, on the identical sequence. On success the backend emits
 * `lock_state` then `update_clips`; this function does not call `list_clips`
 * itself. The PIN is never logged — not here, not by any caller.
 */
export async function unlock(pin: string): Promise<void> {
  await invokeCommand("unlock", { pin });
}

/**
 * contract §2 `lock`. No PIN argument — the caller is already unlocked.
 * Idempotent on an already-locked store. Never returns `storage`: a failed
 * checkpoint or close is absorbed backend-side rather than reported.
 */
export async function lock(): Promise<void> {
  await invokeCommand("lock");
}

/**
 * contract §2 `enable_encryption`. The irreversibility warning and the offer
 * to export first are shown before this is ever called. The store remains
 * unlocked afterwards — `lock_state` carries `locked: false`.
 */
export async function enableEncryption(pin: string): Promise<void> {
  await invokeCommand("enable_encryption", { pin });
}

/**
 * contract §2 `disable_encryption`. Requires the store unlocked and the PIN.
 * Not subject to backoff: `bad_pin` from this command always carries
 * `attempts_remaining: null` and `retry_after_ms: null`.
 */
export async function disableEncryption(pin: string): Promise<void> {
  await invokeCommand("disable_encryption", { pin });
}

/**
 * contract §2 `change_pin`. Re-wraps the DEK; the database is not
 * re-encrypted, so this is instant. Emits nothing — no field of `LockState`
 * changes. Not subject to backoff, on the same reasoning as
 * `disable_encryption`.
 */
export async function changePin(currentPin: string, newPin: string): Promise<void> {
  await invokeCommand("change_pin", { current_pin: currentPin, new_pin: newPin });
}
