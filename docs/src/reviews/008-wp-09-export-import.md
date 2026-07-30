# Review 008 — WP-09 Export and import (G3)

**Reviewed:** `src-tauri/src/export_file.rs`, `src/commands/export_import.rs`,
`src/commands/wire.rs`, `src/storage/clips.rs`, `src/lib.rs`, `Cargo.toml`,
`capabilities/default.json`, `tests/{ipc,kill_mid_export}.rs`;
`src/lib/components/SettingsPanel.svelte`, `src/lib/ipc/commands.ts`,
`src/lib/contract/validate.ts`, `src/lib/{copy,errorMessage}.ts`;
`tests/{export-import,settings-panel-export-warning}.test.ts`.

**Verdict:** ACCEPT — 2 findings, neither a code defect in WP-09.

This package's objective says why it ships before encryption: *"the recovery path
that makes the encryption work safe to ship."* A forgotten PIN is unrecoverable by
design and this export is the only mitigation.

## The assigned check

**Is the plaintext design disclosed?** Yes — `src/lib/copy.ts:15`, rendered at
`SettingsPanel.svelte:520` in a view that replaces the settings body before `save()`
runs. Not softened.

**Does any code path write an export the user did not request?** No, traced
independently: `export_file::render` has one non-test caller, `export_import::export`
has one, `export_clips` is registered once, and `exportClips()` has one call site
behind the warning view. Corroborating negatives — the only non-test `File::create`
pairs in `src-tauri/src` are `settings.rs` and the export; `RunEvent::Exit` calls
`store.shutdown()` and nothing else; both `thread::spawn` sites are `#[cfg(test)]`.

## Findings

### F1 — the frontend test evidence no longer described the tree [major]

`SettingsPanel.svelte` grew from 243 lines to 570+ **during the review**, as WP-07's
frontend landed. The reported counts were measured against a component that no longer
existed, and this package's two frontend test files render it directly.
**Disposition:** discharged. Re-measured against HEAD: 152/152 across 17 files, and
the two WP-09 files passing 28/28. The critic's prediction held — but it bounded the
risk by reading rather than assuming, and stated plainly that both files still
passing was a prediction it could not observe.

### F2 — an export file with `version` below 1 is described with a false sentence [minor]

`export_file.rs:214` returned `unsupported_version` for any integer that is not 1,
which the frontend renders as *"This export file is from a newer version of
FastClip."* Version 0 is not newer.
**Why it survives:** a truthful, already-declared alternative exists —
`malformed_json` renders as *"This is not a FastClip export file."*
**Disposition:** routed to the architect, which chose `malformed_json` and **fixed
the rule that produced the gap**: "a version lower than the current one is migrated"
had no floor. The boundary is now *below the lowest version this build knows how to
read*.

> **Correction, 2026-07-30.** The disposition above is wrong as written. The
> architect did ratify the rule — `contract.md:330` and the section *A version below
> 1 is not a version problem* both say a lower value is `malformed_json` — but **the
> backend change was never made**. `export_file.rs:214` still returned
> `UnsupportedVersion` for any non-matching integer, high or low, so the contract and
> the code disagreed for two days while this file recorded the finding as closed.
>
> The orchestrator wrote "fixed" on the strength of the architect's report without
> reading the line the report was about. That is the **second** instance of the same
> error in this project; the first was claiming production code was intact after an
> agent crashed mid-task, having read the neighbouring function rather than the
> broken one. The lesson stated then applies unchanged: *"I checked" has to mean the
> specific thing, not the neighbourhood.*
>
> A finding is closed when the change is on disk, not when an agent says it will be.
> Routed to `backend-dev` on 2026-07-30.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Round trip preserves every clip | Yes | `export_import.rs:406` (wiped store), `ipc.rs:583` (through the seam, fresh ids) |
| A partially-valid import changes nothing | Yes | Bad record at index 3 across four shapes, store asserted unchanged; every truncation |
| Plaintext warning visible at export time | Yes | `export-import.test.ts:85-95` asserts `save` **and** `export_clips` both uncalled while the warning shows |
| Merge-only, fresh id, never deletes or overwrites | Yes | `clips.rs:259-310`; colliding ids produce distinct clips |
| A half-failed merge leaves the store untouched | Yes | `BEGIN IMMEDIATE`; an abandoned import leaves the store exactly as it was |
| `use_count` never exported | Yes | Absent from the wire `Clip`; imported clips start at zero |
| Export warning taken from the copy deck | Input did not exist | `copy.md` was "Not yet written" until WP-11. The *constraint* was satisfied |

## What the critic verified

**The `.part` cannot survive a failure.** All three post-write failure paths call
`discard`; failures before the write create no file; the handle is closed by
`write_and_sync` returning, which matters on Windows.

**The killed-process residue is bounded as the contract says.** `kill_mid_export.rs`
kills a real second process and asserts the *target* holds all 400 clips, with a
sentinel stopping the test passing by never having run.

**`preserve_order` is off and a test pins it.** `serde_json` has no `indexmap`
dependency; `export_file.rs:936` fails if a future dependency flips it.

**`tauri-plugin-fs` is inert.** It arrives transitively but `fs` appears nowhere in
`gen/`, and `init()` is never called.

## Two things dropped, one premise corrected

Dropped: `dialog:allow-ask` is ungranted anyway since capabilities are allow-lists;
and import reading the whole file into memory is contract-mandated.

Corrected, so it does not become precedent: `backend-dev` justified echoing an
unknown JSON key partly because it "is bounded". **A JSON object key is not
length-bounded.** The disclosure itself is compelled by §1 and spec §4.6 and stands;
the reasoning was unsound. The architect added a frontend truncation rule.

## Recorded

`test-engineer` could not complete one red-before-green proof — the environment
reverted `commands.ts`, outside its write scope — and reported the assertion as **not
red-proven** rather than claiming it. The critic closed it analytically: with a cast,
`{ exported: "3" }` produces `"Exported 3 clips."` and the `not.toMatch(/Exported/)`
assertion fails.

It also found a harness defect of its own: `vi.restoreAllMocks()` does not clear call
history for mocks created in a `vi.mock()` factory, so a `save` call leaked across
tests.

**A defect found later, in this package's frontend:** `exportClips` returned
`result as ExportResult` — an unvalidated cast on an IPC payload, the exact
pre-refactor defect the boundary layer exists to delete — while `importClips` eleven
lines below validated correctly. Caught by a test-engineer assertion and by the
compiler reporting `parseExportResult` as imported and never used. Fixed.
