// Inline validation for the create/edit form (spec §4.2, contract "Validation
// rules, stated once"). This mirrors the backend's rules so the user sees a
// message before a round trip; the backend's own validation is the one that
// decides (contract §4, "Both sides validate").
//
// Character counts are Unicode scalar values, not JavaScript's UTF-16
// `.length` (contract §0 "Character counts").

// Matches Unicode category Cc exactly, the same set the backend checks with
// `char::is_control` (wire.rs): C0 controls (U+0000-U+001F, includes tab and
// newline), DEL (U+007F), and C1 controls (U+0080-U+009F). The C1 range
// matters in practice: U+0085 (NEL) is treated as whitespace by Rust's
// `trim()`, so a label of a single U+0085 must be caught here as
// `contains_control_characters` before it can fall through to a `required`
// message for a field the user did not leave empty. Written with \x escapes
// rather than literal bytes so the pattern is legible in a diff.
const CONTROL_CHARACTERS = new RegExp("[\\x00-\\x1F\\x7F-\\x9F]");

export function charLength(value: string): number {
  return [...value].length;
}

export type FieldError = "required" | "too_long" | "contains_control_characters" | null;

/** label: 1-100 characters, not whitespace-only, no control characters (incl. newline/tab). */
export function validateLabel(label: string): FieldError {
  if (label.trim().length === 0) return "required";
  if (CONTROL_CHARACTERS.test(label)) return "contains_control_characters";
  if (charLength(label) > 100) return "too_long";
  return null;
}

/** value: 1-10,000 characters, not whitespace-only. Newlines and other characters are permitted. */
export function validateValue(value: string): FieldError {
  if (value.trim().length === 0) return "required";
  if (charLength(value) > 10_000) return "too_long";
  return null;
}
