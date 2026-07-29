// Generated from docs/src/architecture/contract.md. This module is the
// single source of truth for the wire shapes the frontend consumes; nothing
// else hand-declares them. Regenerate by hand from the contract when it
// changes — there is no codegen tool yet.
//
// Casing follows contract §0: snake_case everywhere, in both directions,
// without exception.

/**
 * The closed set of palette tokens (contract §1 `Colour`, palette.md
 * "Tokens"). `COLOUR_TOKENS` is the one declaration; `Colour` is derived from
 * it so the type and the boundary validator cannot drift apart.
 *
 * Ratified at WP-10 — nine tokens, each with its own fill and foreground
 * declared in the Tailwind theme (src/app.css) and documented with computed
 * contrast ratios on docs/src/product/palette.md. The provisional "unset"
 * placeholder WP-04 shipped is gone: no build ships with it, per WP-10's
 * definition of done.
 */
export const COLOUR_TOKENS = [
  "red",
  "amber",
  "lime",
  "green",
  "teal",
  "blue",
  "violet",
  "pink",
  "slate",
] as const;

export type Colour = (typeof COLOUR_TOKENS)[number];

/** The clip as it crosses the seam, in both directions (contract §1 `Clip`). */
export type Clip = {
  id: string;
  label: string;
  value: string;
  colour: Colour;
};

/** A clip before it has identity — the argument to `create_clip` (contract §1 `ClipDraft`). */
export type ClipDraft = {
  label: string;
  value: string;
  colour: Colour;
};

/** Contract §1 `LockState`. */
export type LockState = {
  encryption_enabled: boolean;
  locked: boolean;
  attempts_remaining: number | null;
  retry_after_ms: number | null;
};

/** Contract §1 `Settings`. */
export type Settings = {
  always_on_top: boolean;
};

/** Contract §1 `ExportResult` / `ImportResult`. */
export type ExportResult = { exported: number };
export type ImportResult = { imported: number };

/**
 * The on-disk export file format (contract §1 `ExportFile`). A file format,
 * not an IPC payload — versioned separately, parsed by WP-09.
 */
export type ExportFile = {
  format: "fastclip-export";
  version: number;
  clips: Array<{
    id?: string;
    label: string;
    value: string;
    colour: Colour;
  }>;
};

export type InvalidReason =
  | "required"
  | "too_long"
  | "contains_control_characters"
  | "not_a_palette_token"
  | "not_permitted"
  | "unknown_field"
  | "malformed_uuid"
  | "not_a_permutation"
  | "not_six_digits";

export type ImportReason =
  | "malformed_json"
  | "unsupported_version"
  | "missing_field"
  | "unknown_field"
  | "invalid_value";

/** Contract §4. Every command rejects with one of these, never a string. */
export type ClipError =
  | { kind: "not_found"; clip_id: string }
  | { kind: "invalid_input"; field: string; reason: InvalidReason }
  | { kind: "storage" }
  | {
      kind: "io";
      operation: "read" | "write";
      path: string;
      reason: "not_found" | "permission_denied" | "disk_full" | "other";
    }
  | { kind: "crypto"; reason: "bad_key_material" | "corrupt" }
  | {
      kind: "unsupported_version";
      component: "schema" | "key_material";
      found: number;
      supported: number;
    }
  | { kind: "import"; reason: ImportReason; field: string | null; index: number | null }
  | { kind: "locked" }
  | { kind: "wrong_state"; required: "locked" | "encrypted" | "unencrypted" }
  | { kind: "bad_pin"; attempts_remaining: number | null; retry_after_ms: number | null }
  | { kind: "backoff"; retry_after_ms: number }
  | { kind: "clipboard" }
  | { kind: "internal" };
