import { describe, expect, it } from "vitest";
import { describeError, describeImportError, invalidInputMessage } from "./errorMessage";

// Regression coverage for the G3 rework findings (F1, F2):
// - `field` on an `import` error is truncated for display (copy.md
//   "Import-file failures"), because contract §4 records it has no length
//   bound.
// - `describeError` returns `null` for `locked` rather than a sentence the
//   deck refused (copy.md "Failures" `locked` row).

describe("describeImportError: field truncation (copy.md 'Import-file failures')", () => {
  it("leaves a field at or under 60 code points untouched", () => {
    const field = "a".repeat(60);
    const message = describeImportError({ kind: "import", reason: "unknown_field", field, index: 0 });
    expect(message).toBe(`Clip 1 has a field FastClip does not recognise ("${field}").`);
  });

  it("truncates a field over 60 code points to 60 and appends an ellipsis", () => {
    const field = "a".repeat(200);
    const message = describeImportError({ kind: "import", reason: "unknown_field", field, index: 0 });
    expect(message).toBe(`Clip 1 has a field FastClip does not recognise ("${"a".repeat(60)}…").`);
  });

  it("counts Unicode code points, not UTF-16 code units, so a cut never splits a surrogate pair", () => {
    // Each 𝕒 (U+1D552) is one code point but two UTF-16 code units.
    const field = "\u{1D552}".repeat(200);
    const message = describeImportError({ kind: "import", reason: "unknown_field", field, index: 0 });
    const expectedField = "\u{1D552}".repeat(60) + "…";
    expect(message).toBe(`Clip 1 has a field FastClip does not recognise ("${expectedField}").`);
    // No lone surrogate in the rendered text.
    expect(message).not.toMatch(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/);
  });

  // The test above uses 200 repetitions of the *same* 2-UTF-16-unit
  // character, so the cut point (60 code points = 120 UTF-16 units) is even
  // either way a naive implementation counts — a UTF-16 `.slice(0, 60)`
  // would land on a character boundary there too (60 units = 30 whole
  // astral characters), and would fail only on *length*, not on producing a
  // lone surrogate. It does not actually exercise the failure this bound
  // exists to prevent. This one does: 59 single-unit characters put the
  // 60th *code point* — the character this bound must keep whole — at
  // UTF-16 unit offset 59, so `field.slice(0, 60)` (units, the wrong axis)
  // would take that character's high surrogate only, and drop its low
  // surrogate. `truncateImportField`'s `Array.from(field).slice(0, 60)`
  // must not do that.
  it("truncates exactly at a boundary where the 60th code point is astral — a UTF-16-unit-based cut would split it, a code-point-based cut must not", () => {
    const astral = "\u{1D552}"; // 𝕒, one code point, two UTF-16 units
    const field = "a".repeat(59) + astral + "a".repeat(100);
    const message = describeImportError({ kind: "import", reason: "unknown_field", field, index: 0 });
    const expectedField = "a".repeat(59) + astral + "…"; // 59 BMP chars + the 60th code point (astral), whole
    expect(message).toBe(`Clip 1 has a field FastClip does not recognise ("${expectedField}").`);
    expect(message).not.toMatch(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/);
  });

  it("a top-level fault (index null) never carries the field suffix", () => {
    const message = describeImportError({ kind: "import", reason: "missing_field", field: null, index: null });
    expect(message).toBe("The file is missing a required field.");
  });
});

describe("describeError: 'locked' has no separate sentence (copy.md 'Failures')", () => {
  it("returns null rather than a sentence the deck refused", () => {
    expect(describeError({ kind: "locked" })).toBeNull();
  });

  it("every other declared kind still returns a non-null sentence", () => {
    expect(describeError({ kind: "not_found", clip_id: "x" })).not.toBeNull();
    expect(describeError({ kind: "storage" })).not.toBeNull();
    expect(describeError({ kind: "wrong_state", required: "unencrypted" })).not.toBeNull();
    expect(describeError({ kind: "clipboard" })).not.toBeNull();
    expect(describeError({ kind: "internal" })).not.toBeNull();
  });
});

describe("invalidInputMessage: the four InvalidReason values with no UI path still have a sentence (copy.md Validation)", () => {
  it("not_permitted", () => {
    expect(invalidInputMessage("not_permitted")).toBe("That value cannot be set here.");
  });
  it("unknown_field", () => {
    expect(invalidInputMessage("unknown_field")).toBe("Something in this request was not understood.");
  });
  it("malformed_uuid", () => {
    expect(invalidInputMessage("malformed_uuid")).toBe("Something in this request was not understood.");
  });
  it("not_a_permutation", () => {
    expect(invalidInputMessage("not_a_permutation")).toBe("The list changed elsewhere. Reloading.");
  });
});
