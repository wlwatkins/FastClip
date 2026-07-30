// The boundary validator every IPC payload passes through. The pre-refactor
// build cast every payload (`event.payload as Array<FastClip>`), which is
// erased at runtime and does not fail when the shape is wrong. This module
// is the fix: every value crossing the seam is checked, and a malformed one
// throws loudly instead of being rendered.

import {
  COLOUR_TOKENS,
  type Clip,
  type ClipError,
  type Colour,
  type ExportResult,
  type ImportReason,
  type ImportResult,
  type InvalidReason,
  type LockState,
  type Settings,
} from "./types";

/** Thrown by every `parse*` function below on a payload that does not match its contract shape. */
export class BoundaryValidationError extends Error {
  constructor(
    public readonly context: string,
    public readonly value: unknown,
  ) {
    super(`Malformed IPC payload at ${context}: ${safeDescribe(value)}`);
    this.name = "BoundaryValidationError";
  }
}

function safeDescribe(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isOneOf<T extends string>(value: unknown, members: readonly T[]): value is T {
  return typeof value === "string" && (members as readonly string[]).includes(value);
}

/** Membership check against the closed palette set (contract §1 `Colour`). */
export function isColour(value: unknown): value is Colour {
  return isOneOf(value, COLOUR_TOKENS);
}

/** Validates a single `Clip` (contract §1). Throws on any field of the wrong shape. */
export function parseClip(value: unknown, context = "Clip"): Clip {
  if (!isRecord(value)) {
    throw new BoundaryValidationError(context, value);
  }
  const { id, label, value: text, colour } = value;
  if (
    typeof id !== "string" ||
    typeof label !== "string" ||
    typeof text !== "string" ||
    !isColour(colour)
  ) {
    throw new BoundaryValidationError(context, value);
  }
  return { id, label, value: text, colour };
}

/** Validates the complete clip list carried by `list_clips` and the `update_clips` event. */
export function parseClipList(value: unknown, context = "Clip[]"): Clip[] {
  if (!Array.isArray(value)) {
    throw new BoundaryValidationError(context, value);
  }
  return value.map((entry, index) => parseClip(entry, `${context}[${index}]`));
}

/** Validates `get_lock_state`'s return and the `lock_state` event payload. */
export function parseLockState(value: unknown, context = "LockState"): LockState {
  if (!isRecord(value)) {
    throw new BoundaryValidationError(context, value);
  }
  const { encryption_enabled, locked, attempts_remaining, retry_after_ms } = value;
  if (
    typeof encryption_enabled !== "boolean" ||
    typeof locked !== "boolean" ||
    (attempts_remaining !== null && typeof attempts_remaining !== "number") ||
    (retry_after_ms !== null && typeof retry_after_ms !== "number")
  ) {
    throw new BoundaryValidationError(context, value);
  }
  return { encryption_enabled, locked, attempts_remaining, retry_after_ms };
}

/** Validates `get_settings`'s return. */
export function parseSettings(value: unknown, context = "Settings"): Settings {
  if (!isRecord(value) || typeof value.always_on_top !== "boolean") {
    throw new BoundaryValidationError(context, value);
  }
  return { always_on_top: value.always_on_top };
}

/** Validates `export_clips`'s return (contract §1 `ExportResult`). */
export function parseExportResult(value: unknown, context = "ExportResult"): ExportResult {
  if (!isRecord(value) || typeof value.exported !== "number") {
    throw new BoundaryValidationError(context, value);
  }
  return { exported: value.exported };
}

/** Validates `import_clips`'s return (contract §1 `ImportResult`). */
export function parseImportResult(value: unknown, context = "ImportResult"): ImportResult {
  if (!isRecord(value) || typeof value.imported !== "number") {
    throw new BoundaryValidationError(context, value);
  }
  return { imported: value.imported };
}

const INVALID_REASONS: readonly InvalidReason[] = [
  "required",
  "too_long",
  "contains_control_characters",
  "not_a_palette_token",
  "not_permitted",
  "unknown_field",
  "malformed_uuid",
  "not_a_permutation",
  "not_six_digits",
];

const IMPORT_REASONS: readonly ImportReason[] = [
  "malformed_json",
  "unsupported_version",
  "missing_field",
  "unknown_field",
  "invalid_value",
];

const IO_REASONS = ["not_found", "permission_denied", "disk_full", "other"] as const;

/**
 * Maps a rejected `invoke` to a `ClipError`, never throwing. Contract §0:
 * "A rejection that is not a `ClipError` object is a backend defect; the
 * frontend maps it to `{ "kind": "internal" }`." A malformed variant-specific
 * field is the same defect and gets the same treatment.
 */
export function toClipError(reason: unknown): ClipError {
  if (!isRecord(reason) || typeof reason.kind !== "string") {
    return { kind: "internal" };
  }

  switch (reason.kind) {
    case "not_found":
      return typeof reason.clip_id === "string"
        ? { kind: "not_found", clip_id: reason.clip_id }
        : { kind: "internal" };

    case "invalid_input":
      return typeof reason.field === "string" && isOneOf(reason.reason, INVALID_REASONS)
        ? { kind: "invalid_input", field: reason.field, reason: reason.reason }
        : { kind: "internal" };

    case "storage":
      return { kind: "storage" };

    case "io":
      return (reason.operation === "read" || reason.operation === "write") &&
        typeof reason.path === "string" &&
        isOneOf(reason.reason, IO_REASONS)
        ? { kind: "io", operation: reason.operation, path: reason.path, reason: reason.reason }
        : { kind: "internal" };

    case "crypto":
      return isOneOf(reason.reason, ["bad_key_material", "corrupt"] as const)
        ? { kind: "crypto", reason: reason.reason }
        : { kind: "internal" };

    case "unsupported_version":
      return isOneOf(reason.component, ["schema", "key_material"] as const) &&
        typeof reason.found === "number" &&
        typeof reason.supported === "number"
        ? {
            kind: "unsupported_version",
            component: reason.component,
            found: reason.found,
            supported: reason.supported,
          }
        : { kind: "internal" };

    case "import":
      return isOneOf(reason.reason, IMPORT_REASONS) &&
        (reason.field === null || typeof reason.field === "string") &&
        (reason.index === null || typeof reason.index === "number")
        ? {
            kind: "import",
            reason: reason.reason,
            field: (reason.field as string | null) ?? null,
            index: (reason.index as number | null) ?? null,
          }
        : { kind: "internal" };

    case "locked":
      return { kind: "locked" };

    case "wrong_state":
      return isOneOf(reason.required, ["locked", "encrypted", "unencrypted"] as const)
        ? { kind: "wrong_state", required: reason.required }
        : { kind: "internal" };

    case "bad_pin":
      return (reason.attempts_remaining === null || typeof reason.attempts_remaining === "number") &&
        (reason.retry_after_ms === null || typeof reason.retry_after_ms === "number")
        ? {
            kind: "bad_pin",
            attempts_remaining: (reason.attempts_remaining as number | null) ?? null,
            retry_after_ms: (reason.retry_after_ms as number | null) ?? null,
          }
        : { kind: "internal" };

    case "backoff":
      return typeof reason.retry_after_ms === "number"
        ? { kind: "backoff", retry_after_ms: reason.retry_after_ms }
        : { kind: "internal" };

    case "clipboard":
      return { kind: "clipboard" };

    case "internal":
      return { kind: "internal" };

    default:
      return { kind: "internal" };
  }
}
