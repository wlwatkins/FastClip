// Maps a `ClipError` to a sentence the user can read (contract §4: "The
// frontend maps every variant to a sentence from the copy deck. A raw error
// is never shown."). Implements docs/src/product/copy.md "Failures — every
// ClipError variant"; that page is the source and this file the
// implementation, not the other way round.

import type { ClipError, ImportReason, InvalidReason } from "./contract/types";
import { WRONG_PIN_MESSAGE, UNEXPECTED_ERROR_MESSAGE } from "./copy";

const INVALID_REASON_TEXT: Record<InvalidReason, string> = {
  required: "This field is required.",
  too_long: "This is too long.",
  contains_control_characters: "This contains a character that is not allowed.",
  not_a_palette_token: "Choose a colour from the list.",
  not_permitted: "That value cannot be set here.",
  unknown_field: "Something in this request was not understood.",
  malformed_uuid: "Something in this request was not understood.",
  not_a_permutation: "The list changed elsewhere. Reloading.",
  not_six_digits: "Enter exactly 6 digits.",
};

/** The inline message for one field's validation state, `invalid_input` only. */
export function invalidInputMessage(reason: InvalidReason): string {
  return INVALID_REASON_TEXT[reason];
}

const IMPORT_REASON_TEXT: Record<ImportReason, string> = {
  malformed_json: "This is not a FastClip export file.",
  unsupported_version: "This export file is from a newer version of FastClip.",
  missing_field: "is missing a required field",
  unknown_field: "has a field FastClip does not recognise",
  invalid_value: "has an invalid value",
};

// contract §4 "field echoes a key from the file, and has no length bound":
// the wire value is unbounded, and the contract delegates the display length
// to this page (copy.md "Import-file failures"). 60 code points, elided with
// "…", counted in Unicode code points so a cut never lands inside a
// surrogate pair.
const IMPORT_FIELD_DISPLAY_LIMIT = 60;

function truncateImportField(field: string): string {
  const codePoints = Array.from(field);
  if (codePoints.length <= IMPORT_FIELD_DISPLAY_LIMIT) return field;
  return codePoints.slice(0, IMPORT_FIELD_DISPLAY_LIMIT).join("") + "…";
}

const CRYPTO_REASON_TEXT: Record<"bad_key_material" | "corrupt", string> = {
  // The key material is missing, unreadable, or bound to another Windows
  // account or machine (contract §4). Distinct from `corrupt` because the
  // remedy the sentence should suggest — importing on this device rather
  // than describing a damaged file — differs.
  bad_key_material:
    "This store is bound to a different Windows account or machine and cannot be opened here. Import a backup to recover your clips on this device.",
  // The key unwrapped but the database will not open or fails its integrity
  // check, or (encryption off) the database is unreadable on its own.
  corrupt: "This store could not be opened. Import a backup to recover your clips.",
};

function describeCryptoError(reason: "bad_key_material" | "corrupt"): string {
  return CRYPTO_REASON_TEXT[reason];
}

/**
 * The message for an `import` error (contract §4: "index is the 0-based
 * position of the offending clip in the clips array; field names the
 * offending field. Either may be null when the fault is at the top level.").
 * `malformed_json` and `unsupported_version` describe the whole file and
 * never carry a field or index; the other three name where the fault is when
 * they can.
 */
export function describeImportError(error: Extract<ClipError, { kind: "import" }>): string {
  const { reason, field, index } = error;
  if (reason === "malformed_json" || reason === "unsupported_version") {
    return IMPORT_REASON_TEXT[reason];
  }
  const where = index !== null ? `Clip ${index + 1}` : "The file";
  const suffix = field !== null ? ` ("${truncateImportField(field)}")` : "";
  return `${where} ${IMPORT_REASON_TEXT[reason]}${suffix}.`;
}

/**
 * A sentence for any `ClipError`, for surfaces that show one message rather
 * than an inline field error. Returns `null` for `locked` (copy.md
 * "Failures" `locked` row: "no separate sentence — the prompt itself is the
 * answer"). `null` is a sentinel meaning "show nothing here", not an absent
 * case — every caller must treat it as such rather than falling back to a
 * sentence this function does not have.
 */
export function describeError(error: ClipError): string | null {
  switch (error.kind) {
    case "not_found":
      return "That clip no longer exists.";
    case "invalid_input":
      return invalidInputMessage(error.reason);
    case "storage":
      // Covers a store that could not be created or reached at all *and* a
      // `clips.db` that exists intact but is currently unreadable — most
      // often another process, including another running copy of FastClip,
      // holding it open. The second case is not damage, and this sentence
      // must never suggest re-importing: doing so over a merely locked file
      // is how a user replaces a store that was completely intact
      // (docs/src/product/copy.md "The unreadable-store sentence").
      return "FastClip could not read or write the clip store. If another program — including another copy of FastClip — has it open, close it and try again. Nothing has been changed.";
    case "io":
      return "The file could not be reached.";
    case "crypto":
      return describeCryptoError(error.reason);
    case "unsupported_version":
      return "This store is from a newer version of FastClip.";
    case "import":
      return describeImportError(error);
    case "locked":
      // copy.md "Failures" locked row: the PIN prompt already shown in its
      // place is the answer. See the doc comment on this function.
      return null;
    case "wrong_state":
      return "That action is not available right now.";
    case "bad_pin":
      return WRONG_PIN_MESSAGE;
    case "backoff":
      // A generic fallback for a surface that shows one sentence rather than
      // the dedicated countdown (PinPrompt.svelte uses backoffMessage()
      // directly). Kept intentionally vague on duration: this path has no
      // `secondsRemaining` to hand, and "wait and try again" is still true
      // regardless of the flat 30-second figure (ADR-0011).
      return "Too many attempts. Wait and try again.";
    case "clipboard":
      return "The clip could not be copied.";
    case "internal":
      return UNEXPECTED_ERROR_MESSAGE;
  }
}
