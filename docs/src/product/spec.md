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
| `use_count` | unsigned integer | backend-owned, starts at 0 |

Ordering is a property of the list, not of a clip. See [§4.3](#43-reorder).

`use_count` is the only field the user never edits. The backend increments it
and the frontend never writes it — an incoming `use_count` on any command is
`invalid_input`. It exists to rank the tray menu
([§4.5](#45-tray-icon-with-right-click-copy)) and has no other consumer.

It is a defined field with one purpose, which is what the three deleted fields
were not.

### Removed fields

`icon`, `visible` and `clear_time` are deleted. They are serialised to disk
today and read by nothing. Three undefined fields carried through a rewrite
produce three invented meanings.

There is no migration. Per
[ADR-0003](../architecture/adr/0003-no-legacy-migration.md), pre-refactor
stores are not read at all, so nothing needs to tolerate these fields.
Deserialisation is strict.

## 4. Features

Exactly these.

### 4.1 Copy a clip

Click a clip; its `value` goes to the clipboard.

The user receives clear, non-blocking confirmation. Without it they click twice
and paste twice. The current build has a commented-out fade animation reaching
for this; specify it once and build it once.

The full `value` is visible on hover.

**Every copy increments that clip's `use_count`, written through to disk before
the copy is reported complete.** A copy from the window and a copy from the tray
count identically — the user made the same gesture, and a ranking that ignored
half of them would be wrong.

This puts a disk write on the hottest path in the app. It is accepted for
accuracy, and it is a performance risk worth measuring: if a copy ever feels
slow, this is the first thing to look at.

### 4.2 Create, edit, delete

- **Create:** label, value, colour. Validation per [§3](#3-data-model), errors
  shown inline.
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
`value` without raising the window, and increments its `use_count` like any
other copy. Long labels are truncated in the menu.

**The menu shows at most ten clips**, chosen by:

1. descending `use_count`
2. then, if fewer than ten clips have ever been used, the user's list order
   ([§4.3](#43-reorder)) fills the remaining slots

So the menu is populated from a fresh install, and a clip added today is
reachable from the tray before it has been used once. Ties in `use_count` break
by list order.

The menu rebuilds whenever the clip list or any `use_count` changes.

**Architectural consequence:** the tray menu is native, so no webview is
involved and the backend writes the clipboard itself. Because every copy must
also increment a persisted `use_count`, the window's copy path must reach the
backend as well — so both paths go through **one backend copy command**. This
is settled by the counting requirement, not left to the architect.

It is not a reversal of
[ADR-0002](../architecture/adr/0002-threat-model.md). That ADR declined a
`copy_clip` command introduced *for security reasons*, and its reasoning still
stands: plaintext still crosses the IPC boundary, and the frontend still
receives `value` for the edit form and for search
([§4.7](#47-search)). The driver here is durable state, not confidentiality.

### 4.6 Export and import JSON

- **Export:** writes all clips to a user-chosen `.json` file.
- **Import:** merges. Every imported clip is added with a freshly minted `id`.
  Import never deletes or overwrites. There is no replace-all in this version.
- A malformed or partially-valid file is rejected whole, with a message naming
  what was wrong.
- `use_count` is **not** exported. It describes how this machine was used, not
  what a clip is, and importing someone else's counts would corrupt the
  ranking. Imported clips start at zero.

Export requires the store to be unlocked. The export file is plaintext,
because it must work when the encrypted store cannot be opened. The user is told this in the UI at the moment of export.

This feature is the recovery path that makes the encryption work safe to ship.

### 4.7 Search

A search box filters the list as the user types. A clip matches when the typed
text appears anywhere in its `label` **or** its `value`, compared
case-insensitively.

Substring containment only — not fuzzy, not regex, not word-boundary. A user
who remembers three characters of a snippet should find it.

**Appearance.** The box is hidden by default and revealed by a toolbar button
or `Ctrl+F`, which also focuses it. At 300×600 a permanent box costs a row that
would otherwise show a clip, and search is the exception rather than the
everyday path.

**Behaviour.**

- Filtering is client-side, over the list already in memory. There is no search
  command; a round trip per keystroke is wrong for a tool whose value is speed.
- The filtered list preserves the user's order
  ([§4.3](#43-reorder)). Matches are not ranked.
- `Escape` clears the query and hides the box. Clearing the query restores the
  full list without hiding the box.
- Search state is not persisted. Reopening the window shows the full list.
- The tray menu ([§4.5](#45-tray-icon-with-right-click-copy)) is never
  filtered. It is a native menu with no text input.

**Matching on `value` is deliberate**, and it means a clip can match on text
the user cannot see in the list — the label is often shorter than what they
remember typing. This is intended, not a defect to fix.

**Reordering is disabled while a filter is active.** Dragging within a filtered
subset has no unambiguous meaning in the full order, and the result would land
somewhere the user cannot see. Drag handles are inert whenever the query is
non-empty; clearing it restores them.

### 4.8 Encryption and unlocking

Encryption is **opt-in and off by default**. Most clips are not secrets, and
paying a prompt on every launch to protect a list of email signatures is a bad
trade. Governed by
[ADR-0004](../architecture/adr/0004-optional-pin-encryption.md).

**Enabling.** In settings, the user turns encryption on and sets a 6-digit PIN,
entered twice. The store is re-written encrypted. Before it completes, the user
is told plainly that **a forgotten PIN means the clips cannot be recovered**,
and is offered an export first.

**Unlocking.** With encryption on, launching FastClip shows a PIN prompt before
any clip. Nothing is listed, searched or copied until it is entered.

**Wrong PIN.** After five failures, attempts back off exponentially starting at
30 seconds. Nothing is ever wiped — a destructive lockout turns a mistyped PIN
into data loss, which is worse than the attack it prevents.

**Changing the PIN** requires the current one and is instant regardless of clip
count. **Disabling encryption** requires the PIN, then rewrites the store as
plaintext.

**While locked**, the application discloses nothing:

- no clip labels or values anywhere, including the tray
- the tray menu shows a single *Unlock FastClip* item, which raises the window
  with the PIN prompt focused. The PIN is never typed into a native menu.
- export is unavailable

**Portability.** An encrypted store is bound to the Windows account that
created it. It cannot be opened on another machine or by another user, by
design — that binding is what makes a 6-digit PIN safe. Export is how clips
move.

## 5. Security posture

Governed by [ADR-0002](../architecture/adr/0002-threat-model.md) for the IPC
boundary, which is **not** hardened, and by
[ADR-0004](../architecture/adr/0004-optional-pin-encryption.md) for encryption,
which is **opt-in** ([§4.8](#48-encryption-and-unlocking)).

| State | What is protected |
| ----- | ----------------- |
| Encryption off — the default | Nothing. The store is an unencrypted SQLite database. |
| Encryption on | The file is useless without the Windows account **and** the PIN. |

Neither state protects against software running under the user's account: it
can read the DPAPI blob and log the PIN as it is typed.

The user-facing claim must describe **the default**, which is unencrypted.
Softening the README on the strength of a feature most users will not enable
would be false. Where encryption is described, it is described as optional.

## 6. Non-goals

- fuzzy, regex or ranked search. Substring containment only ([§4.7](#47-search))
- analytics. `use_count` is a local ranking heuristic, never transmitted, never
  aggregated, and never shown as a statistic to the user
- clipboard history, or capturing anything the user did not enter
- copying files or non-text content
- **global** keyboard shortcuts — system-wide hotkeys that fire a clip while
  another application has focus (considered, deferred). In-window shortcuts such
  as `Ctrl+F` and `Escape` ([§4.7](#47-search)) are in scope; they are a
  different mechanism and require no OS-level registration.
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

1. No React and no Mantine remain, proven by `npm ls`. Tailwind is the only
   styling system ([ADR-0006](../architecture/adr/0006-tailwind.md)).
2. `icon`, `visible` and `clear_time` appear nowhere in the codebase.
3. A pre-refactor store on disk is neither read nor modified. The new store
   uses a different filename, and starting the app with an old `db` file
   present produces an empty clip list and no error
   ([ADR-0003](../architecture/adr/0003-no-legacy-migration.md)).
4. Clip order is stable across restarts and matches what the user set.
5. With encryption off, the store is plaintext. With encryption on, it is
   unreadable in a text editor, and opening it requires both the Windows
   account and the PIN.
6. No clip `value` appears in any log, in stdout, or in any file other than the
   encrypted store and a user-initiated export.
7. Export, wipe the store, import: every clip returns.
8. Tray right-click copies without raising the window, and its menu shows at
   most ten clips ranked by `use_count`, filled out by list order.
9. A copy from either path increments `use_count` and the new value survives an
   immediate process kill.
10. With encryption on, a locked FastClip lists no label or value anywhere,
    including the tray, and five wrong PINs trigger backoff without destroying
    data.
11. Typing in the search box filters the list on `label` and `value`,
    case-insensitively, and reordering is inert while a query is active.
12. `cargo test`, `cargo clippy -- -D warnings`, `svelte-check` and `vitest run`
    pass in CI from a clean checkout.
13. The README's security claim matches [§5](#5-security-posture) and describes
    the unencrypted default.

## 9. Open for the architect at G0b

The open engineering questions live in
[contract §6](../architecture/contract.md#6-open-questions-for-the-architect)
and are not restated here. This page listed them once and had already drifted
out of step with that list, which is the argument against keeping two copies.

Two questions that touched this specification are settled:

| Question | Resolution |
| -------- | ---------- |
| Does the window's copy path reuse the tray's backend clipboard write? | Yes, one command for both. `use_count` ([§4.1](#41-copy-a-clip)) requires it. |
| Who mints `id`? | The backend. See [contract §5](../architecture/contract.md#5-closed-questions). |
