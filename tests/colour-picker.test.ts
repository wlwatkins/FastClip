import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import ColourPicker from "../src/lib/components/ColourPicker.svelte";
import { COLOUR_TOKENS } from "../src/lib/contract/types";
import { COLOUR_META } from "../src/lib/colour";

// WP-05 definition of done: "each swatch renders its token's foreground
// colour on its own fill, not currentColor. Every pair in palette.md is
// verified to clear 4.5:1" (contrast itself is tests/palette-contrast.test.ts,
// WP-10's share) "and the picker is the first place that pairing is actually
// rendered." This file's job is narrower and specific to WP-05: prove each
// swatch applies its *own* token's fgClass to its own fill, not a shared or
// inherited one — that is what makes the checkmark's `stroke="currentColor"`
// resolve to the correct foreground rather than whatever surrounds it.
//
// Selection itself uses native <input type="radio"> (ColourPicker.svelte's
// own comment: "Tab, arrow-key and Space/Enter selection all come from the
// browser's own radio group behaviour"), which is a browser platform
// guarantee this suite does not re-implement — jsdom does not run real radio
// group arrow-key navigation, and firing a synthetic keydown here would
// prove handler wiring that does not exist (there is no keydown handler on
// this component), not real keyboard operation. See tests/titlebar-keyboard
// test's own note on the same limitation.

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ColourPicker: swatch identity", () => {
  it("renders exactly one radio input per palette token, each with an accessible name matching its label", () => {
    render(ColourPicker, { selected: COLOUR_TOKENS[0], onchange: () => {} });
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(COLOUR_TOKENS.length);
    for (const token of COLOUR_TOKENS) {
      expect(screen.getByRole("radio", { name: COLOUR_META[token].label })).toBeInTheDocument();
    }
  });

  it("every swatch's visible fill span carries its own token's fgClass, not a shared or hard-coded one", () => {
    const { container } = render(ColourPicker, { selected: COLOUR_TOKENS[0], onchange: () => {} });
    for (const token of COLOUR_TOKENS) {
      const radio = screen.getByRole("radio", { name: COLOUR_META[token].label });
      const swatch = radio.nextElementSibling as HTMLElement; // the aria-hidden fill span, per ColourPicker.svelte markup
      expect(swatch).not.toBeNull();
      expect(swatch.className).toContain(COLOUR_META[token].fillClass);
      expect(swatch.className).toContain(COLOUR_META[token].fgClass);
    }
    // Guard against every swatch collapsing onto one shared class by accident:
    // the fgClass set actually has as many distinct members as tokens.
    const distinctFg = new Set(COLOUR_TOKENS.map((t) => COLOUR_META[t].fgClass));
    expect(distinctFg.size).toBe(COLOUR_TOKENS.length);
    void container;
  });

  it("the checkmark svg on the selected swatch declares stroke=\"currentColor\", so it inherits that swatch's own fgClass rather than a fixed colour", () => {
    const selected = COLOUR_TOKENS[2];
    render(ColourPicker, { selected, onchange: () => {} });
    const radio = screen.getByRole("radio", { name: COLOUR_META[selected].label });
    const swatch = radio.nextElementSibling as HTMLElement;
    const svg = swatch.querySelector("svg");
    expect(svg).not.toBeNull();
    expect(svg?.getAttribute("stroke")).toBe("currentColor");
  });

  it("marks the currently selected token's radio as checked, and no other", () => {
    const selected = COLOUR_TOKENS[1];
    render(ColourPicker, { selected, onchange: () => {} });
    for (const token of COLOUR_TOKENS) {
      const radio = screen.getByRole("radio", { name: COLOUR_META[token].label }) as HTMLInputElement;
      expect(radio.checked).toBe(token === selected);
    }
  });

  it("selecting a different swatch (native radio change) calls onchange with that token", async () => {
    const onchange = vi.fn();
    const target = COLOUR_TOKENS[3];
    render(ColourPicker, { selected: COLOUR_TOKENS[0], onchange });
    const radio = screen.getByRole("radio", { name: COLOUR_META[target].label }) as HTMLInputElement;
    radio.click();
    expect(onchange).toHaveBeenCalledWith(target);
  });

  it("every radio is a real, unhidden-from-AT input reachable by Tab (visually hidden via sr-only, not display:none)", () => {
    render(ColourPicker, { selected: COLOUR_TOKENS[0], onchange: () => {} });
    for (const token of COLOUR_TOKENS) {
      const radio = screen.getByRole("radio", { name: COLOUR_META[token].label });
      expect(radio).not.toHaveAttribute("tabindex", "-1");
      expect(radio.className).toMatch(/sr-only/);
      radio.focus();
      expect(document.activeElement).toBe(radio);
    }
  });
});
