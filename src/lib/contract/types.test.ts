import { describe, expect, it } from "vitest";

import { COLOUR_TOKENS } from "./types";

// WP-10 exit criterion (docs/src/work/wp-10-palette.md "Definition of
// done"): "unset" is WP-04's provisional placeholder — see the comment on
// COLOUR_TOKENS for why it existed — and no build may ship with it. This
// test is the guard against it silently reappearing (a bad merge, a revert
// of this package) rather than being caught only by eye at review.
describe("COLOUR_TOKENS", () => {
  it("does not contain the provisional 'unset' placeholder", () => {
    expect(COLOUR_TOKENS).not.toContain("unset");
  });

  it("holds between eight and ten tokens (palette.md requirement 1)", () => {
    expect(COLOUR_TOKENS.length).toBeGreaterThanOrEqual(8);
    expect(COLOUR_TOKENS.length).toBeLessThanOrEqual(10);
  });

  it("has no duplicate token names", () => {
    expect(new Set(COLOUR_TOKENS).size).toBe(COLOUR_TOKENS.length);
  });
});
