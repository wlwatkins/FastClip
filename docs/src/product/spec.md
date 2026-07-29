# Specification

**Status:** Accepted at gate G0a.
**Read before:** the [IPC contract](../architecture/contract.md), which is
derived from this page. If they disagree, the contract is wrong.

If something you need is not specified here, stop and ask.

## 1. Purpose

FastClip stores labelled text snippets for people who retype the same text many
times a day: support replies, code snippets, addresses, ticket templates.

The product value is speed. A feature that adds a step to "see it, click it,
paste it" is the wrong feature.

The window is 300×600, undecorated and transparent, expected to sit docked at a
screen edge or always-on-top beside the app being worked in.

## 2. Users

One user, one machine. No accounts, no network. FastClip never makes an
outbound connection, so anything requiring one is out of scope.

## 3. Data model

| Field | Type | Constraint |
| ----- | ---- | ---------- |
| `id` | UUID | backend-minted, stable for the clip's life |
| `label` | string | 1–100 characters, required |
| `value` | string | 1–10 000 characters, required |
| `colour` | [palette token](./palette.md) | required |

Ordering is a property of the list, not of a clip. See §4.3.

### Removed fields

`icon`, `visible` and `clear_time` are deleted. They are serialised to disk
today and read by nothing. Three undefined fields carried through a rewrite
produce three invented meanings.

Migration must tolerate them in existing data and drop them. Do not use
`deny_unknown_fields` on that path.

## 4. Features

Exactly these.

### 4.1 Copy a clip

Click a clip; its `value` goes to the clipboard.

The user receives clear, non-blocking confirmation. Without it they click twice
and paste twice. The current build has a commented-out fade animation reaching
for this; specify it once and build it once.

The full `value` is visible on hover.

### 4.2 Create, edit, delete

- **Create:** label, value, colour. Validation per §3, errors shown inline.
- **Edit:** the same form, pre-filled. `id` never changes.
- **Delete:** requires confirmation. It is irreversible and its button sits
  beside one the user clicks all day.

### 4.3 Reorder

The user drags clips into an order that persists across restarts.

The store is a `HashMap` today, so list order is non-deterministic and changes
between calls — buttons move under the cursor. This is a live bug as well as a
missing feature. The store becomes an ordered structure.

A crash mid-drag leaves either the old order or the new one, never a partial
one.

### 4.4 Always-on-top

A toggle, persisted across restarts. Already implemented; keep the behaviour.

### 4.5 Tray icon with right-click copy

A tray icon whose right-click menu lists clips by label. Choosing one copies its
`value` without raising the window. The menu rebuilds when the clip list
changes. Long labels are truncated in the menu.

**Architectural consequence:** the tray menu is native, so no webview is
involved and the backend must write to the clipboard itself. A backend-side
copy capability therefore exists regardless of
[ADR-0002](../architecture/adr/0002-threat-model.md), whose reasoning was about
security and still stands. The architect decides at G0b whether the window's
copy path reuses it. Two independent clipboard code paths must not appear by
accident.

### 4.6 Export and import JSON

- **Export:** writes all clips to a user-chosen `.json` file.
- **Import:** merges. Every imported clip is added with a freshly minted `id`.
  Import never deletes or overwrites. There is no replace-all in this version.
- A malformed or partially-valid file is rejected whole, with a message naming
  what was wrong.

The export file is plaintext, because it must work when the encrypted store
cannot be opened. The user is told this in the UI at the moment of export.

This feature is the recovery path that makes the encryption work safe to ship.

## 5. Security posture

Governed by [ADR-0002](../architecture/adr/0002-threat-model.md). The store is
encrypted at rest; the IPC boundary is not hardened.

The user-facing claim, which must not be overstated: clips are encrypted on
disk, but FastClip is not a password manager and does not protect against
software running under the user's account.

## 6. Non-goals

- clipboard history, or capturing anything the user did not enter
- copying files or non-text content
- global keyboard shortcuts (considered, deferred)
- cloud sync, accounts, telemetry, update checks, any network call
- multi-user or shared clip sets
- platforms other than Windows. Nothing is tested or shipped for them; the code
  should not actively prevent them.

## 7. Colour

Colour is a scanning aid: with thirty clips it is how a user finds one without
reading. It is a fixed set of named tokens, never free hex, because free choice
lets a user render their own label unreadable.

Requirements and the token table are on the [palette](./palette.md) page.

## 8. Acceptance criteria

1. No React, Mantine or Tailwind remains, proven by `npm ls`.
2. `icon`, `visible` and `clear_time` appear nowhere in the codebase.
3. A user upgrading from the current build keeps every clip with its label,
   value and an equivalent colour. Verified by a test against a real
   pre-refactor database file.
4. Clip order is stable across restarts and matches what the user set.
5. The store is unreadable in a text editor.
6. No clip `value` appears in any log, in stdout, or in any file other than the
   encrypted store and a user-initiated export.
7. Export, wipe the store, import: every clip returns.
8. Tray right-click copies without raising the window.
9. `cargo test`, `cargo clippy -- -D warnings`, `svelte-check` and `vitest run`
   pass in CI from a clean checkout.
10. The README's security claim matches §5.

## 9. Open for the architect at G0b

1. Is `update_clip` distinct from `new_clip`, or is there one `upsert`?
2. How is order represented on the wire and on disk?
3. Does the window's copy path reuse the tray's backend clipboard write?
4. The on-disk format version field, and how a failed migration reaches the
   user.
5. Import atomicity.

Identity is closed: the backend mints every `id`. See
[contract §5](../architecture/contract.md).
