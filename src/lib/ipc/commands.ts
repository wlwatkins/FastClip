// Thin wrapper over `invoke` for the clip commands this package uses
// (contract §2: list_clips, create_clip, update_clip, delete_clip,
// copy_clip). Every rejection is normalised to a `ClipError` via
// `toClipError` (contract §0: "A rejection that is not a `ClipError`
// object is a backend defect; the frontend maps it to `internal`.") and
// every success payload that carries data is run through the boundary
// validator — nothing crossing the seam is cast.

import { invoke } from "@tauri-apps/api/core";
import type { Clip, ClipDraft } from "../contract/types";
import { parseClipList, toClipError } from "../contract/validate";
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
