# Review 004 — WP-10 Palette and accessibility (G3)

**Reviewed:** `docs/src/product/palette.md`, `src/app.css`,
`src/lib/contract/types.ts`, `src/lib/contract/types.test.ts`,
`src-tauri/src/colour.rs`, `src-tauri/src/storage/{clips,recovery,store,schema}.rs`,
`src-tauri/tests/kill_mid_write.rs`, `tests/palette-contrast.test.ts`,
`tests/titlebar-keyboard.test.ts`, `vitest.config.ts`,
`src/lib/components/TitleBar.svelte`. Working tree at `72bbdc0` plus uncommitted
WP-10 work.

**Verdict:** ACCEPT

Gates verified by the orchestrator, not by the critic, which has no shell:
`cargo test --lib` 64 passed, `cargo fmt --check` clean, `cargo clippy
--all-targets -- -D warnings` clean, frontend suite 27 passed across 4 files.
There is no CI on this repository, so local runs are the only evidence.

## Findings

### F1 — no keyboard acceptance criterion survives the closing of WP-10 [major]

**Location:** `work/wp-10-palette.md:51` against `wp-05-crud.md`,
`wp-07-encryption.md`, `wp-14-settings.md`, `wp-12-release.md`
**Failure scenario:** WP-10 depends only on WP-04 and runs in parallel with
WP-05, so criterion 3 was satisfied against an empty `<main>` whose only
controls are two window-chrome buttons. WP-05 then adds the clip rows, the
create and edit forms, the delete confirmation and the colour picker; WP-07 the
PIN entry; WP-14 the settings toggle. None of those three definitions of done
mentioned the keyboard, and neither does spec §8. Concrete outcome: the colour
picker ships mouse-only and no gate in the plan fails.
**Why it survives scrutiny:** the specification carries no global requirement
for packages to inherit — spec mentions the keyboard three times, all
feature-specific, and §8 says nothing. WP-06 and WP-13 carry their own criteria
but cover only their own features. WP-10 is the only package that ever asserts
keyboard operability, and it asserted it over two buttons.
**Attribution:** specification layer, not this deliverable. `frontend-dev` and
`test-engineer` covered the surface that existed and said so plainly. Criterion
3 is **met-for-now**.
**Disposition:** fixed by the orchestrator before WP-05 was dispatched, since
the finding expires the moment WP-05 lands without it. WP-05, WP-07 and WP-14
now each require every control they add to be `Tab`-reachable, `Enter`/`Space`
operable, accessibly named, and to show a focus indicator distinct from hover.
WP-05 additionally requires each colour swatch to render its token's foreground
on its own fill rather than `currentColor` — the picker is the first place that
pairing is drawn, because the Rail layout puts label text on the row background
and never on a fill.

### F2 — the contrast suite's coverage is defined by a regex, not by `COLOUR_TOKENS` [minor]

**Location:** `tests/palette-contrast.test.ts:21`, `:86`
**Failure scenario:** the token class is `[a-z]+`, which cannot match an
underscore, and contract §1 permits multi-word `snake_case` tokens. A tenth
token `deep_blue`, added correctly to `COLOUR_TOKENS`, the Rust enum and
`app.css`, has both declarations skipped: the count stays 9, the `> 0` guard
passes, and a token whose contrast was never computed ships green. The same
hole swallows any non-hex declaration — one token retuned to `oklch()`, the
format Tailwind v4's own palette uses, vanishes while the other eight keep the
suite green. A token in `COLOUR_TOKENS` but missing from `app.css` is invisible
to every test in both suites.
**Why it survives scrutiny:** this is the only file in the repository that reads
`src/app.css`; the Rust cross-check compares Rust to `types.ts` and never
touches the theme. Tailwind does not error on a missing theme entry — it
silently declines to generate the utility.
**Disposition:** fixed by `test-engineer`. The class is now `[a-z_]+`, and
`tests/palette-contrast.test.ts:81` asserts the parsed set equals
`COLOUR_TOKENS`, so coverage is defined by the contract's token list rather than
by what the regex matched. Proved by deleting the two `--color-clip-slate`
declarations from `app.css` — exactly the "token forgotten in `app.css`" case —
and watching the set-equality assertion name `slate` as missing. Restored, 15/15
green.

### F3 — the four "activated with Enter/Space" tests assert nothing about keys [minor]

**Location:** `tests/titlebar-keyboard.test.ts:60-96`, comment at `:65-66`
**Failure scenario:** each fires `keyDown` and then an explicit
`fireEvent.click`, so the assertion rests entirely on the synthetic click —
delete the `keyDown` line and each passes identically. The comment claiming
jsdom dispatches a click for Enter on keydown is false. A regression this
cannot catch: an `onkeydown` handler calling `preventDefault()` on Enter stops
activation in a real browser while the suite stays green.
**Why it survives scrutiny:** the structural tests at `:32-46` do the real work
and are the right jsdom-available proxy. The defect is the naming — four test
names and the test report claim coverage that does not exist, which is the risk
`wp-10-palette.md:64` names.
**Disposition:** fixed by `test-engineer`. The four tests are collapsed into two,
named for what they check — handler wiring via a synthetic click — and the false
claim about jsdom dispatching a click for Enter is replaced by an accurate note
naming the exact regression the tests cannot catch. The structural tests are
unchanged, being the honest jsdom-available proxy. Net suite count fell from 27
to 26, which is two duplicative tests removed and one coverage assertion added,
not a regression.

### F4 — the palette page still declared itself unverified [minor]

**Location:** `docs/src/product/palette.md:124-131`
**Failure scenario:** the Verification section said the ratios were "not
measured by an automated test in this package" and marked the table
`PARTIALLY_VERIFIED`. That test landed in this package and passes. A reader at
WP-12 checking whether the palette is verified reads the page the work package
calls the deliverable, and either treats it as provisional or writes the test a
second time.
**Disposition:** fixed by `frontend-dev`. The section now names
`tests/palette-contrast.test.ts`, states what it recomputes and from where, and
separates requirement 2 (machine-verified) from requirements 1, 3 and 4
(argued by a human, not tested).

## Not findings, decided

**The Rust test that parses `src/lib/contract/types.ts` stays.** It is the only
mechanism anywhere checking the drift contract §1 calls silent. It reads and
does not write, so it is not a lane violation, and its dependence on
comma-splittable formatting is a false-positive risk rather than a false-negative
one — it fails loudly with the path and both lists printed.

**`resolve.conditions: ["browser"]` is the documented interop fix** and lives in
`vitest.config.ts` only; `vite.config.ts` sets no conditions, so `tauri build` is
unchanged.

**The `ImportReason` gap `backend-dev` reported is not a gap.** `ImportReason`
has no `too_long` and no `contains_control_characters` either, so `invalid_value`
is the catch-all for every field-value failure on import, and the developer's
proposed `import { reason: "invalid_value", field: "colour", index: n }` is
correct. One clarifying sentence in contract §1 `ExportFile` is a note for the
`architect` before WP-09, not a finding here.

## Failure layer

None for the package's output. F1 is specification (the work-package plan), F2
and F3 tests, F4 deliverable documentation.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Token table filled in, including the default | Yes | Nine rows with Fill, Foreground and Contrast, within the 8–10 required; `slate` named as default with reasoning, matched by `Colour::DEFAULT` and asserted in Rust |
| A contrast test passes for every token | Yes | `tests/palette-contrast.test.ts` recomputes from `src/app.css`, not from the table. Formula is WCAG 2 §1.4.3 exactly — 0.03928 threshold, 12.92 divisor, 2.4 exponent, 0.2126/0.7152/0.0722 coefficients — and its own describe block pins the formula at 21:1, 1:1 and symmetry. Caveat in F2 |
| Every control keyboard-reachable and operable | Met-for-now | Both controls are native `<button>`, both carry `aria-label`, neither has `tabindex="-1"`, both have a `focus-visible` outline distinct from hover. True for the surface that exists. See F1 |
| `"unset"` removed from all three, with a test | Yes | Absent from `types.ts`, `colour.rs` and `app.css`; asserted by three independent tests across two suites, including a Rust test that re-parses the TypeScript array so a reintroduction fails `cargo test` |

## What the critic checked and found sound

**No token is stored as hex** — the assigned check. `Colour::to_sql` binds
`as_str()`, a test asserts the stored column literally equals `"pink"`, the
column is `TEXT` in both the schema and the ratified storage design, and a
repository-wide grep for hex and `rgba()` across `src/` returns hits in
`src/app.css` alone.

**`backend-dev`'s "not a schema change" claim checks out.** The schema SQL
matches the ratified storage design line for line, `colour` is still
`TEXT NOT NULL`, `SCHEMA_VERSION` is still 1, and the bytes written are
identical to what a `&str` call site would have written. The type moved; the
disk did not.

**No data-loss path.** `Colour::from_stored` returns `storage` and repairs
nothing; a test asserts a row carrying `'unset'` produces `storage` **and** that
the row is still there afterwards. `Colour::DEFAULT` is documented as explicitly
not a fallback for an absent field.

**No disclosure.** The log line for an uninterpretable stored colour is a fixed
string with no interpolation. `ClipRow`'s hand-written `Debug` redacts `label`
and `value` to lengths. A test asserts the rejection error never echoes the
rejected token. `Colour` derives `Serialize` but deliberately not `Deserialize`,
which is the correct implementation of the lenient-argument rule.

**Token names do not collide with Tailwind's own `bg-clip-*` utilities** — none
is named `text`, `border`, `padding`, `content` or `ellipsis`. Worth knowing
before a tenth is chosen.

## What could not be verified

The critic has no shell: every test outcome above is the orchestrator's reported
run, not the critic's observation. `npm run typecheck` and `npm run check`
outcomes were not reported to it. One structural note it raised:
`tsconfig.json` includes `src` only, so the two new test files under `tests/`
are not typechecked — a WP-02 configuration condition, not this package's doing.

Palette requirements 1, 3 and 4 remain untested, as `test-engineer` disclosed
and as `palette.md` now states. Nothing in the toolchain renders pixels or
simulates colour-vision deficiency.

## Next action

orchestrator — landed. F1 and F4 fixed, F2 and F3 with `test-engineer`. The
`ImportReason` clarification goes to `architect` with WP-09.
