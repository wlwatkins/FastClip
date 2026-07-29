# IPC contract

**Status:** Baseline with the owner's product decisions applied. Not yet
ratified.
**Owner:** `architect`. Nobody else edits this page.

The only agreed interface between the Svelte frontend and the Rust backend.
After the architect ratifies it at G0b, `frontend-dev` and `backend-dev`
implement against this page, not against each other's code.

Derived from the [specification](../product/spec.md). If they disagree, this
page is the defect.

## 1. Data type: `Clip`

| Field | Rust | TypeScript | Notes |
| ----- | ---- | ---------- | ----- |
| `id` | `Uuid` | `string` | hyphenated UUID, backend-minted |
| `label` | `String` | `string` | 1–100 characters |
| `value` | `String` | `string` | 1–10 000 characters |
| `colour` | `Colour` | `Colour` | [palette token](../product/palette.md), never hex |
| `use_count` | `u64` | `number` | backend-owned; read-only to the frontend |

`use_count` is written only by the backend. It appears in payloads the frontend
*receives* and must never appear in one the frontend *sends* — an incoming
`use_count` is `invalid_input`, on the same rule as `id`
([§5](#5-closed-questions)).

`icon`, `visible` and `clear_time` are removed and must not appear in any new
type. There is no migration path
([ADR-0003](./adr/0003-no-legacy-migration.md)), so deserialisation is strict —
`deny_unknown_fields` is available and an unknown field is a defect.

The architect decides whether `colour` is a Rust `enum` — which rejects unknown
values at the boundary but makes adding a colour a breaking change — or a
validated `String`.

### What `colour` holds today

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

## 2. Commands

Registered in `src-tauri/src/lib.rs`. All carry
`#[command(rename_all = "snake_case")]`; the wire casing is stated here rather
than inferred.

| Command | Args | Returns | Mutates |
| ------- | ---- | ------- | ------- |
| `get_clips` | — | `Clip[]` | no |
| `new_clip` | `{ clip: Clip }` | `void` | yes |
| `update_clip` | `{ clip: Clip }` | `void` | yes |
| `del_clip` | `{ clip_id: string }` | `void` | yes |

Defects in the current surface:

- `del_clip` takes a bare `clip_id` rather than a wrapped object. Fix or
  justify.
- `new_clip` and `update_clip` share one implementation
  (`insert_or_update_clip`), so `update_clip` on an unknown id silently creates
  a clip. Open question 1.
- `new_clip` accepts a client-supplied `id`. Identity belongs to the backend
  ([§5](#5-closed-questions)), so this signature must change.

### New surface required by the specification

The architect specifies each fully — names, arguments, errors, events.

| Feature | Needs |
| ------- | ----- |
| [Copy §4.1](../product/spec.md#41-copy-a-clip) | one backend command that writes the clipboard and increments `use_count`, used by both the window and the tray |
| [Reorder §4.3](../product/spec.md#43-reorder) | a way to persist user ordering |
| [Tray copy §4.5](../product/spec.md#45-tray-icon-with-right-click-copy) | backend-side clipboard write |
| [Export §4.6](../product/spec.md#46-export-and-import-json) | serialise all clips to a chosen path |
| [Import §4.6](../product/spec.md#46-export-and-import-json) | validate, then merge with new ids |
| [Encryption §4.8](../product/spec.md#48-encryption-and-unlocking) | enable with a PIN, disable with the PIN, change the PIN, unlock, and report lock state |

## 3. Events

### `update_clips`

| | |
| --- | --- |
| Emitted by | backend, after every successful mutation |
| Payload | `Clip[]`, the complete list in display order |
| Consumed by | the clip list view, and the tray menu builder |

Ordering is now part of this payload's meaning rather than an accident of
iteration.

The event is emitted through a `lazy_static` global `APP_HANDLE` populated
asynchronously in `setup()`. If the handle is not yet set, the emit is skipped
with an `eprintln!` and the frontend never updates. See
[inherited debt](../reference/debt.md).

## 4. Errors

There is no error contract today. Every command returns
`Result<_, tauri::Error>`, which reaches JavaScript as an opaque string, so the
frontend cannot distinguish "not found" from "disk full" from "wrong key".

The architect defines a discriminated type at G0b. A starting shape:

```ts
type ClipError =
  | { kind: "not_found";     id: string }
  | { kind: "invalid_input"; field: string; reason: string }
  | { kind: "storage";       retryable: boolean }
  | { kind: "crypto";        reason: "bad_key" | "corrupt" | "unsupported_version" }
  | { kind: "import";        reason: string; line?: number }
  | { kind: "locked" }
  | { kind: "bad_pin";       attempts_remaining: number }
  | { kind: "backoff";       retry_after_ms: number }
```

Every command that touches clips returns `locked` when the store is locked
([spec §4.8](../product/spec.md#48-encryption-and-unlocking)). The frontend
must handle it on every call, not only at launch — the store can be locked
while the window is open.

The `crypto`, `import`, `locked`, `bad_pin` and `backoff` arms are required.
[Spec §8](../product/spec.md#8-acceptance-criteria) means a user must be told
their store could not be opened rather than meeting a blank window.

## 5. Closed questions

| # | Question | Resolution |
| - | -------- | ---------- |
| 1 | Does plaintext cross IPC? | Yes, accepted. [ADR-0002](./adr/0002-threat-model.md). No security-motivated `copy_clip`. |
| 2 | What do `icon`, `visible`, `clear_time` mean? | Deleted. [Spec §3](../product/spec.md#3-data-model). |
| 3 | What is the list's ordering? | User drag-to-reorder, persisted. [Spec §4.3](../product/spec.md#43-reorder). |
| 4 | What is `colour` after Mantine? | Fixed named tokens. [Spec §7](../product/spec.md#7-colour). |
| 5 | Who mints `id`? | **The backend.** Owner's decision. See below. |
| 6 | Does the window's copy path reuse the tray's backend clipboard write? | **Yes, one command for both.** Forced by `use_count` ([spec §4.1](../product/spec.md#41-copy-a-clip)), not by security. |

### Identity (closed)

The backend mints every `id`. The frontend never generates one and never sends
one.

Consequences, binding on the architect:

- `new_clip` must not accept an `id`. Its argument is a clip *without* identity
  — label, value and colour only. The architect names that type and decides
  whether the command returns the created `Clip` or nothing and relies on the
  `update_clips` event.
- The frontend has no reason to depend on the `uuid` npm package. It is in
  `package.json` today only because `FastClip`'s constructor called it.
  `frontend-dev` removes the dependency.
- Any id arriving from the frontend on any command other than as a lookup key
  is `invalid_input`, not a value to trust.

## 6. Open questions for the architect

1. **`upsert` or two commands?** If separate, `update_clip` on an unknown id
   returns `not_found`.
2. **Order representation.** A `position` field per clip, or an ordered list
   plus `reorder(ids: string[])`? The second makes the order one fact; the
   first makes it N facts that can disagree. Choose and state why.
3. **On-disk format version field,** and how a decryption failure reaches the
   user. See [storage](./storage.md).
4. **Import atomicity.** A merge that fails halfway leaves the store untouched.
   Specify the mechanism, not only the requirement.
5. **What is the lock-state surface?** Is it a command the frontend polls, an
   event the backend emits, or both? The window and the tray must never
   disagree about whether the store is open.
6. **Does the copy command return before or after the `use_count` write?**
   [Spec §4.1](../product/spec.md#41-copy-a-clip) says the write completes
   first, which puts disk I/O on the hottest path. Specify the ordering and
   whether the `update_clips` event fires on every copy — at one event per
   click, the tray menu would rebuild constantly.
