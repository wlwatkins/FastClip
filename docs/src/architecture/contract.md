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

`icon`, `visible` and `clear_time` are removed and must not appear in any new
type. Deserialising existing on-disk data ignores them rather than failing; do
not use `deny_unknown_fields` on the migration path.

The architect decides whether `colour` is a Rust `enum` — which rejects unknown
values at the boundary but makes adding a colour a breaking change — or a
validated `String`.

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
  a clip. Open question 2.
- `new_clip` accepts a client-supplied `id`. Identity belongs to the backend
  (§5), so this signature must change.

### New surface required by the specification

The architect specifies each fully — names, arguments, errors, events.

| Feature | Needs |
| ------- | ----- |
| [Reorder](../product/spec.md) §4.3 | a way to persist user ordering |
| [Tray copy](../product/spec.md) §4.5 | backend-side clipboard write |
| [Export](../product/spec.md) §4.6 | serialise all clips to a chosen path |
| [Import](../product/spec.md) §4.6 | validate, then merge with new ids |

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
```

The `crypto` and `import` arms are required.
[Acceptance criterion 3](../product/spec.md) means a user must be told their
database could not be opened rather than meeting a blank window.

## 5. Closed questions

| # | Question | Resolution |
| - | -------- | ---------- |
| 1 | Does plaintext cross IPC? | Yes, accepted. [ADR-0002](./adr/0002-threat-model.md). No security-motivated `copy_clip`. |
| 2 | What do `icon`, `visible`, `clear_time` mean? | Deleted. [Spec §3](../product/spec.md). |
| 3 | What is the list's ordering? | User drag-to-reorder, persisted. [Spec §4.3](../product/spec.md). |
| 4 | What is `colour` after Mantine? | Fixed named tokens. [Spec §7](../product/spec.md). |
| 5 | Who mints `id`? | **The backend.** Owner's decision. See below. |

### Identity — closed

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

## 6. Open questions — architect, G0b

1. **`upsert` or two commands?** If separate, `update_clip` on an unknown id
   returns `not_found`.
2. **Order representation.** A `position` field per clip, or an ordered list
   plus `reorder(ids: string[])`? The second makes the order one fact; the
   first makes it N facts that can disagree. Choose and state why.
3. **Does the window's copy path reuse the tray's backend clipboard write?**
   The tray forces a backend-side write to exist ([spec §4.5](../product/spec.md)).
   The driver is architectural, not security, so this does not reopen ADR-0002.
4. **On-disk format version field,** and how a failed migration reaches the
   user. See [storage](./storage.md).
5. **Import atomicity.** A merge that fails halfway leaves the store untouched.
   Specify the mechanism, not only the requirement.
