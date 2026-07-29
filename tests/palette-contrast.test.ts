import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { COLOUR_TOKENS } from "../src/lib/contract/types";

// WP-10 (docs/src/work/wp-10-palette.md, test-engineer share): "Compute the
// contrast ratio for every token and fail below 4.5:1. The figures on the
// palette page are a claim until a test checks them."
//
// This test does NOT read docs/src/product/palette.md and does not hard-code
// the nine expected ratios. It parses the `--color-clip-<token>` /
// `--color-clip-<token>-fg` custom properties straight out of src/app.css —
// the actual Tailwind theme values the app ships — and recomputes the WCAG
// 2 §1.4.3 relative-luminance contrast ratio independently. If a hue is
// nudged in app.css without the palette page being updated to match, this
// test is what catches it; asserting against palette.md's own table would
// not.

const APP_CSS_PATH = resolve(__dirname, "../src/app.css");

// `[a-z_]+`, not `[a-z]+`: contract §1 permits multi-word snake_case tokens
// (e.g. a hypothetical `deep_blue`), and a token name that the class cannot
// match is a token silently dropped from every assertion below rather than
// a parse error — see the "matches every declared token" test, which is the
// backstop for exactly that failure mode.
const TOKEN_DECL = /--color-clip-([a-z_]+)(-fg)?:\s*(#[0-9a-fA-F]{6})\s*;/g;

type TokenColours = { fill?: string; fg?: string };

function parsePaletteTokens(css: string): Map<string, TokenColours> {
  const tokens = new Map<string, TokenColours>();
  for (const match of css.matchAll(TOKEN_DECL)) {
    const [, name, fgSuffix, hex] = match;
    const entry = tokens.get(name) ?? {};
    if (fgSuffix === "-fg") {
      entry.fg = hex;
    } else {
      entry.fill = hex;
    }
    tokens.set(name, entry);
  }
  return tokens;
}

/** sRGB hex -> [0,1] channel, WCAG 2 §1.4.3 gamma-expanded. */
function srgbChannelToLinear(channel255: number): number {
  const c = channel255 / 255;
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

function relativeLuminance(hex: string): number {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const [rl, gl, bl] = [r, g, b].map(srgbChannelToLinear);
  return 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
}

/** WCAG 2 §1.4.3 contrast ratio, lighter-over-darker, rounded to 2dp. */
function contrastRatio(hexA: string, hexB: string): number {
  const lA = relativeLuminance(hexA);
  const lB = relativeLuminance(hexB);
  const lighter = Math.max(lA, lB);
  const darker = Math.min(lA, lB);
  const ratio = (lighter + 0.05) / (darker + 0.05);
  return Math.round(ratio * 100) / 100;
}

describe("palette contrast (WP-10)", () => {
  const css = readFileSync(APP_CSS_PATH, "utf-8");
  const tokens = parsePaletteTokens(css);

  it("finds at least one clip colour token declared in src/app.css", () => {
    // Guards against the regex silently matching nothing (e.g. app.css moved
    // or the declaration syntax changed) and every it.each below vacuously
    // passing with zero cases.
    expect(tokens.size).toBeGreaterThan(0);
  });

  it("matches every declared token: parsed set from app.css equals COLOUR_TOKENS", () => {
    // The regex above defines this suite's coverage. Without pinning it to
    // COLOUR_TOKENS (the contract's actual token list, contract §1 /
    // src/lib/contract/types.ts), a token the regex fails to match — a
    // multi-word snake_case name the class doesn't cover, a fill declared as
    // oklch()/rgb() instead of 6-digit hex, or a token added to
    // COLOUR_TOKENS but forgotten in app.css — silently drops out of every
    // ratio assertion below instead of failing the suite. This assertion is
    // what makes that a red test rather than a smaller, quietly-passing one.
    const parsedNames = new Set(tokens.keys());
    const declaredNames = new Set(COLOUR_TOKENS);
    expect(parsedNames).toEqual(declaredNames);
  });

  it("declares every token with both a fill and a foreground", () => {
    for (const [name, { fill, fg }] of tokens) {
      expect(fill, `${name}: missing --color-clip-${name}`).toBeDefined();
      expect(fg, `${name}: missing --color-clip-${name}-fg`).toBeDefined();
    }
  });

  const entries = [...tokens.entries()].filter(
    ([, { fill, fg }]) => fill && fg,
  ) as Array<[string, { fill: string; fg: string }]>;

  it.each(entries)(
    "%s: foreground on fill clears WCAG AA (4.5:1)",
    (name, { fill, fg }) => {
      const ratio = contrastRatio(fill, fg);
      expect(
        ratio,
        `${name}: computed contrast ${ratio}:1 (fill ${fill}, fg ${fg}) is below the 4.5:1 WCAG AA floor`,
      ).toBeGreaterThanOrEqual(4.5);
    },
  );
});

describe("contrastRatio (self-check of the formula itself)", () => {
  it("gives the maximum ratio, 21:1, for black on white", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 1);
  });

  it("gives 1:1 for identical colours", () => {
    expect(contrastRatio("#336699", "#336699")).toBe(1);
  });

  it("is symmetric in argument order", () => {
    expect(contrastRatio("#111111", "#eeeeee")).toBe(
      contrastRatio("#eeeeee", "#111111"),
    );
  });
});
