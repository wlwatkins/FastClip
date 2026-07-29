import { describe, expect, it } from "vitest";
import { BoundaryValidationError, parseClip, parseClipList, isColour } from "./validate";

// WP-05 (test-engineer share): "A malformed payload is rejected at the
// boundary. src/lib/contract/validate.ts is the thing under test; a payload
// that would once have passed as an `as` cast must now throw." The
// pre-refactor build cast `event.payload as Array<FastClip>` and rendered
// whatever came back; these tests are the replacement for that trust.

const VALID_CLIP = { id: "11111111-1111-1111-1111-111111111111", label: "L", value: "V", colour: "blue" };

describe("parseClip: a payload that would once have passed an `as Clip` cast", () => {
  it("accepts a well-formed clip", () => {
    expect(parseClip(VALID_CLIP)).toEqual(VALID_CLIP);
  });

  it("throws on a missing label — the field an `as` cast would silently leave undefined", () => {
    const { label: _label, ...malformed } = VALID_CLIP;
    expect(() => parseClip(malformed)).toThrow(BoundaryValidationError);
  });

  it("throws on a colour outside the closed palette set (contract §1 Colour)", () => {
    expect(() => parseClip({ ...VALID_CLIP, colour: "chartreuse" })).toThrow(BoundaryValidationError);
  });

  it("throws on the retired 'unset' provisional token surviving into a payload", () => {
    // Contract: no build may ship with "unset"; a runtime payload carrying it
    // is exactly as malformed as any other non-member string.
    expect(() => parseClip({ ...VALID_CLIP, colour: "unset" })).toThrow(BoundaryValidationError);
  });

  it("throws when label is the wrong JSON type (a number, not a string)", () => {
    expect(() => parseClip({ ...VALID_CLIP, label: 42 })).toThrow(BoundaryValidationError);
  });

  it("throws on null", () => {
    expect(() => parseClip(null)).toThrow(BoundaryValidationError);
  });

  it("throws on an array (not a record)", () => {
    expect(() => parseClip([VALID_CLIP])).toThrow(BoundaryValidationError);
  });

  it("throws on a bare string", () => {
    expect(() => parseClip("not a clip")).toThrow(BoundaryValidationError);
  });

  it("does not throw merely because extra fields (e.g. a stray use_count) are present — parseClip picks named fields only", () => {
    // Contract §1: use_count never crosses the seam; the backend must never
    // send it. If it did anyway, the frontend reading only the four named
    // fields (rather than spreading the object) means it is silently
    // ignored client-side, not smuggled into state. This test documents that
    // behaviour rather than asserting it is a defect — the backend's own
    // never-sends-it guarantee is backend-dev's, proven in ipc.rs.
    const withExtra = { ...VALID_CLIP, use_count: 7 };
    expect(parseClip(withExtra)).toEqual(VALID_CLIP);
  });
});

describe("parseClipList: the update_clips and list_clips payload", () => {
  it("accepts an empty array — no clips is not an error", () => {
    expect(parseClipList([])).toEqual([]);
  });

  it("accepts a list of valid clips, in order", () => {
    const second = { ...VALID_CLIP, id: "22222222-2222-2222-2222-222222222222" };
    expect(parseClipList([VALID_CLIP, second])).toEqual([VALID_CLIP, second]);
  });

  it("throws if any single entry is malformed, naming its index in the error context", () => {
    const malformed = { ...VALID_CLIP, value: 123 };
    expect(() => parseClipList([VALID_CLIP, malformed])).toThrow(BoundaryValidationError);
    try {
      parseClipList([VALID_CLIP, malformed]);
      expect.unreachable();
    } catch (err) {
      expect(err).toBeInstanceOf(BoundaryValidationError);
      expect((err as BoundaryValidationError).context).toContain("[1]");
    }
  });

  it("throws when the top-level payload is not an array at all (e.g. a bare object)", () => {
    expect(() => parseClipList({ clips: [VALID_CLIP] })).toThrow(BoundaryValidationError);
  });

  it("throws when the top-level payload is null", () => {
    expect(() => parseClipList(null)).toThrow(BoundaryValidationError);
  });

  it("throws when the top-level payload is a string that merely looks like JSON", () => {
    expect(() => parseClipList(JSON.stringify([VALID_CLIP]))).toThrow(BoundaryValidationError);
  });
});

describe("isColour: palette membership", () => {
  it("is true for every declared token and false for a token-shaped string outside the set", () => {
    expect(isColour("blue")).toBe(true);
    expect(isColour("chartreuse")).toBe(false);
    expect(isColour("unset")).toBe(false);
    expect(isColour(42)).toBe(false);
    expect(isColour(null)).toBe(false);
  });
});
