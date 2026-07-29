// Per-token Tailwind class names and display labels for the palette
// (docs/src/product/palette.md "Tokens"). Written out literally, one entry
// per token, rather than built with a template string (`bg-clip-${token}`):
// Tailwind's class scanner only picks up class names that appear as literal
// text in a source file, so a computed class name is invisible to it and
// silently renders unstyled.
//
// `fgClass` exists so a colour swatch can render its own foreground on its
// own fill (WP-05 definition of done) rather than falling back to whatever
// `currentColor` happens to be inherited from — the two are different until
// every consumer applies `fgClass` explicitly.

import { COLOUR_TOKENS, type Colour } from "./contract/types";

export type ColourMeta = {
  /** e.g. "bg-clip-red" */
  fillClass: string;
  /** e.g. "text-clip-red-fg" */
  fgClass: string;
  /** Human-readable label, used as the colour swatch's accessible name. */
  label: string;
};

export const COLOUR_META: Record<Colour, ColourMeta> = {
  red: { fillClass: "bg-clip-red", fgClass: "text-clip-red-fg", label: "Red" },
  amber: { fillClass: "bg-clip-amber", fgClass: "text-clip-amber-fg", label: "Amber" },
  lime: { fillClass: "bg-clip-lime", fgClass: "text-clip-lime-fg", label: "Lime" },
  green: { fillClass: "bg-clip-green", fgClass: "text-clip-green-fg", label: "Green" },
  teal: { fillClass: "bg-clip-teal", fgClass: "text-clip-teal-fg", label: "Teal" },
  blue: { fillClass: "bg-clip-blue", fgClass: "text-clip-blue-fg", label: "Blue" },
  violet: { fillClass: "bg-clip-violet", fgClass: "text-clip-violet-fg", label: "Violet" },
  pink: { fillClass: "bg-clip-pink", fgClass: "text-clip-pink-fg", label: "Pink" },
  slate: { fillClass: "bg-clip-slate", fgClass: "text-clip-slate-fg", label: "Slate" },
};

/**
 * The default colour for a new clip (palette.md "Default"): the one token
 * that carries no category meaning, so an unassigned clip does not resemble
 * one the user deliberately coloured.
 */
export const DEFAULT_COLOUR: Colour = "slate";

// Assert at module load that every declared token has metadata and every
// metadata entry names a declared token, so a palette change that misses
// this file fails immediately rather than rendering an unstyled swatch.
for (const token of COLOUR_TOKENS) {
  if (!(token in COLOUR_META)) {
    throw new Error(`colour.ts: COLOUR_META is missing the "${token}" token.`);
  }
}
