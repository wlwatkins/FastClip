// Maps a `ClipError` to a sentence the user can read (contract §4: "The
// frontend maps every variant to a sentence from the copy deck. A raw error
// is never shown.").
//
// PROVISIONAL: docs/src/product/copy.md is not yet written — it is WP-11's
// deliverable, which explicitly depends on WP-07 and WP-09 and runs after
// this package. WP-11's own definition of done is "replace inline strings in
// the components with the deck's wording", so authoring plain sentences here
// now and replacing them later is the sequencing the work packages describe,
// not a workaround. Every sentence below is this file's own wording and
// should be treated as provisional until WP-11 lands.

import type { ClipError, InvalidReason } from "./contract/types";

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

/** A sentence for any `ClipError`, for surfaces that show one message rather than an inline field error. */
export function describeError(error: ClipError): string {
  switch (error.kind) {
    case "not_found":
      return "That clip no longer exists.";
    case "invalid_input":
      return invalidInputMessage(error.reason);
    case "storage":
      return "The clip store could not be read or written.";
    case "io":
      return "The file could not be reached.";
    case "crypto":
      return "The store could not be opened. Import a backup to recover your clips.";
    case "unsupported_version":
      return "This store is from a newer version of FastClip.";
    case "import":
      return "The import file could not be read.";
    case "locked":
      return "FastClip is locked.";
    case "wrong_state":
      return "That action is not available right now.";
    case "bad_pin":
      return "Wrong PIN.";
    case "backoff":
      return "Too many attempts. Wait and try again.";
    case "clipboard":
      return "The clip could not be copied.";
    case "internal":
      return "An unexpected error occurred.";
  }
}
