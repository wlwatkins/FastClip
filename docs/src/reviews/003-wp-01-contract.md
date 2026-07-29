# Review 003 — WP-01 Contract ratification (G0b)

**Reviewed:** `docs/src/architecture/contract.md`, `storage.md`,
`adr/0007-list-order-representation.md`, `0008-use-count-stays-backend-side.md`,
`0009-durability-level.md`, `0010-manual-lock.md`, `adr/index.md`. Working tree
at `a2c4129`, uncommitted. ADR-0005 excluded — reviewed at WP-02's gate.

**Verdict:** ACCEPT

Four review rounds. Verdicts: `REWORK_ARCHITECTURE` (7 findings),
`REWORK_ARCHITECTURE` (6, one critical), `REWORK_ARCHITECTURE` (6), `ACCEPT` (4
carried).

## Findings — carried, not blocking

None is in a `BLOCK` category. None touches a command name, argument, payload
field, error variant or event. All four are under-specified failure branches in
`storage.md`'s mechanism page, which is backend-internal; the IPC contract that
both developers are dispatched against is unambiguous.

### F1 — startup recovery step 5's `keyfile` delete has no defined failure behaviour [major]

**Location:** `storage.md:227`, with the misdirected reference at `:302-304`
**Failure scenario:** classification `plaintext` with a stray `keyfile` held open
by a scanner or a second instance. Steps 1, 2, 6 and 7 each define their own
failure; step 5 does not. The idiomatic `?` makes startup record the fault, so
`get_lock_state` returns `storage` and the user sees a failure screen over an
intact plaintext store — and those messages point at import, which is how a user
replaces a store that was never damaged.
**Root cause:** an artefact of round three's F4 fix. The sweep moved from step 4
to step 2; line 265 was updated to follow it, line 304 was not, so the section
granting deletes non-fatality now names a step that deletes nothing.
**Carried to:** WP-03.

### F2 — the conversion abort deletes `clips.db.new` with no requirement to close step 3's connection [minor]

**Location:** `storage.md:648-655` against `:181-182`
**Failure scenario:** `disable_encryption` fails step 3's assertion — the case
round three's F2 fix strengthened step 3 to catch. Abort runs before step 4's
close, and on Windows deleting a file with a live SQLite handle fails with a
sharing violation, leaving a complete plaintext copy of every clip beside an
encrypted `clips.db` for the session.
**Why minor:** the natural Rust structure drops the `Connection` before the abort
runs. The document specifies this handle/delete interaction at step 5 and is
silent here.
**Carried to:** WP-07.

### F3 — abort is enumerated as reachable at steps 1–5; a failed rename at step 6 is a sixth [minor]

**Location:** `storage.md:648-649` against `:755-756`
**Failure scenario:** a second instance holds `clips.db`, `MoveFileEx` fails, and
a developer who sets the classification without checking the rename has
`encrypted` in memory over a plaintext store — which step 8's unconditional
emission then reports to settings.
**Why minor:** the realistic implementation skips steps 7–9 entirely. The harmful
branch needs a deliberately discarded `Result`. Two statements on disk disagree
about step 6.
**Carried to:** WP-07.

### F4 — `lock` step 5's tray rebuild has no defined outcome, while steps 3 and 4 do [minor]

**Location:** `storage.md:518-520`, `:530-535`; `adr/0010-manual-lock.md:59-65`
**Failure scenario:** both available behaviours break a stated rule. Absorb, and
the tray keeps listing clip labels over a locked store — acceptance criterion 10
breached in the one surface the contract says breaches it exactly as the window
would. Report, and the only route is `internal`, contradicting ADR-0010's "Once
locking begins it cannot fail".
**Stated as a prediction:** whether Tauri 2's tray menu setter is fallible was not
observable. If it is infallible, this finding is void.
**Carried to:** WP-03.

## Failure layer

architecture — all four in the mechanism page and ADR, not in the IPC contract.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| Contract §6 is empty | Yes | `contract.md:1415-1421` reads "None." The two open items (SQLite crate, palette tokens) each affect no command name, argument, payload field, error variant or event, and each names its owning package. |
| Every command lists its error variants | Yes | All sixteen commands have an Errors row in their own section; `internal` factored out once rather than repeated. |
| An ADR exists for order representation | Yes | ADR-0007, Accepted, four alternatives named, dense `0..N-1` invariant carried into `storage.md:115-154`. |
| No contradiction with the specification | Yes, with two declared divergences | `contract.md:942-947` tabulates manual lock and the five-minute backoff ceiling. Both are owner decisions escalated as spec edits. Spec §§3, 4.1–4.8, 5 and 8 checked line by line; no undeclared divergence. |
| No `TBD` | Yes | One hit across `docs/src`, in WP-01's own description of this check. |
| Every command's error set covers its reachable conditions | Yes | The criterion that failed all three previous rounds. `disable_encryption` and `change_pin` now carry `unsupported_version { key_material }`; `export_clips`'s readback declares `io`; the conversions' `crypto { corrupt }` is reconciled at `contract.md:466-481`. |

## The three "did a fix open a hole" checks

This package produced a new defect from a fix twice. All three came back clean:

- **The unconditional step 8 emission tells the frontend nothing false.** The
  rename at step 6 is the instant the header changes its answer, and the payload
  describes the store as it then is. On enable with a failed step 7,
  `{ encryption_enabled: true, locked: false }` is true — the store is encrypted
  and the DEK is live.
- **The sweep at step 2 reintroduces no protected deletion.** `keyfile.new` is
  never the only key material: enable writes it while the store is still
  plaintext, and `change_pin` renames over a `keyfile` still holding a working
  wrap under the old PIN. The destructive pair remains gated on a positive
  classification.
- **Neither readback adds an undeclared variant.** Every reachable failure of the
  enable step 1 readback lands on `crypto { bad_key_material }`, which is
  declared.

## Residual risks — recorded, not raised

Each misleads a reader without changing behaviour.

- `enable_encryption` is now a fourth reader of `keyfile`, while
  `contract.md:835-843` still says "There are three". The variant is unreachable
  — the process wrote the file microseconds earlier — so the error set is
  adequate. Notable as an instance of the class the sweep was run for, created by
  the sweep's own fix.
- `storage.md:793-801` claims every failure message points at export and import
  "except the last"; false for four of nine rows. `contract.md:1286` already
  resolves it in the safe direction.
- `contract.md:1164` is falsified eight lines later by its own second bullet, and
  "on disk" is wrong for `lock`, which writes nothing.
- The classification stays `absent` for the whole first-run session after step 6
  creates the database. Every consumer traced maps it correctly.

## What could not be verified

No code exists for any of this; every finding is a prediction about what an
implementation will do with the text. The critic has no shell. Tauri 2's tray
setter fallibility (F4) and Windows `DeleteFile`/`MoveFileEx` sharing semantics
(F2, F3) are asserted from knowledge, not observation. The copy deck is unwritten,
so its conflicting instructions could be checked but not its sentences.

## Landing conditions

Carried into dispatch briefs rather than fixed in a fifth round: **F1 and F4 to
WP-03, F2 and F3 to WP-07**, as sentences the `architect` writes into
`storage.md` when those packages are dispatched.

**Open with the owner, not resolvable by any agent:** spec §4.8 needs two edits —
manual lock, which it does not describe, and the five-minute backoff ceiling,
which contradicts its uncapped exponential from the ninth wrong PIN.

## Next action

orchestrator — land the gate.
