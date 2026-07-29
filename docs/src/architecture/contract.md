# IPC contract

**Status:** Ratified at G0b, amended in the second round after the owner
answered the escalations. No open questions remain.
**Owner:** `architect`. Nobody else edits this page.

The only agreed interface between the Svelte frontend and the Rust backend.
`frontend-dev` and `backend-dev` implement against this page, not against each
other's code.

Derived from the [specification](../product/spec.md). If they disagree, this
page is the defect.

A developer who finds this page ambiguous stops and reports it. That report is a
defect in the architect's work, and it is worth more than a sensible guess.

## 0. Wire rules

These apply to every command, every event and every type below. Nothing here is
inferred from an attribute; the attributes are listed as a way to produce the
stated wire form, not as its definition.

| Rule | |
| ---- | --- |
| Casing | **`snake_case` everywhere, in both directions, without exception.** Argument names, payload field names, event names, error `kind` values and error string enums. There is no `camelCase` anywhere on the wire, including in the generated TypeScript types. |
| Argument shape | Tauri delivers a command's arguments as the named keys of one JSON object. Arguments are therefore **flat named keys**, never nested under a wrapper. `invoke("delete_clip", { clip_id: "…" })`, not `invoke("delete_clip", { args: { clip_id: "…" } })`. An argument whose value is itself an object — `clip` — is named and carries the object directly. |
| Producing it in Rust | `#[tauri::command(rename_all = "snake_case")]` on every command. **No struct field is ever renamed** — every serialised struct uses its Rust field names unchanged, and `#[serde(rename_all = …)]` appears on no struct in this contract. Enum *variant* names are a separate matter: Rust variants are `PascalCase` and the wire is `snake_case`, so `Colour` and every enum in [`ClipError`](#4-errors) carry `#[serde(rename_all = "snake_case")]` for their variants only. |
| Void | A command that returns nothing returns `Result<(), ClipError>`, which reaches JavaScript as a resolved promise carrying `null`. |
| Failure | A command that fails rejects with a **`ClipError` object** ([§4](#4-errors)), never a string. A rejection that is not a `ClipError` object is a backend defect; the frontend maps it to `{ "kind": "internal" }`. |
| Validation | Every payload crossing the boundary is validated on arrival, on both sides. The frontend never casts (`payload as Clip[]` is the specific defect [WP-04](../work/wp-04-frontend-scaffold.md) exists to prevent recurring). |
| Rejecting bad arguments | **A command's arguments must not fail to deserialise.** Tauri turns a deserialisation failure into a rejection carrying a plain string, which no `ClipError` can be recovered from, so every argument type is declared leniently and validated in the command body. See [Argument deserialisation](#argument-deserialisation). |
| Character counts | Every length in this contract is counted in **Unicode scalar values** — Rust `s.chars().count()`, TypeScript `[...s].length`. Not bytes, and **not** JavaScript's `s.length`, which counts UTF-16 code units and would disagree with the backend on any character outside the basic multilingual plane. |
| UUIDs | Lowercase hyphenated, 36 characters. The backend emits lowercase. Input is parsed case-insensitively and stored lowercase. |

## 1. Data types

### `Clip`

The clip as it crosses the seam, in both directions.

| Field | Rust | TypeScript | Constraint |
| ----- | ---- | ---------- | ---------- |
| `id` | `Uuid` | `string` | hyphenated UUID, backend-minted, stable for the clip's life |
| `label` | `String` | `string` | 1–100 characters; no control characters |
| `value` | `String` | `string` | 1–10 000 characters |
| `colour` | `Colour` | `Colour` | a [palette token](../product/palette.md), never hex |

```ts
type Clip = { id: string; label: string; value: string; colour: Colour };
```

There is no order field. Order is the array index
([ADR-0007](./adr/0007-list-order-representation.md)).

There is no `use_count`. It exists, it is backend-owned, and it never crosses
the seam ([ADR-0008](./adr/0008-use-count-stays-backend-side.md)).

`icon`, `visible` and `clear_time` are removed and must not appear in any new
type. There is no migration path
([ADR-0003](./adr/0003-no-legacy-migration.md)), so an unknown field is
`invalid_input { field, reason: "unknown_field" }` and never ignored. How that
rejection is produced is in
[Argument deserialisation](#argument-deserialisation) — **not** with
`#[serde(deny_unknown_fields)]`, which produces the wrong error shape.

### Argument deserialisation

Tauri deserialises a command's arguments **before** the command body runs. A
failure there never reaches our code: it rejects with a plain string such as
``invalid args `clip` for command `create_clip`: unknown field `id` ``, which
the frontend can only map to `internal` ([§4](#4-errors)). Three of the declared
`InvalidReason` values — `required`, `unknown_field` and `not_permitted` —
describe conditions that a strict argument type would swallow exactly this way.

So **argument types are lenient and the command body validates**. This applies to
**every argument of every command**, not only to the ones whose type is a struct.

#### Inside a struct argument — `ClipDraft`, `Clip`, `ExportFile`

- Every declared field is `Option<T>`. A missing field deserialises to `None`
  and becomes `invalid_input { field, reason: "required" }`.
- `colour` is received as a `String` and resolved to a
  [`Colour`](#colour) by hand, so an unknown token is
  `not_a_palette_token` rather than a serde enum failure.
- Unknown fields are captured, not rejected, with
  `#[serde(flatten)] extra: serde_json::Map<String, serde_json::Value>`. A
  non-empty `extra` is `invalid_input` naming **the lexicographically smallest
  key** in it: `id` and `use_count` give `not_permitted`, anything else gives
  `unknown_field`. `deny_unknown_fields` cannot be combined with `flatten`, and
  is not wanted here in any case.

  "Lexicographically smallest" is stated because `serde_json::Map` is a
  `BTreeMap` **only while the crate's `preserve_order` feature is off**, and with
  it on the map becomes an `IndexMap` and the same payload names a different
  field. That feature must stay off, and a dependency that enables it changes
  this contract's observable behaviour without touching this page. The worked
  consequence: `{ "use_count": 1, "icon": "x" }` reports
  `invalid_input { field: "icon", reason: "unknown_field" }` — never
  `use_count`/`not_permitted`, because `icon` sorts first. Both keys are
  rejections and only a frontend that bypassed its generated types can send
  either, so which one is named matters less than its being decided here rather
  than by a transitive feature flag.

#### The argument itself — every command

**Every command parameter is declared `Option<T>`, including the scalars and the
array.** `clip_id: Option<String>`, `pin: Option<String>`,
`enabled: Option<bool>`, `order: Option<Vec<String>>`, `clip: Option<ClipDraft>`,
and so on. An absent key deserialises to `None` and the body returns
`invalid_input { field, reason: "required" }`, where `field` is the argument's
wire name.

Without this, the ten commands whose argument is a scalar or an array have no
accurate answer for a missing one, and `create_clip` and `update_clip` have none
for a missing `clip`.
`invoke("set_always_on_top", {})` would fail inside Tauri and reject
with a plain string; the frontend would map it to `internal`, which
[§0](#0-wire-rules) forbids for a modelled condition, and the command's declared
error set — `storage` alone — would be a false description of what happened.
`invoke("delete_clip", {})` is the same shape: an absent `clip_id` is not
`malformed_uuid`, and pretending it is would send the user a message about a
malformed identifier they never sent.

An absent key is not exotic. `JSON.stringify` drops a key whose value is
`undefined`, so any frontend bug that lets a variable go undefined produces
exactly this call.

**`invalid_input { reason: "required" }` is therefore in the declared error set of
every command that takes an argument.** The per-command tables below list it.

[ADR-0003](./adr/0003-no-legacy-migration.md) says strict deserialisation "is
now available" and names `deny_unknown_fields`. Strictness is delivered in full
— an unknown field is still an error and never ignored — by a mechanism that can
report which field it was. The ADR's decision stands; only the attribute
changes.

One condition remains outside this scheme: an argument or field carrying the
**wrong JSON type**, such as `label: 42`, `clip_id: 7`, or an `order` array with
a number in it. It is not modelled, it reaches the frontend as `internal`, and
that is correct — unlike an absent key, it is only reachable by a frontend that
has bypassed its own generated types, which is a defect rather than a user-facing
failure.

The line between the two is drawn there because a `serde_json::Value` parameter
per argument would push type checking into every command body to model a
condition no correct frontend can produce. **Absent is modelled; wrongly typed is
not.**

The same lenient-then-validate rule applies to the
[`ExportFile`](#exportfile) parsed by `import_clips`, for the same reason: the
`import` variant carries a `field` name, which serde's own error text would only
give up as a substring.

### `ClipDraft`

A clip before it has identity. The argument to `create_clip`.

```ts
type ClipDraft = { label: string; value: string; colour: Colour };
```

Identical to `Clip` minus `id`. An `id` present in a `ClipDraft` is
`invalid_input { field: "id", reason: "not_permitted" }` — the backend mints
every id ([§5](#5-closed-questions)).

### `Colour`

A **closed enum** in Rust and a string union in TypeScript. Its variants are
exactly the entries of the Token column in
[the palette table](../product/palette.md), lowercase ASCII, `snake_case` if a
token name has more than one word.

**The token list is one generated fact with two consumers.** In TypeScript it is
an array, and the type is derived from the array rather than written twice:

```ts
export const COLOUR_TOKENS = ["…"] as const;      // one entry per palette token
export type Colour = (typeof COLOUR_TOKENS)[number];
```

The boundary validator checks membership of `COLOUR_TOKENS`; nothing else
enumerates a colour. In Rust the same list is the closed enum's variants.

An unrecognised token is `invalid_input { field: "colour", reason: "not_a_palette_token" }`.

Decided against a validated `String`: a `String` moves the check to every call
site and lets an unknown token reach storage, where nothing can render it. The
named cost — adding a colour becomes a change to a type rather than to data — is
correct here, because [spec §7](../product/spec.md#7-colour) closes the set
deliberately and adding to it should be a reviewed act.

Decided against writing the union out as literals with the array beside it. Two
declarations of one list drift, and the drift is silent: the type would accept a
token the validator rejects.

**Sequencing, corrected.** The Token column on
[the palette page](../product/palette.md) is empty, filling it is
[WP-10](../work/wp-10-palette.md)'s deliverable, and the owner has deferred the
choice of tokens to that package.

An earlier version of this section said the first package to break was
[WP-05](../work/wp-05-crud.md), the first in which a `Colour` crosses the seam.
That was wrong. **[WP-04](../work/wp-04-frontend-scaffold.md) breaks first**,
because it generates the TypeScript types and the boundary validator from this
page. With an empty list, `Colour` degrades to `string`, the validator accepts
`colour: "chartreuse"`, and WP-04 has shipped the exact unchecked-payload defect
it exists to prevent — without failing anything, which is the worst form of it.
WP-04 also declares the tokens as a Tailwind theme extension, and there are none
to declare.

**What WP-04 generates in the interim.** `COLOUR_TOKENS` holds exactly one
provisional entry, `"unset"`, and everything else is built normally: the derived
union, the validator, the Tailwind theme entry, the Rust enum variant. No *code*
changes shape when the palette arrives — WP-10 replaces the contents of that one
array and the corresponding Rust variants, and deletes `"unset"`.

Three constraints on the provisional token, without which it becomes the palette
by default:

- It is **not** a colour name, so it cannot be mistaken for a palette entry or
  quietly kept.
- **No build ships with it.** [WP-12](../work/wp-12-release.md) gates on WP-10,
  and a `"unset"` surviving into a release is a `BLOCK`-level finding.
- Its rendered appearance is not designed. It exists so the type system has a
  member, not so a clip looks right before the palette exists.

**Existing data does change, and only a developer is holding any.** Every clip
created by a WP-04 to WP-09 build stores `colour = 'unset'`. The moment WP-10
deletes that variant, the closed enum cannot parse those rows, and
[`list_clips`](#list_clips) declares only `locked` and `storage` — so a developer
whose dev store predates WP-10 gets an unreadable list with no variant that says
why.

The remedy is to **delete the whole `~/.fast-clip/` directory when WP-10 lands**.
Not `clips.db` alone: a stale `clips.db-wal` beside a freshly created database is
a mismatched sidecar, which is the hazard the conversions avoid by deleting
sidecars before the rename ([storage](./storage.md#switching-encryption-on-and-off))
and which a partial delete would reintroduce by the back door. Anything that
removes the store handles all three files
([storage](./storage.md#required-properties)); naming the directory is the way to
get that right without listing them. It is a development store containing test
clips, and there is no user data anywhere in the world at that point. No error variant is added for this: a variant modelling
a condition that exists for the length of one work package would outlive it by
the life of the product.

This is also the general rule for a token ever being **removed** after release,
which is a different thing entirely: that is a stored value the running build
cannot interpret, so it is a schema change, and it goes through
`PRAGMA user_version` and a migration that rewrites the affected rows
([versions on disk](#versions-on-disk)). Adding a token is not a schema change;
removing one is.

This breaks the dependency between WP-04 and WP-10 in the only direction
available to the architect: WP-04 stops needing WP-10's output. Which package
runs first, and whether the work index changes, is the orchestrator's to settle
with the owner.

[WP-03](../work/wp-03-storage.md) is not blocked either way: the store holds the
token as `TEXT` and does not enumerate it.

### `LockState`

```ts
type LockState = {
  encryption_enabled: boolean;
  locked: boolean;
  attempts_remaining: number | null;
  retry_after_ms: number | null;
};
```

| Field | Meaning |
| ----- | ------- |
| `encryption_enabled` | The store is encrypted ([ADR-0004](./adr/0004-optional-pin-encryption.md)). |
| `locked` | Encryption is on and the store is not open — either the PIN has not been entered since launch, or the user called [`lock`](#lock). `encryption_enabled: false` implies `locked: false`; the reverse combination is a backend defect. |
| `attempts_remaining` | Unlock attempts before backoff engages, counting down from 5. `null` when `locked` is false. |
| `retry_after_ms` | Milliseconds until the next unlock attempt is permitted, or `null` when none is pending. |

`retry_after_ms` is a **snapshot at the moment of emission**. The frontend counts
down locally from receipt and re-enables the PIN input at zero. The backend
re-checks on the next `unlock` call and is authoritative: a frontend whose clock
runs fast gets `backoff` again.

The failed-attempt count and the backoff deadline are persisted
([storage](./storage.md)), so restarting FastClip does not clear them.

### `Settings`

```ts
type Settings = { always_on_top: boolean };
```

Readable and writable while locked. It contains no clip data, and
[spec §4.4](../product/spec.md#44-always-on-top) requires the toggle to apply at
launch, which is before any PIN has been entered.

### `ExportResult` and `ImportResult`

```ts
type ExportResult = { exported: number };
type ImportResult = { imported: number };
```

A count, so the user can be told how many clips moved. Both are zero-or-more; an
export of an empty store and an import of an empty file both succeed.

### `ExportFile`

The on-disk shape written by `export_clips` and read by `import_clips`. It is a
file format, not an IPC payload, and it is versioned separately from everything
else ([below](#versions-on-disk)).

```json
{
  "format": "fastclip-export",
  "version": 1,
  "clips": [
    { "id": "8a1f…", "label": "Support greeting", "value": "Hello, …", "colour": "amber" }
  ]
}
```

| Field | Rule |
| ----- | ---- |
| `format` | Exactly `"fastclip-export"`. Anything else is `import { reason: "malformed_json" }`. |
| `version` | Integer. This build writes and accepts `1`. A higher value is `import { reason: "unsupported_version" }`. |
| `clips` | Array. May be empty, which imports nothing and succeeds. |
| `clips[].id` | **Written on export, ignored on import.** Import mints a fresh id for every clip ([spec §4.6](../product/spec.md#46-export-and-import-json)), so two clips in one file carrying the same id produce two distinct clips. It may be absent. |
| `clips[].label`, `value`, `colour` | Required, validated exactly as in [§1](#1-data-types). |
| `clips[].use_count` | **Never written**, and rejected on import as an unknown field. [Spec §4.6](../product/spec.md#46-export-and-import-json): importing another machine's counts would corrupt the ranking. |

Any other field on a clip, or at the top level, is
`import { reason: "unknown_field", field, index }`.

### Versions on disk

Four artefacts are versioned independently, from the first release. Each has one
owner and one failure surface.

| Artefact | Where the version lives | Current | Failure |
| -------- | ----------------------- | ------- | ------- |
| Database schema | `PRAGMA user_version` inside the database | `1` | `unsupported_version { component: "schema" }` |
| Key material | A single leading byte of `~/.fast-clip/keyfile`, outside the DPAPI blob | `1` | `unsupported_version { component: "key_material" }` |
| Export file | The `version` field of [`ExportFile`](#exportfile) | `1` | `import { reason: "unsupported_version" }` |
| Settings | The `version` field of `~/.fast-clip/settings.json` | `1` | **None.** The file is ignored and the default used. |

The settings file is the one artefact whose unrecognised version is **not** an
error. `component` has no `"settings"` value and
[`get_settings`](#get_settings) cannot return `unsupported_version`. An
unrecognised version there is treated exactly as a missing or truncated file:
`{ always_on_top: false }`, the app starts, and the next
[`set_always_on_top`](#set_always_on_top) overwrites the file at version 1.
Refusing to start over one boolean would be a worse failure than losing it, and
there is nothing in that file to preserve
([question 19](#closed-in-the-second-round-at-g0b)).

The database schema version is `PRAGMA user_version` rather than a migrations
table because it is a header field readable in one statement, before anything is
known about the schema. A migrations table must itself be found before it can be
read, and duplicates a fact SQLite already stores.

**A version can only be checked after the file can be read.** With encryption
on, `PRAGMA user_version` is inside the encrypted database, so the sequence is:
unwrap the key material (failure → `bad_pin` or `crypto`), open the database
(failure → `crypto { reason: "corrupt" }`), read `user_version` (too high →
`unsupported_version`). `unsupported_version { component: "schema" }` therefore
never coexists with a decryption failure, and the two can never be confused.

With encryption **off** there is nothing to unwrap: startup recovery opens the
database and reads `user_version` before any command is served, and both failures
reach the frontend from `get_lock_state`
([opening the database](#opening-the-database)).

A version **lower** than the current one is migrated, not rejected. There is
nothing to migrate at version 1; the mechanism exists from the first release so
that there is one when there is
([ADR-0005](./adr/0005-sqlite-store.md)). This is unrelated to
[ADR-0003](./adr/0003-no-legacy-migration.md), which refuses to read
pre-refactor data at all.

## 2. Commands

Sixteen commands, all registered in `invoke_handler!` in `src-tauri/src/lib.rs`
under exactly the names below.

| Command | Arguments | Returns | Mutates | Emits | Works while locked |
| ------- | --------- | ------- | ------- | ----- | ------------------ |
| [`list_clips`](#list_clips) | — | `Clip[]` | no | — | no |
| [`create_clip`](#create_clip) | `clip: ClipDraft` | `null` | yes | `update_clips` | no |
| [`update_clip`](#update_clip) | `clip: Clip` | `null` | yes | `update_clips` | no |
| [`delete_clip`](#delete_clip) | `clip_id: string` | `null` | yes | `update_clips` | no |
| [`copy_clip`](#copy_clip) | `clip_id: string` | `null` | yes | — | no |
| [`reorder_clips`](#reorder_clips) | `order: string[]` | `null` | yes | `update_clips` | no |
| [`export_clips`](#export_clips) | `path: string` | `ExportResult` | no | — | no |
| [`import_clips`](#import_clips) | `path: string` | `ImportResult` | yes | `update_clips` | no |
| [`get_lock_state`](#get_lock_state) | — | `LockState` | no | — | yes |
| [`unlock`](#unlock) | `pin: string` | `null` | yes | `lock_state`, `update_clips` | yes |
| [`lock`](#lock) | — | `null` | yes | `lock_state` | yes |
| [`enable_encryption`](#enable_encryption) | `pin: string` | `null` | yes | `lock_state` | yes |
| [`disable_encryption`](#disable_encryption) | `pin: string` | `null` | yes | `lock_state` | no |
| [`change_pin`](#change_pin) | `current_pin: string`, `new_pin: string` | `null` | yes | — | no |
| [`get_settings`](#get_settings) | — | `Settings` | no | — | yes |
| [`set_always_on_top`](#set_always_on_top) | `enabled: boolean` | `null` | yes | — | yes |

"Works while locked: no" means the command returns `locked` whenever
`LockState.locked` is true, before doing anything else
([spec §4.8](../product/spec.md#48-encryption-and-unlocking)). The frontend
handles `locked` on **every** such call, not only at launch — see
[§4](#when-locked-is-reachable).

Each such command checks lock state **twice**: on entry, and again after it
acquires the store's connection lock. The second check is what makes a
concurrent [`lock`](#lock) produce `locked` rather than a torn write or an
`internal` ([storage](./storage.md#locking-on-demand)). It belongs in one shared
guard, not in the ten command bodies marked "no".

`enable_encryption` is marked "yes" because it never returns `locked`. It cannot
usefully be called while locked either, since the store is then already
encrypted: it returns `wrong_state { required: "unencrypted" }`.

`lock` is marked "yes" for a different reason: called on an already-locked store
it succeeds and changes nothing ([below](#lock)).

**This list is exhaustive.** [Search](../product/spec.md#47-search) is the one
specified feature with no command and no event: it filters the list already in
memory, and a round trip per keystroke is wrong for a tool whose value is speed.
A backend change that appears necessary for search means the design is wrong —
stop and escalate ([WP-13](../work/wp-13-search.md)).

`internal` ([§4](#4-errors)) may be returned by any command and is omitted from
the per-command error lists below.

### Renamed from the pre-refactor surface

| Was | Is | Why |
| --- | -- | --- |
| `get_clips` | `list_clips` | The surface grows from four commands to sixteen. One verb scheme — `list`, `create`, `update`, `delete`, `copy`, `reorder`, `export`, `import`, `get`, `set`, `lock` — is worth a rename made before any code is written against it. |
| `new_clip` | `create_clip` | As above, and its argument type changes to `ClipDraft` regardless. |
| `del_clip` | `delete_clip` | As above. The abbreviation saves three characters and costs a reader a guess. |

Nothing is renamed to minimise a diff, because there is no diff to minimise:
[WP-03](../work/wp-03-storage.md) replaces the storage layer and
[WP-05](../work/wp-05-crud.md) reimplements every command.

### Resolved defects in the pre-refactor surface

| Defect | Resolution |
| ------ | ---------- |
| `del_clip` takes a bare `clip_id` rather than a wrapped object | **Justified, not fixed.** Tauri already delivers arguments as the named keys of one object, so `clip_id: String` produces `{ "clip_id": "…" }` on the wire. Wrapping it in a struct would nest the payload one level deeper for nothing. See the argument-shape rule in [§0](#0-wire-rules). |
| `new_clip` and `update_clip` share `insert_or_update_clip`, so an update on an unknown id silently creates a clip | Fixed. Two commands, two behaviours; `update_clip` on an unknown id is `not_found`. Open question 1, closed in [§5](#5-closed-questions). |
| `new_clip` accepts a client-supplied `id` | Fixed. `create_clip` takes a `ClipDraft`, which has no `id` field at all. |
| Commands return `Result<_, tauri::Error>`, opaque to the frontend | Fixed. Every command returns `Result<_, ClipError>` ([§4](#4-errors)). |

### Opening the database

Two failures can only be discovered by opening the clip database:
`crypto { reason: "corrupt" }` and
`unsupported_version { component: "schema" }`.

**Discovery and reporting are separate questions**, and an earlier version of
this section answered only the first while writing rules as though it had
answered both. That produced a sentence the backend cannot honour, corrected
[below](#the-sentence-that-was-wrong).

#### Where the fault is discovered — one place

| Encryption | The database opens | Discovered by |
| ---------- | ------------------ | ------------- |
| Off — the default | During startup recovery, before any command is served. The backend opens it there regardless, because that is where it creates the database when it is absent ([storage](./storage.md#startup-recovery)). | startup recovery |
| On | At [`unlock`](#unlock). The DEK does not exist until a PIN unwraps it, so nothing earlier can open the database. | `unlock` |

This is the property worth having and it is unchanged: **the fault is one value,
established once.** No command opens the store because it found it closed, so no
two commands can discover different faults, and there is a defined point at which
the store is known good.

#### How it reaches the caller — from wherever the caller asks

Startup recovery does not fail the launch. It **records** the fault and leaves
the store with no connection ([storage](./storage.md#startup-recovery)), so that
there is a window in which to show a message. Every command that needs the store
then passes the same guard, in this order:

1. Is the store locked? → `locked`.
2. Is there a recorded fault? → **return it verbatim.**
3. Otherwise use the connection.

So a recorded open-time fault is **relayed by every command that needs the
store**, and each of them declares `crypto` and `unsupported_version`
accordingly. It is relayed, never rediscovered: every report carries the
identical value, because there is only one.

"Needs the store" is the ten commands marked "works while locked: no", **plus
[`enable_encryption`](#enable_encryption)**, which has to read the plaintext
store in order to convert it. That command is marked "yes" because it never
returns `locked`, which is a different question from whether it needs an open
database — and the two are easy to conflate, which is why they are separated
here. [`lock`](#lock), [`get_settings`](#get_settings) and
[`set_always_on_top`](#set_always_on_top) need no connection at all and relay
nothing: `lock` on an encrypted store is a no-op success, and the settings
commands read a different file.

`get_lock_state` is still the command the [startup sequence](#startup-sequence)
meets it at first, and that is a sequencing fact rather than an exclusivity one.
A frontend that follows the sequence sees the fault there and stops. A frontend
that does not, or that had a command in flight, gets the same value from that
command instead of an undeclared rejection.

#### The sentence that was wrong

This section previously said *"A `list_clips` returning `unsupported_version` is
a backend defect"*, and the [startup sequence](#startup-sequence) repeated it.

**The backend cannot honour that.** `list_clips` invoked on a store that never
opened must return some `ClipError`; the page declared none for the case, so
every available implementation was a contract violation. Calling something a
backend defect is only legitimate when the backend can avoid doing it.

The reasoning that produced it is worth naming, because it is the class of
mistake and not the instance: this section had rejected *"letting every
clip-touching command declare the open-time variants in case it happened to be
first"*. That rejection was correct about commands **opening on demand** and each
discovering a fault independently — which is what defines no point at which the
store is known good. It does not apply to relaying one recorded value. One
sentence was carrying both meanings, and the wrong one won.

Two alternatives were available and both are rejected:

| Alternative | Rejected because |
| ----------- | ---------------- |
| Map a recorded open-time fault to `storage` | It is untrue for a too-new schema, and it discards the `crypto` sentence the [copy deck](../product/copy.md) requires to point at import as the recovery path. It is also the mapping the conversion paragraph below argues against, so the page would have prescribed it in one place and forbidden it in another. |
| Return `locked` so the frontend shows the PIN prompt | There is no PIN. It puts a prompt the user cannot satisfy over a plaintext store. |

#### The conversions, and the same mechanism

[`enable_encryption`](#enable_encryption) and
[`disable_encryption`](#disable_encryption) also declare
`crypto { reason: "corrupt" }`, and both must. They open databases as part of the
work they were asked to do — `disable_encryption` has to read the encrypted store
in order to write a plaintext one, and each reopens the converted database before
returning ([storage](./storage.md#switching-encryption-on-and-off)). A failure
there is a failure of the requested operation and is reported as what it is.
**Do not map it to `storage`.**

A conversion whose post-commit reopen fails **records a fault in the same way
startup recovery does** — `storage`, because the database was verified moments
earlier and what failed is access rather than content
([storage](./storage.md#the-reopen-and-why-it-is-verified-first)). The guard
above then relays it. One mechanism, one rule, no special case: the backend holds
at most one recorded fault, and every command that needs the store returns it
until the next launch clears it.

---

### `list_clips`

Every clip, in display order.

```ts
const clips: Clip[] = await invoke("list_clips");
```

| | |
| --- | --- |
| Arguments | none |
| Returns | `Clip[]`, complete, in display order. Empty array when there are no clips — not an error. |
| Mutates | no |
| Errors | `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

**`crypto` and `unsupported_version` never *originate* here** — this command
never opens the store. They are relayed: startup recovery or a failed conversion
recorded a fault, and the guard every clip-touching command shares returns it
verbatim ([opening the database](#opening-the-database)). A read that fails after
the database is open is `storage`.

### `create_clip`

```ts
await invoke("create_clip", { clip: { label, value, colour } });
```

| | |
| --- | --- |
| Arguments | `clip: ClipDraft` |
| Returns | `null` |
| Mutates | yes. The clip is appended at the end of the list; its `position` is the current maximum plus one, and `use_count` starts at 0. |
| Emits | `update_clips` |
| Errors | `invalid_input`, `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

It returns `null` rather than the created `Clip`. The `update_clips` event is
the only path by which list state reaches the frontend, so there is exactly one
place to apply it and no possibility of applying a return value and an event out
of order. The cost is that the frontend cannot focus or highlight the new row;
nothing in the specification asks it to.

`invalid_input` reasons reachable here: `required`, `too_long`,
`contains_control_characters`, `not_a_palette_token`, `not_permitted` (an `id`
or a `use_count` was sent), `unknown_field`. `required` covers both a missing
field inside the draft and a missing `clip` argument, in which case `field` is
`"clip"`.

### `update_clip`

```ts
await invoke("update_clip", { clip: { id, label, value, colour } });
```

| | |
| --- | --- |
| Arguments | `clip: Clip` |
| Returns | `null` |
| Mutates | yes. `id`, `position` and `use_count` are unchanged. |
| Emits | `update_clips` |
| Errors | `invalid_input`, `not_found`, `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

An unknown `id` is `not_found`. It never creates a clip.

### `delete_clip`

```ts
await invoke("delete_clip", { clip_id: "…" });
```

| | |
| --- | --- |
| Arguments | `clip_id: string` |
| Returns | `null` |
| Mutates | yes. Remaining clips are renumbered to keep `position` dense, in the same transaction ([ADR-0007](./adr/0007-list-order-representation.md)). |
| Emits | `update_clips` |
| Errors | `invalid_input` (`required`, `malformed_uuid`), `not_found`, `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

Confirmation is a frontend concern ([spec §4.2](../product/spec.md#42-create-edit-delete)).
The backend deletes when asked.

### `copy_clip`

The single clipboard write. Both the clip list and the tray reach it
([spec §4.5](../product/spec.md#45-tray-icon-with-right-click-copy)).

```ts
await invoke("copy_clip", { clip_id: "…" });
```

| | |
| --- | --- |
| Arguments | `clip_id: string` |
| Returns | `null` |
| Mutates | yes. `use_count` is incremented by exactly one. |
| Emits | **nothing** ([ADR-0008](./adr/0008-use-count-stays-backend-side.md)) |
| Errors | `invalid_input` (`required`, `malformed_uuid`), `not_found`, `clipboard`, `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

**Order of operations, in this order, before the command returns:**

1. Look up the clip. Unknown id → `not_found`, nothing else happens.
2. Write `value` to the clipboard. Failure → `clipboard`, and `use_count` is not
   incremented.
3. `UPDATE clips SET use_count = use_count + 1 WHERE id = ?`, committed.
   A database or commit failure → `storage`; the clipboard already holds the
   value, and the count is one low. **The row matching no clip → `not_found`**,
   not `storage`: the clip was deleted between step 1 and here, which is a true
   statement about the clip and not about the disk. It is the same variant step 1
   would have returned a moment earlier, so the frontend's handling — resynchronise
   with `list_clips` — is already right for it.
4. Return.

The clipboard write comes first because a count recorded for a copy that never
reached the clipboard puts a phantom into the tray ranking, which is worse than
a count that is one low. The commit completes before returning because
[spec §4.1](../product/spec.md#41-copy-a-clip) requires it and
[acceptance criterion 9](../product/spec.md#8-acceptance-criteria) tests it.
What "committed" costs is settled by
[ADR-0009](./adr/0009-durability-level.md).

`copy_clip` is a thin wrapper over one internal function. The tray calls that
function directly, not the IPC command, so a tray copy generates no IPC traffic
and there is exactly one clipboard implementation.

### `reorder_clips`

```ts
await invoke("reorder_clips", { order: [id1, id2, id3, …] });
```

| | |
| --- | --- |
| Arguments | `order: string[]` — every clip id, in the new display order |
| Returns | `null` |
| Mutates | yes, in one transaction |
| Emits | `update_clips` |
| Errors | `invalid_input` (`required`, `malformed_uuid`, `not_a_permutation`), `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

`order` must be an exact permutation of the stored id set: same length, same
members, no duplicates. Anything else is
`invalid_input { field: "order", reason: "not_a_permutation" }`. This command
**never** returns `not_found`; an id that is not in the store makes the whole
argument wrong, not one element of it.

On `not_a_permutation` the frontend discards the drag and renders the most
recent `update_clips` payload. It does not retry with the same order. This is
the expected outcome when a clip was created or deleted between the frontend's
last list and the drop, and it is why a partially applied order cannot exist.

### `export_clips`

```ts
const { exported } = await invoke("export_clips", { path: "C:\\…\\clips.json" });
```

| | |
| --- | --- |
| Arguments | `path: string` — an absolute path to a `.json` file |
| Returns | `ExportResult` — `{ exported: number }` |
| Mutates | no |
| Errors | `invalid_input` (`required`), `io`, `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

The **frontend** opens the file dialog and passes the chosen absolute path. The
backend never opens a dialog: putting UI sequencing in the backend would make
the command impossible to test without a GUI. A cancelled dialog is a frontend
state and produces no `invoke` and no error variant.

The file is written whole and atomically: to `<path>.part` in the target
directory, then renamed onto `path`. **`<path>.part` is deleted on every failure
path, including a failed rename**, so neither a partial nor a complete plaintext
file is ever left under a name the user did not choose. The temporary file
carries clip values, so this is a disclosure rule and not tidiness
([acceptance criterion 6](../product/spec.md#8-acceptance-criteria)).

**The file is read back and parsed before the rename**, and before the command
reports success: reopen `<path>.part`, parse it as an
[`ExportFile`](#exportfile), and assert the clip count equals the number
exported. Failure → `io { operation: "write", path, reason: "other" }`, and the
temporary file is deleted. `reason` is `"other"` because the write itself
reported no fault; what failed is the check that it produced a readable file. Export is the only recovery path this design has
([ADR-0005](./adr/0005-sqlite-store.md)), and
[criterion 7](../product/spec.md#8-acceptance-criteria) has the user wipe the
store between the export and the import — so an export that silently wrote a
truncated file is the one failure that turns a backup into data loss. A write
that is never read back is not a backup.

The one residue this cannot prevent is a `<path>.part` left by a process killed
mid-write. It sits in a directory FastClip does not own and cannot sweep, which
is a reason to keep the window between write and rename as short as it is.

The exported file is **plaintext**. The warning is shown at the moment of
export, from the [copy deck](../product/copy.md)
([spec §4.6](../product/spec.md#46-export-and-import-json)).

Export requires the store to be unlocked
([spec §4.8](../product/spec.md#48-encryption-and-unlocking)).

### `import_clips`

```ts
const { imported } = await invoke("import_clips", { path: "C:\\…\\clips.json" });
```

| | |
| --- | --- |
| Arguments | `path: string` — an absolute path to a `.json` file |
| Returns | `ImportResult` — `{ imported: number }` |
| Mutates | yes |
| Emits | `update_clips`, only on success |
| Errors | `invalid_input` (`required`), `import`, `io`, `locked`, `crypto` (`corrupt`), `unsupported_version` (`schema`), `storage` |

**Atomicity, as a mechanism.** Two phases, and the store is not touched during
the first:

1. **Validate whole.** Read the file, parse it, and validate every clip against
   [§1](#1-data-types). The first failure aborts with an `import` error naming
   the reason, the field and the index of the offending clip. Nothing has been
   written.
2. **Apply whole.** `BEGIN IMMEDIATE`, insert every clip with a freshly minted
   id and `use_count` 0, `COMMIT`. Any error → `ROLLBACK` and a `storage` error.

`BEGIN IMMEDIATE` rather than the default deferred transaction, so the write
lock is taken at the start and a concurrent writer cannot cause a lock upgrade
to fail halfway through. The atomicity is SQLite's, which is what
[ADR-0005](./adr/0005-sqlite-store.md) chose SQLite for; a staging table or a
backup-and-restore would reimplement it worse.

**Merge semantics**, from [spec §4.6](../product/spec.md#46-export-and-import-json):

- Every imported clip is **added**. Nothing is deleted or overwritten, and there
  is no replace-all.
- Imported clips are **appended in file order** after the user's existing clips.
  Interleaving or prepending would move clips the user did not touch; appending
  leaves the existing order exactly as it was.
- There is **no deduplication**. Importing the same file twice produces two
  copies of every clip. That follows from "import never overwrites" and is
  stated here so it is not invented as a feature.
- An empty `clips` array succeeds and returns `{ "imported": 0 }`.

### `get_lock_state`

```ts
const state: LockState = await invoke("get_lock_state");
```

| | |
| --- | --- |
| Arguments | none |
| Returns | `LockState` |
| Mutates | no |
| Errors | `crypto` (`bad_key_material`, `corrupt`), `unsupported_version` (`key_material`, `schema`), `storage` |

Called **once at startup**. It is not polled: lock state changes are pushed by
the [`lock_state`](#lock_state) event.

It does not open the clip database itself. Whether the store is encrypted is read
from the database file's header, and the rest from the key material beside it
([storage](./storage.md)).

**It is nonetheless where the [startup sequence](#startup-sequence) meets an
open-time fault when encryption is off**, because startup recovery opened the
database before any command was served
([opening the database](#opening-the-database)). That is what the `corrupt` and
`schema` variants above are: not a second attempt to open the store, but the
recorded result of the one that already happened, relayed like any other command
relays it. Being first in the sequence is why the frontend meets it here; it is
not an exclusive right to carry it. With encryption **on** the database was not
opened at startup, so neither variant is reachable from this command and both
belong to [`unlock`](#unlock).

**On a first run there is no store yet, and this command still succeeds.** The
backend creates `~/.fast-clip/` and an empty `clips.db` at schema version 1
during startup, before it serves any command, and sweeps any leftover
intermediate files while it is there
([storage](./storage.md#startup-recovery)). A failure to create either is
`storage`, and it is the only reason a fresh install reaches the failure screen.
**A `clips.db` that is present but unreadable is a different case and is not a
first run.** Startup recovery classifies it, deletes nothing, creates nothing,
and this command returns `storage`
([storage](./storage.md#classifying-clipsdb)). Treating it as absent is what
would destroy an encrypted store, because the `keyfile` delete and the
create-fresh step are both gated on that classification.

`get_lock_state` therefore never has to describe an absent file, and it returns
`{ encryption_enabled: false, locked: false, attempts_remaining: null, retry_after_ms: null }`
to a new user.

### `unlock`

```ts
await invoke("unlock", { pin: "123456" });
```

| | |
| --- | --- |
| Arguments | `pin: string` — exactly 6 ASCII digits |
| Returns | `null` |
| Mutates | yes. Lock state changes, and the failed-attempt counter is reset on success or incremented on failure. |
| Emits | `lock_state`, then `update_clips`, both only on success |
| Errors | `invalid_input` (`required`, `not_six_digits`), `bad_pin`, `backoff`, `wrong_state` (`encrypted`, `locked`), `crypto` (`bad_key_material`, `corrupt`), `unsupported_version` (`key_material`, `schema`), `storage` |

A wrong PIN is detected when the wrapped DEK fails to unwrap, which is an
authenticated check rather than a guess — so `attempts_remaining` is exact.
**Nothing is ever wiped**
([ADR-0004](./adr/0004-optional-pin-encryption.md)).

**Which variant a failed attempt returns:**

| Attempt | Returns |
| ------- | ------- |
| PIN was evaluated and was wrong, attempts remain | `bad_pin { attempts_remaining: n, retry_after_ms: null }` |
| PIN was evaluated and was wrong, and no attempts remain — the fifth failure **or any later one** | `bad_pin { attempts_remaining: 0, retry_after_ms: 30000 }` — **one** error carrying both facts |
| A backoff is already running, so the PIN was **not** evaluated | `backoff { retry_after_ms }` |

`bad_pin` carries `retry_after_ms` so that the attempt which triggers the wait
reports the wait. Without it the frontend would learn `attempts_remaining: 0`,
have no duration to count down from, and only discover the wait by making a
sixth attempt — and `lock_state` is not emitted on a failed unlock, so no other
message could supply it.

**Every evaluated failure from the fifth onward returns the identical error.**
The sixth wrong PIN — entered after the fifth's wait expired — is
`bad_pin { attempts_remaining: 0, retry_after_ms: 30000 }`, and so is the
twentieth. Row two is not a description of one transition; the wait does not vary
with the attempt ([ADR-0011](./adr/0011-flat-backoff.md)), so the frontend has
one branch here rather than a series.

While a backoff is active every `unlock` returns `backoff` without evaluating
the PIN, so a correct PIN entered early neither succeeds nor resets the wait.

Every wait is 30 seconds — the fifth failure and every one after it
([ADR-0011](./adr/0011-flat-backoff.md),
[storage](./storage.md#argon2id-parameters-and-the-backoff)). The counter and
the deadline are persisted, so closing FastClip does not clear either.

On success the backend emits `lock_state` and then `update_clips`. The frontend
does **not** call `list_clips` after unlocking.

`unlock` when the store is not encrypted is `wrong_state { required: "encrypted" }`.
`unlock` when already unlocked is `wrong_state { required: "locked" }`; it is not
a silent success, because that would accept a wrong PIN.

`unlock` is also the way back from a manual [`lock`](#lock), on exactly the path
it takes at launch: **read `keyfile` and its version byte**, unwrap the DEK,
reopen the database, emit `lock_state` then `update_clips`, rebuild the tray.
There is no second unlock path and no shortcut for a store that was open earlier
in the same process.

**That re-read is why `unlock` declares the key-material variants**, not only the
database ones. `crypto { bad_key_material }` and
`unsupported_version { component: "key_material" }` are reachable here even
though [`get_lock_state`](#get_lock_state) already read the same file at launch:
between the two reads a sync client can restore an older `keyfile`, the user can
delete it, or DPAPI can fail to unprotect it. Manual lock makes that interval
arbitrarily long and puts a second read in the middle of a session, where before
there was only ever one at startup.

**Every reader of `keyfile` declares the same two variants.** There are three —
`unlock`, [`disable_encryption`](#disable_encryption) and
[`change_pin`](#change_pin) — and the version byte sits *outside* the DPAPI blob
precisely so that it is parsed before anything else, so every one of them meets
it. A reader that declared `crypto { bad_key_material }` but not
`unsupported_version { component: "key_material" }` would have to coerce a
too-new key file into "your store belongs to another Windows account", which is a
different fact and a different remedy, or return `internal`, which
[§4](#4-errors) forbids for a modelled condition.

An earlier version of this page applied the reasoning to `unlock` alone and named
the other two as its precedent without changing them.

### `lock`

Locks the store at the user's request. It is the only way the store becomes
locked while FastClip is running.

```ts
await invoke("lock");
```

| | |
| --- | --- |
| Arguments | none |
| Returns | `null` |
| Mutates | yes, in memory only. Nothing on disk changes. |
| Emits | `lock_state`, on every success including the no-op below |
| Errors | `wrong_state` (`encrypted`) |

**Scope.** Manual lock only. There is no idle timeout, no configurable timeout
and no lock-on-minimise: the store locks when the user asks and at no other
time. The PIN is not an argument — the caller is already unlocked, and demanding
a PIN to give up access protects nothing.

**The emitted payload is fixed:**
`{ "encryption_enabled": true, "locked": true, "attempts_remaining": 5, "retry_after_ms": null }`.
`attempts_remaining` is 5 and `retry_after_ms` is `null` in every case. A
successful `unlock` resets `failed_attempts` to 0, and no `unlock` succeeds while
a backoff is running, so an unlocked store always has a clear counter. Locking is
not a failed attempt and never starts a backoff.

**Errors, in full.** `wrong_state { required: "encrypted" }` when
`encryption_enabled` is false: there is no PIN, so locking would produce a state
the user cannot leave. That is the only variant. `lock` **cannot** return
`storage` — releasing the key must not be refused because a disk operation
failed, so a failed checkpoint or close is absorbed rather than reported
([storage](./storage.md#locking-on-demand)).

**Called while already locked**, it returns `null`, emits `lock_state` with an
unchanged payload, and does nothing else. Locking is idempotent because the
caller's postcondition — the store is locked — already holds
([question 16](#closed-in-the-second-round-at-g0b)).

**It does not emit `update_clips`.** No event carries an empty list; the frontend
discards its own on `locked: true`. An empty `Clip[]` is indistinguishable from a
store whose clips were all deleted
([question 18](#closed-in-the-second-round-at-g0b)).

**Interaction with `get_lock_state`:** none beyond the event.
[`get_lock_state`](#get_lock_state) is still called once at startup and is never
polled, and the [`lock_state`](#lock_state) event carries this change like every
other. A manual lock writes nothing, so it changes nothing `get_lock_state` will
report at the next launch — whether the store is unlocked is in-memory state and
every launch starts locked
([storage](./storage.md#who-owns-lock-state)).

**An operation in flight.** `lock` takes the store's connection lock, so a
command already executing runs to completion and no transaction is interrupted. A
clip-touching command invoked around the same moment therefore **either completes
in full or returns `locked`**. It never partially applies, and the lock never
turns it into `internal` or `storage` by closing the connection underneath it.
Which of the two outcomes it gets is not defined and the frontend must not depend
on it. A command can still fail on its own merits at that moment — a disk fault
is a disk fault whether or not someone pressed Lock; the guarantee is that the
lock adds no third outcome of its own. The mechanism is in [storage](./storage.md#locking-on-demand)
([question 17](#closed-in-the-second-round-at-g0b)).

**The tray rebuilds before `lock` returns**, to the single *Unlock FastClip* item
[spec §4.8](../product/spec.md#48-encryption-and-unlocking) requires while
locked. Clip labels left in a native menu after a manual lock breach
[acceptance criterion 10](../product/spec.md#8-acceptance-criteria) exactly as
labels left in the window would.

**What the frontend does on `locked: true`,** whatever produced it: discard the
in-memory clip list, close any open create or edit form, clear the search query,
and show the PIN prompt. An open edit form holds one clip `value` and a rendered
list holds every one of them, so this is
[spec §4.8](../product/spec.md#48-encryption-and-unlocking)'s disclosure rule
applied to state the frontend is already holding.

**Where the control lives is not specified here.** The contract fixes the call,
not its affordance; the label belongs in the [copy deck](../product/copy.md). One
constraint on it: the control is hidden whenever `encryption_enabled` is false,
so `wrong_state { required: "encrypted" }` is unreachable in normal use and
exists to catch a defect rather than to be shown to a user.

**On the specification.**
[Spec §4.8](../product/spec.md#48-encryption-and-unlocking) does not describe
manual lock — the owner decided it at G0b, after that page was accepted. The
specification is the page that must gain the sentence, and that is escalated to
the orchestrator rather than written here.

Spec §4.8 says nothing about locking after launch, and this command exists, so
until that page gains the sentence there is a ratified command with no
requirement behind it.

That is the only point on which this page leads the specification. The backoff
was a second one — §4.8 specified doubling and the build did not — and it is
closed from the other side: the owner has made the wait a flat 30 seconds
([ADR-0011](./adr/0011-flat-backoff.md)) and §4.8 is being edited to say so.

### `enable_encryption`

```ts
await invoke("enable_encryption", { pin: "123456" });
```

| | |
| --- | --- |
| Arguments | `pin: string` — exactly 6 ASCII digits |
| Returns | `null` |
| Mutates | yes. The store is converted to an encrypted database and key material is written. |
| Emits | `lock_state` |
| Errors | `invalid_input` (`required`, `not_six_digits`), `wrong_state` (`unencrypted`), `crypto` (`bad_key_material`, `corrupt`), `unsupported_version` (`schema`), `storage` |

The PIN is entered **twice in the user interface and sent once**. Confirming the
two entries match is a frontend concern; the backend receives one PIN and has
nothing to compare it to.

The irreversibility warning and the offer to export first are shown before this
command is invoked ([spec §4.8](../product/spec.md#48-encryption-and-unlocking),
[copy deck](../product/copy.md)).

The store remains **unlocked** afterwards. The user has just set the PIN; there
is nothing to prove. The emitted `lock_state` therefore carries
`encryption_enabled: true, locked: false`.

**"Unlocked" means a working connection, not only a flag.** The conversion closes
the database to rename it, so it **reopens the converted store with the DEK
before returning**
([storage](./storage.md#the-reopen-and-why-it-is-verified-first)). Without that
reopen this command would report success and an unlocked state while every
subsequent clip command failed until the process restarted — and the same on the
abort path, where a *recovered* failure would leave the application unusable.
`disable_encryption` reopens for the same reason.

`crypto { corrupt }` is reachable here, and it is not a contradiction of
[opening the database](#opening-the-database): a conversion opens the database it
is building, which is not the path into an existing store.

The conversion mechanism and its crash safety are in
[storage](./storage.md#switching-encryption-on-and-off).

### `disable_encryption`

```ts
await invoke("disable_encryption", { pin: "123456" });
```

| | |
| --- | --- |
| Arguments | `pin: string` |
| Returns | `null` |
| Mutates | yes. The store is rewritten as plaintext and the key material is deleted. |
| Emits | `lock_state` |
| Errors | `invalid_input` (`required`, `not_six_digits`), `bad_pin`, `wrong_state` (`encrypted`), `locked`, `crypto` (`bad_key_material`, `corrupt`), `unsupported_version` (`key_material`, `schema`), `storage` |

Requires the store to be unlocked **and** the PIN
([spec §4.8](../product/spec.md#48-encryption-and-unlocking)). The confirmation
stating that the store becomes plaintext is shown before this command is
invoked.

Not subject to backoff, and its failures do not touch the unlock attempt
counter: the caller is already unlocked, so a limit on guesses would protect
nothing. `bad_pin` from this command carries `attempts_remaining: null` and
`retry_after_ms: null`.

Like `enable_encryption`, it **reopens the converted store before returning**, so
the store is usable when the command resolves.

**Deleting `keyfile` happens after the commit point and cannot fail this
command.** The rename already made the store plaintext, and a `storage` returned
for a refused delete would suppress the `lock_state` emission — leaving the
settings view claiming encryption is on over a plaintext store, which is
[spec §5](../product/spec.md#5-security-posture) asserted backwards. The failure
is absorbed and the stray `keyfile` is removed at the next launch
([storage](./storage.md#failures-after-the-commit-point-are-absorbed-not-reported)).

### `change_pin`

```ts
await invoke("change_pin", { current_pin: "123456", new_pin: "654321" });
```

| | |
| --- | --- |
| Arguments | `current_pin: string`, `new_pin: string` |
| Returns | `null` |
| Mutates | yes. The DEK is re-wrapped under the new PIN. The database is **not** re-encrypted, so this is instant regardless of clip count ([ADR-0004](./adr/0004-optional-pin-encryption.md)). |
| Emits | nothing. No field of `LockState` changes. |
| Errors | `invalid_input` (`required`, `not_six_digits`), `bad_pin`, `wrong_state` (`encrypted`), `locked`, `crypto` (`bad_key_material`, `corrupt`), `unsupported_version` (`key_material`, `schema`), `storage` |

Requires the store to be unlocked. Not subject to backoff, on the same reasoning
as `disable_encryption`; `bad_pin` carries `attempts_remaining: null` and
`retry_after_ms: null`.

`crypto` here is `bad_key_material` only. `change_pin` reads and rewrites
`keyfile` and never opens the database, so `corrupt` is not reachable from it —
unlike `disable_encryption`, which must read the encrypted store to write a
plaintext one.

The old PIN stops working the moment this returns. If the process is killed
mid-change, the key material is either fully re-wrapped or untouched — it is
written to a temporary file and renamed.

### `get_settings`

```ts
const settings: Settings = await invoke("get_settings");
```

| | |
| --- | --- |
| Arguments | none |
| Returns | `Settings` |
| Mutates | no |
| Errors | `storage` |

Available while locked. It reads a small plaintext file that contains no clip
data ([storage](./storage.md)).

**It never fails because of the file's contents.** Missing, truncated, unparseable
or carrying an unrecognised `version`, the result is the same:
`{ "always_on_top": false }`, and the next
[`set_always_on_top`](#set_always_on_top) rewrites the file at version 1
([versions on disk](#versions-on-disk)).

`storage` is therefore reachable for exactly one reason: `~/.fast-clip/` could
not be created or reached at all ([storage](./storage.md#startup-recovery)). That
is fatal, and the frontend does not treat it here — it continues to
[`get_lock_state`](#get_lock_state), which reports the same fault through the
failure path the [startup sequence](#startup-sequence) already defines. One
failure surface, not two.

### `set_always_on_top`

```ts
await invoke("set_always_on_top", { enabled: true });
```

| | |
| --- | --- |
| Arguments | `enabled: boolean` |
| Returns | `null` |
| Mutates | yes, the settings file |
| Emits | nothing |
| Errors | `invalid_input` (`required`), `storage` |

`invalid_input { field: "enabled", reason: "required" }` when the `enabled` key is
absent ([argument deserialisation](#the-argument-itself--every-command)). It is
the only validation this command has: any `boolean` is acceptable.

**The backend both persists and applies the setting.** It calls
`set_always_on_top` on the window itself, here and again at window creation from
the persisted value. The frontend does **not** call Tauri's window API for this,
and its capability set does not grant `core:window:set_always_on_top`.

Two writers of one window property is two sources of truth, and the startup
application has to be backend-side regardless — the window exists before the
webview does, and applying the flag from the frontend would show the window in
the wrong state first.

[Spec §4.4](../product/spec.md#44-always-on-top) says this feature is "already
implemented; keep the behaviour". Its persistence is not: the current build
holds the flag in React state initialised to `false`
(`src/Components/SettingsClip.tsx:12`) and calls
`getCurrentWindow().setAlwaysOnTop()` directly, so the toggle resets on every
launch. Persisting it is new work. It was escalated as owned by no work package,
and the orchestrator created [WP-14](../work/wp-14-settings.md) to own it.
[Spec §8](../product/spec.md#8-acceptance-criteria) still has no acceptance
criterion for it; whether it gains one is the owner's call and WP-14 does not
invent one.

## 3. Events

Both events are emitted with `AppHandle::emit`, so every listening window
receives them. There is one window.

**No emit may be silently dropped.** The pre-refactor build populates a global
`APP_HANDLE` inside a task spawned from `setup()`, so an early `invoke` finds
`None` and the emit is skipped after an `eprintln!`
([inherited debt](../reference/debt.md)). The handle must be available before
any command can be invoked. This is a contract guarantee — the frontend is
entitled to assume an event follows every mutation — and not merely a tidiness
fix.

The frontend registers both listeners **before its first `invoke`**.

### `update_clips`

| | |
| --- | --- |
| Payload | `Clip[]` — the **complete** list in display order. Never a delta. |
| Trigger | After a successful `create_clip`, `update_clip`, `delete_clip`, `reorder_clips`, `import_clips`, and after a successful `unlock`. |
| Not emitted by | `copy_clip` ([ADR-0008](./adr/0008-use-count-stays-backend-side.md)), [`lock`](#lock), the settings commands, or any failed command. Never emitted while locked. |
| Consumed by | the clip list view |

The order of this event relative to the resolution of the command that caused it
is **not defined**. The frontend must not depend on it. This costs nothing,
because every mutating command returns `null` and there is nothing else to
apply.

**The frontend discards an `update_clips` that arrives while its last received
`lock_state` says `locked: true`.** The backend emits none while locked, but a
mutation that committed just before a [`lock`](#lock) emits one, and no ordering
between two differently-named events is guaranteed. Without this rule that event
would repaint the clip list underneath the PIN prompt, which is the disclosure
[spec §4.8](../product/spec.md#48-encryption-and-unlocking) forbids. It costs one
condition and removes a race that cannot otherwise be closed from either side
alone.

**A failed emission does not fail the command.** The mutation has already
committed, so rejecting would tell the user that a create which succeeded had
failed, and leave them looking at an error beside the clip they just made.
Nothing recovers a mutation the user believes did not happen; the list recovers
on the next event or the next `list_clips`. The failure is logged at error level
([ADR-0012](./adr/0012-logging.md)) and the command returns `null`.

This is the same rule the conversions follow after their commit point
([storage](./storage.md#failures-after-the-commit-point-are-absorbed-not-reported)),
and for the same reason: once the store has changed, a report of failure is a
false statement about the store.

**The accepted cost is a silently stale list.** No error reaches the user and
nothing re-syncs automatically. That is tolerable only because the case is
near-unreachable by design rather than by luck: §3 requires the app handle to be
available before any command can be invoked, and the payload is a `Clip[]` of
derived-`Serialize` types with no failure mode of its own. If this is ever
observed in practice, it is evidence that one of those two guarantees broke, and
the log line is what says which.

### `lock_state`

| | |
| --- | --- |
| Payload | `LockState` — complete. Never a delta. |
| Trigger | Any change to `encryption_enabled` or `locked` on disk: a successful `unlock`, [`lock`](#lock), `enable_encryption` or `disable_encryption`. |
| Not emitted by | a failed `unlock` — the attempt count reaches the caller as `bad_pin { attempts_remaining }`. Not emitted at startup; the frontend calls `get_lock_state`. |
| Consumed by | the PIN prompt and the settings view |

**The event reports the store's state, not the outcome of the call.** Two
consequences, and both are the reason the trigger says "on disk":

- [`lock`](#lock) on an already-locked store emits although nothing changed. The
  payload is complete, so applying it twice is a no-op; staying silent would
  leave a frontend that had lost track of the state with nothing to correct it.
- **A conversion emits at its commit point, so it emits even when the command
  then returns an error.** `enable_encryption` and `disable_encryption` rename
  the new database into place and only afterwards reopen it; if that reopen
  fails, the command rejects with `storage` but the store *is* converted, and the
  event has already said so
  ([storage](./storage.md#the-emission-is-not-conditional-on-the-command-succeeding)).
  Tying the emission to the return value is what would let the settings view
  claim encryption is on over a plaintext store.

"Not emitted by a failed command" holds for `update_clips`, which reports a list
that a failed command did not change. It does not hold for `lock_state`, and the
difference is that a conversion's failure can come *after* the state changed.

### Startup sequence

Defined here because two plausible sequences exist, and implementing one on each
side would produce a blank window in exactly the case that matters.

**The failure screen** referred to below is one state: the mapped sentence for
the variant, plus the recovery path, and no clip list. Every branch that reaches
it stops there. It is reached from three places and looks the same in all three,
so the frontend implements it once.

1. Register listeners for `update_clips` and `lock_state`.
2. `get_settings`.
   - Rejects with `storage` → use `{ always_on_top: false }` and continue to
     step 3, which reports the fault ([`get_settings`](#get_settings)).
3. `get_lock_state`.
   - Rejects with `crypto` or `unsupported_version` → **failure screen.** With
     encryption off this is also where a corrupt or too-new *database* is
     reported ([opening the database](#opening-the-database)). Do not proceed.
   - Rejects with `storage` → **failure screen.** Two causes, one variant: the
     application directory could not be created or reached — the fault step 2
     declined to report — or `clips.db` is present and its header could not be
     read at all ([storage](./storage.md#classifying-clipsdb)). The second is
     **not** damage: nothing was deleted or created, and the message must not
     advise re-importing.
4. `locked: false` → `list_clips`, render.
   - Rejects with `storage` → **failure screen.** Do not render an empty list.
     An empty `Clip[]` means the store has no clips; a rejection means it could
     not be read, and showing the two the same way invites the user to create a
     clip into a store that has already failed.
   - Rejects with `crypto` or `unsupported_version` → **failure screen.** The
     store never opened and step 3 did not report it — either because the
     frontend skipped `get_lock_state` or because this call was already in
     flight. It is the same recorded fault, relayed
     ([opening the database](#opening-the-database)), so it maps to the same
     screen.
5. `locked: true` → show the PIN prompt. Do not call `list_clips`.
   - If `retry_after_ms` is non-null, the input starts disabled and counts down.
   - `unlock` rejects with `bad_pin` or `backoff` → stay on the PIN prompt with
     the attempts remaining or the wait. These are ordinary outcomes.
   - `unlock` rejects with `crypto` or `unsupported_version` → **failure screen.**
     The PIN was correct and the store still could not be opened, so leaving the
     user on the PIN prompt would tell them to try a PIN that already worked.
   - On a successful `unlock`, the list arrives on `update_clips`. Do not call
     `list_clips`.

## 4. Errors

Every command returns `Result<T, ClipError>`. `ClipError` reaches JavaScript as
an object with a `kind` discriminant.

```ts
type ClipError =
  | { kind: "not_found";           clip_id: string }
  | { kind: "invalid_input";       field: string; reason: InvalidReason }
  | { kind: "storage" }
  | { kind: "io";                  operation: "read" | "write"; path: string;
                                   reason: "not_found" | "permission_denied" | "disk_full" | "other" }
  | { kind: "crypto";              reason: "bad_key_material" | "corrupt" }
  | { kind: "unsupported_version"; component: "schema" | "key_material";
                                   found: number; supported: number }
  | { kind: "import";              reason: ImportReason; field: string | null; index: number | null }
  | { kind: "locked" }
  | { kind: "wrong_state";         required: "locked" | "encrypted" | "unencrypted" }
  | { kind: "bad_pin";             attempts_remaining: number | null;
                                   retry_after_ms: number | null }
  | { kind: "backoff";             retry_after_ms: number }
  | { kind: "clipboard" }
  | { kind: "internal" };

type InvalidReason =
  | "required"
  | "too_long"
  | "contains_control_characters"
  | "not_a_palette_token"
  | "not_permitted"
  | "unknown_field"
  | "malformed_uuid"
  | "not_a_permutation"
  | "not_six_digits";

type ImportReason =
  | "malformed_json"
  | "unsupported_version"
  | "missing_field"
  | "unknown_field"
  | "invalid_value";
```

In Rust this is an enum with struct variants carrying
`#[serde(tag = "kind", rename_all = "snake_case")]`, and its reason types are
plain enums with the same rename. The JSON above is the definition; the
attributes are one way to produce it.

The frontend maps **every** variant to a sentence from the
[copy deck](../product/copy.md). A raw error is never shown.

### What each variant means

| Variant | Condition | What the user is told |
| ------- | --------- | --------------------- |
| `not_found` | A `clip_id` argument names no stored clip. | The clip no longer exists. No event follows a failed command, so the frontend calls `list_clips` to resynchronise. |
| `invalid_input` | An argument failed validation. `field` names the wire field. | The inline validation message for that field. |
| `storage` | The store could not be read or written, created, or its directory reached — including a `clips.db` that is present but whose header cannot be read at all ([storage](./storage.md#classifying-clipsdb)). | The store could not be read or written. **Not** "disk write failed": this variant covers reads, and the commonest read failure is a file another process is holding, where nothing is damaged and no recovery is needed. |
| `io` | A **user-chosen** file — an export target or an import source — could not be read or written. Never the store. | The export or import could not reach the file. |
| `crypto` | `bad_key_material`: the key material is missing, unreadable, or was created by another Windows account or machine. `corrupt`: the database will not open or fails its integrity check — with encryption on, after the key unwrapped successfully; with encryption off, on its own. | Account-bound store, or an unopenable store. Both point at [import](../product/spec.md#46-export-and-import-json) as the recovery path. |
| `unsupported_version` | An on-disk artefact carries a version this build does not know. | The store is from a newer version of FastClip. |
| `import` | The import file's **content** was rejected. `index` is the 0-based position of the offending clip in the `clips` array; `field` names the offending field. Either may be `null` when the fault is at the top level. | What was wrong and where. |
| `locked` | Encryption is on and the store is not open, at launch or after [`lock`](#lock). | The PIN prompt, and the frontend discards the clip list, any open form and the search query ([`lock`](#lock)). |
| `wrong_state` | The command needs the store in a state it is not in. `required` names the needed state, never the observed one. Three are reachable: `unencrypted` from `enable_encryption`, `encrypted` from `unlock`, `lock`, `disable_encryption` and `change_pin`, and `locked` from `unlock` on an already-unlocked store. | The action is not available right now. |
| `bad_pin` | The PIN was evaluated and did not unwrap the DEK. `attempts_remaining` is `null` where no lockout applies to the entry point; `retry_after_ms` is non-null only on the attempt that exhausts the five. | Wrong PIN, with the attempts remaining, and the wait if one has just started. |
| `backoff` | An unlock was attempted while a wait was already running. The PIN was **not** evaluated. | How long to wait. |
| `clipboard` | The clipboard could not be written. `use_count` was not incremented. | The clip could not be copied. |
| `internal` | An unmodelled failure — a panic, a serialisation fault, a rejection that is not a `ClipError`. | An unexpected error. |

`internal` exists so the frontend's mapping can be total. **No command may
return it for a condition modelled above**; one that does is a backend defect,
and reaching it in a test is a finding.

### Design notes on the error set

Five changes from the shape the baseline contract proposed, each recorded
because a reader would otherwise assume a typo:

| Change | Why |
| ------ | --- |
| `bad_key` removed from `crypto` | SQLCipher cannot distinguish a wrong key from a corrupt file — both surface as "file is not a database". In this design the PIN is verified first by unwrapping the DEK, so a key that reaches SQLCipher is already authenticated; a failure at that point means the database is damaged or was paired with key material from another store. The user's message and remedy are identical in both cases. A `bad_key` variant would present a guess as a fact. |
| `unsupported_version` lifted out of `crypto` into a top-level variant | A schema version too new is reachable with **encryption off**, which is the default. Nesting it under `crypto` would make the commonest configuration unable to report it. |
| `import.line` replaced by `import.index` and `import.field` | A JSON line number is expensive to produce accurately and means nothing to a user looking at a file they did not write. The index of the offending clip and the name of the offending field are what "naming what was wrong" ([spec §4.6](../product/spec.md#46-export-and-import-json)) needs. |
| `storage.retryable` removed; `storage` is now a bare variant | The flag had no defined classification and no consumer. Two backends would have drawn the transient/permanent line differently, the [copy deck](../product/copy.md) carries one sentence for both values, and a Retry affordance built on it would have offered retries that cannot succeed. This is the same argument [ADR-0008](./adr/0008-use-count-stays-backend-side.md) used to take `use_count` off the wire, applied consistently. |
| `bad_pin` gained `retry_after_ms` | Without it the attempt that exhausts the five could report either "0 attempts remaining" or a 30-second wait, and two implementations would pick differently. Carrying both facts in one error settles it and gives the frontend the duration at the moment it must start counting. |

### When `locked` is reachable

The store becomes locked at launch, and again whenever the user calls
[`lock`](#lock). Both are ordinary states rather than edge cases. There is no
idle timeout and no automatic lock of any kind.

The frontend handles `locked` on **every** clip-touching call, at any moment, for
three reasons:

- A manual lock can land between any `invoke` and its response.
- A command issued concurrently with `disable_encryption`, or during a startup
  race, can observe either state.
- One handler per call is cheaper than deciding per call site which calls can see
  it.

An earlier version of this page kept that universal handling as insurance against
a lock-while-running feature that did not exist. Manual lock now exists, so the
handling is load-bearing rather than defensive, and a call site that omits it is
a defect rather than an inconsistency. The reasoning changed; the rule did not.

The baseline contract asserted that "the store can be locked while the window is
open". That was unfounded against the specification when it was written. It is
true now, for a reason that arrived afterwards
([ADR-0010](./adr/0010-manual-lock.md)).

### Validation rules, stated once

| Field | Rule | Failure |
| ----- | ---- | ------- |
| `label` | 1–100 characters. Not empty and not whitespace-only. No control characters, including newline and tab. | `required`, `too_long`, `contains_control_characters` |
| `value` | 1–10 000 characters. Not empty and not whitespace-only. Any character otherwise, including newlines. | `required`, `too_long` |
| `colour` | A palette token. | `not_a_palette_token` |
| `clip_id`, `order[]`, `id` on an update | A hyphenated UUID. Parsed case-insensitively, stored lowercase ([§0](#0-wire-rules)). | `malformed_uuid` |
| `pin`, `current_pin`, `new_pin` | Exactly 6 characters, each ASCII `0`–`9`. | `not_six_digits` |
| `path` | Non-empty. | `required` |
| `id` on a create, `use_count` anywhere | Never accepted. | `not_permitted` |
| any other field | Rejected. | `unknown_field` |
| **any argument, absent** | Every argument is required. There is no optional argument on this page and no default for a missing one. `field` is the argument's wire name. | `required` |

Neither `label` nor `value` is trimmed; what the user typed is what is stored. A
label or value consisting only of whitespace is rejected as `required`, because
a row with a blank label is indistinguishable from a broken one and the user
cannot tell what they clicked.

Both sides validate. The frontend's copy exists to show an inline message before
a round trip ([spec §4.2](../product/spec.md#42-create-edit-delete)); the
backend's is the one that decides. They use the same limits and the same
character counting ([§0](#0-wire-rules)).

## 5. Closed questions

| # | Question | Resolution |
| - | -------- | ---------- |
| 1 | Does plaintext cross IPC? | Yes, accepted. [ADR-0002](./adr/0002-threat-model.md). No security-motivated `copy_clip`. |
| 2 | What do `icon`, `visible`, `clear_time` mean? | Deleted. [Spec §3](../product/spec.md#3-data-model). |
| 3 | What is the list's ordering? | User drag-to-reorder, persisted. [Spec §4.3](../product/spec.md#43-reorder). |
| 4 | What is `colour` after Mantine? | Fixed named tokens. [Spec §7](../product/spec.md#7-colour). |
| 5 | Who mints `id`? | **The backend.** Owner's decision. See [below](#identity-closed). |
| 6 | Does the window's copy path reuse the tray's backend clipboard write? | **Yes, one command for both.** Forced by `use_count` ([spec §4.1](../product/spec.md#41-copy-a-clip)), not by security. |

### Closed at G0b

| # | Question | Resolution | Alternative rejected |
| - | -------- | ---------- | -------------------- |
| 7 | Upsert, or two commands? | **Two.** `create_clip` takes a `ClipDraft` with no `id`; `update_clip` on an unknown id is `not_found`. An upsert is unbuildable anyway once the backend mints identity — the client has no id to insert under. | One `upsert_clip`. Rejected: it requires a client-supplied id, which closed question 5 forbids. |
| 8 | Order representation | **The list carries it on the wire; a dense `position` column carries it in the store.** [ADR-0007](./adr/0007-list-order-representation.md). `reorder_clips { order: string[] }` sends the complete permutation. | A `position` field on the wire `Clip`; a `move_clip` delta; a linked list. Each named in the ADR. |
| 9 | On-disk format version, and how a decryption failure reaches the user | **Independently versioned artefacts, one version field each** ([§1](#versions-on-disk)), and a decryption failure reaches the user as `bad_pin`, `backoff`, `crypto` or `unsupported_version` ([§4](#4-errors)) — never as a blank window. | One version field for everything, and `unsupported_version` nested under `crypto`. Both rejected in [§1](#versions-on-disk) and [§4](#design-notes-on-the-error-set). |
| 10 | Import atomicity | **Validate the whole file in memory, then apply it in one `BEGIN IMMEDIATE` transaction.** Any failure in phase one leaves the store untouched; any failure in phase two rolls back. Imported clips are appended in file order; there is no deduplication. See [`import_clips`](#import_clips). | A staging table, or a backup-and-restore around the merge. Both reimplement SQLite's own atomicity, worse. Interleaving imported clips into the existing order, rejected because it moves clips the user did not touch. |
| 11 | The lock-state surface | **Both, and neither is a poll.** `get_lock_state` is called once at startup; the `lock_state` event carries every subsequent change. The backend is the sole owner, and the tray reads that state in-process, so the window and the tray cannot disagree. | Polling `get_lock_state`. Rejected: it puts a clock between two views of one fact. An event alone. Rejected: nothing tells the frontend the state at launch. |
| 12 | Does the copy command return before or after the `use_count` write? | **After**, as [spec §4.1](../product/spec.md#41-copy-a-clip) requires. Clipboard first, then the committed increment, then return. `update_clips` is **not** emitted on a copy ([ADR-0008](./adr/0008-use-count-stays-backend-side.md)), and what the commit costs is settled by [ADR-0009](./adr/0009-durability-level.md). | Returning before the write. Rejected by the specification. `synchronous = FULL`, rejected in ADR-0009. Emitting `update_clips` per copy, rejected in ADR-0008. |
| 13 | Is `colour` a Rust enum or a validated `String`? | **A closed enum**, variants drawn from the [palette](../product/palette.md) Token column. See [`Colour`](#colour). | A validated `String`. Rejected: it spreads the check across call sites and lets an unrenderable token reach storage. |
| 14 | Does `create_clip` return the created `Clip`? | **No, `null`.** `update_clips` is the single path by which list state reaches the frontend. | Returning the `Clip`, which would give the frontend two paths to apply and no defined order between them. |
| 15 | Who opens the export and import file dialogs? | **The frontend**, which passes an absolute path. Cancellation is a frontend state, not an error variant. | The backend opening the dialog. Rejected: it makes the command untestable without a GUI. |

### Closed in the second round at G0b

The owner answered the escalations raised above. One answer — manual lock is in
scope — opened questions 16 to 18. Question 19 came from reading
[WP-14](../work/wp-14-settings.md), which assumed an error variant this page does
not define.

| # | Question | Resolution | Alternative rejected |
| - | -------- | ---------- | -------------------- |
| 16 | [`lock`](#lock) on an already-locked store: success, or an error? | **Success, and it still emits `lock_state`.** The caller's postcondition already holds, so there is nothing to refuse. Re-emitting the unchanged state keeps one convergence path on the frontend. | `wrong_state { required: "unlocked" }`. Rejected: it adds a fourth member to `required` for a case reachable only by a double-click, and shows a failure for a state the user asked for and is in. Also rejected: succeeding silently with no event, which leaves a frontend that has lost track of the state with nothing to correct it. |
| 17 | What happens to an operation in flight when `lock` is called? | **`lock` takes the store's connection lock, so nothing is interrupted, and every clip-touching command re-checks lock state after acquiring that lock.** The observable rule: such a command either completes in full or returns `locked`. Never partially, never `internal`. Mechanism in [storage](./storage.md#locking-on-demand). | Cancelling the in-flight command. Rejected: the user pressed Lock, not Undo, and a half-applied mutation is the outcome [ADR-0007](./adr/0007-list-order-representation.md) and the import design both exist to prevent. Also rejected: locking without taking the connection lock, which tears a transaction and lets a write land after the store was reported locked. |
| 18 | Does `lock` emit `update_clips` carrying an empty array, so the frontend clears? | **No. `lock_state` only; the frontend discards its own list on `locked: true`.** | An empty `update_clips`. Rejected: that event is defined as the complete list, so an empty one asserts the store holds no clips. It is indistinguishable from every clip having been deleted, and it is false. |
| 19 | What does an unrecognised `version` in `settings.json` do? | **Nothing user-visible: the file is ignored, `always_on_top` falls back to `false`, the app starts, and the next write replaces the file at version 1.** [`get_settings`](#get_settings) returns only `storage`. See [versions on disk](#versions-on-disk). | `unsupported_version { component: "settings" }`, which [WP-14](../work/wp-14-settings.md) assumed. Rejected: it needs a fourth `component` value and a fifth error variant on `get_settings`, and it refuses to start FastClip over one boolean — while WP-14's own rule lets a *truncated* settings file start the app. The two cannot both be right, and the lenient one is. |

### Identity (closed)

The backend mints every `id`. The frontend never generates one and never sends
one.

- `create_clip` takes a [`ClipDraft`](#clipdraft), which has no `id` field.
- The frontend has no reason to depend on the `uuid` npm package.
  `frontend-dev` removes the dependency
  ([WP-04](../work/wp-04-frontend-scaffold.md)).
- An id arriving from the frontend anywhere other than as a lookup key is
  `invalid_input { field: "id", reason: "not_permitted" }`.

## 6. Open questions for the architect

**None.** The six questions this section held at G0b are closed and recorded in
[§5](#5-closed-questions) as resolutions 7 to 15. Four further questions — three
opened by the owner's decision to put manual lock in scope, one by
[WP-14](../work/wp-14-settings.md) — are closed as 16 to 19. Each names the
alternative rejected.

Two decisions remain open elsewhere. Neither is a contract question, and no
command name, argument, payload field, error variant or event on this page
depends on either answer:

- **Which Rust crate provides SQLite and SQLCipher.** Deferred to
  [ADR-0005](./adr/0005-sqlite-store.md) on the evidence of the
  [WP-02](../work/wp-02-toolchain.md) spike.
- **Which colour tokens the palette contains.** Deferred by the owner to
  [WP-10](../work/wp-10-palette.md). The [`Colour`](#colour) type and its
  generated shape are settled ([question 13](#closed-at-g0b),
  [`Colour`](#colour)); its membership is not.
  [WP-04](../work/wp-04-frontend-scaffold.md) — not WP-05 — is the first package
  affected, and it proceeds on the provisional single-token list rather than
  waiting.

## 7. What `colour` holds today

⚠️ **Corrected after review. An earlier version of this page said `colour` held
a Mantine palette name. It does not.**

`NewClip.tsx:13` and `EditClip.tsx:19` initialise colour to the literal
`'rgba(47, 119, 150, 0.7)'` and write it from a Mantine `ColorPicker` with
`format="rgba"`. Stored values are **free-form rgba strings**, not palette
names.

`FastClip`'s constructor declares a default of `"red"`, but `NewClip` always
passes the picker's value, so that default is unreachable. Existing stores may
nonetheless contain either form.

This matters less than it did, because
[ADR-0003](./adr/0003-no-legacy-migration.md) removed the migration — no stored
rgba value is ever converted. It is recorded here because the earlier claim was
wrong, and because it explains why the palette is a closed set: users already
have free colour choice, and it already produces the colour-reset defect in
[inherited debt](../reference/debt.md). [Spec §7](../product/spec.md#7-colour)
closes the set as a fix, not a restriction.
