# User-facing copy

**Status:** Written. `src/lib/copy.ts` and `src/lib/errorMessage.ts` implement
this page; a call site quotes a constant or a function from one of the two,
never a literal string. Where this page and the code disagree, this page is
the defect — file it against `frontend-dev`.

Every string the user reads, in one place. Error messages written inline by
whichever agent was in that file come out inconsistent in tone and vague about
what to do next.

## Rules

- British spelling.
- Say what happened and what to do. "Error" is not a message.
- Never show a raw error. The
  [error contract](../architecture/contract.md) exists so the frontend can map
  each variant to a sentence. Map all of them.
- No blame.
- No string interpolates a clip's `value` or a search query. A clip's `label`
  is not covered by this rule — it is already visible in the list the message
  refers to, and naming it is what lets the user tell one confirmation or
  error from another when more than one clip is on screen.

## Validation

Inline, shown beside the field, before the round trip that would catch the
same thing server-side (contract §4 "Both sides validate").

| Situation | Message |
| --------- | ------- |
| Empty label | "This field is required." |
| Label over 100 characters | "This is too long." |
| Label contains a control character (tab, newline, or similar) | "This contains a character that is not allowed." |
| Empty value | "This field is required." |
| Value over 10 000 characters | "This is too long." |
| Colour is not a palette token | "Choose a colour from the list." (unreachable through the UI's own colour picker; present because `invalid_input` is still a declared error for `create_clip`/`update_clip`) |
| PIN is not exactly 6 digits | "Enter exactly 6 digits." |
| PINs entered twice do not match (enable, change) | "The PINs do not match." |
| A payload the frontend built carries a field the backend does not permit (`id` on a `ClipDraft`, or any other value only the backend may set) | "That value cannot be set here." (unreachable through this frontend's own forms, which never send such a field; present because `not_permitted` is a declared `InvalidReason`) |
| A payload the frontend built carries a field the backend does not recognise | "Something in this request was not understood." (unreachable the same way — this frontend never sends an unrecognised field; present because `unknown_field` is declared) |
| `reorder_clips`'s `order` argument names an id that is not in the store, is missing one, or duplicates one | "The list changed elsewhere. Reloading." — the frontend re-fetches the list, because the id set changed under it between the last list and this reorder |
| An element of `reorder_clips`'s `order` array is not a valid id | "Something in this request was not understood." (contract §2 `reorder_clips`: the frontend built this array from ids the backend gave it, so this is a frontend defect, not a user mistake; present because `malformed_uuid` is declared) |

`required` and `too_long` share one sentence between label and value: the
field the message sits beside already says which one failed, so repeating the
field name in the sentence would be saying it twice.

## Confirmation

| Situation | Message |
| --------- | ------- |
| Clip copied | Toast: `Copied "{label}"`. Non-blocking, disappears on its own (spec §4.1: "clear, non-blocking confirmation"). |
| Delete confirmation | Dialog title "Delete clip", body `Delete "{label}"? This cannot be undone.`, confirm button "Delete", cancel button "Cancel". |
| Clip deleted | No message. The row leaving the list is the confirmation; a toast on top of a dialog that already asked for confirmation would be a second dialogue for one action. |

## Search

| Situation | Message |
| --------- | ------- |
| Search box placeholder | "Search label or value…" |
| No clips match the query | "No clips match your search." — distinct from "No clips yet.", which is shown only when the library itself is empty and no search is active, so the two are never confusable. |
| Reorder disabled while filtering | `aria-label` on the inert drag handle: "Reorder {label}. Disabled while a search is active — clear the search to reorder." |
| Reorder handle, search inactive | `aria-label`: "Reorder {label}. Press arrow up or arrow down to move, or drag." |

## Encryption and PIN

| Situation | Message |
| --------- | ------- |
| Settings toggle label | "PIN protection", with a state readout of "On" or "Off" beside it. |
| Enable button | "Turn on PIN protection…" |
| Enable: PIN entry prompt | Dialog title "Turn on PIN protection", fields "6-digit PIN" and "Confirm PIN", submit "Turn on". |
| Enable: PIN confirmation mismatch | "The PINs do not match." |
| **Enable: irreversibility warning** | "If you forget this PIN, your clips cannot be recovered. There is no reset and no backdoor. Export a copy first if you want a fallback." Shown before the PIN is ever asked for, with an "Export clips first…" action beside "Continue". Must not be softened — ADR-0004: "A forgotten PIN means the clips are unrecoverable. There is no reset and no backdoor." |
| Unlock prompt at launch | Title "FastClip is locked", field "PIN", button "Unlock". Shown in place of the clip list; nothing is listed, searched or copied until it is entered (spec §4.8). |
| Wrong PIN | Alert: "Wrong PIN." alongside a status line giving the count (below). |
| Attempts remaining (status line, shown once the store has reported a count) | "{n} attempt remaining." / "{n} attempts remaining." |
| Backoff active | "Too many attempts. Try again in {n} second." / "…in {n} seconds." **Flat wording only** — the wait is a flat 30 seconds with no escalation and no ceiling (ADR-0011), so the sentence must never imply the wait grows with further attempts. |
| Change PIN | Dialog title "Change PIN", fields "Current PIN", "New PIN", "Confirm new PIN", submit "Change PIN". |
| Disable encryption confirmation | Dialog title "Turn off PIN protection", body "Your clips will be stored unencrypted on this computer.", field "Current PIN", submit "Turn off". Must say plainly that the store becomes plaintext, which it does. |
| Manual lock | Button "Lock now", visible only once encryption is on (spec/ADR-0010: manual lock, no idle timeout, no lock-on-minimise — the user is the only trigger). |
| Tray item while locked, and the PIN prompt's landmark `aria-label` | "Unlock FastClip" — names no clip (ADR-0004: "A locked FastClip discloses nothing."). The tray item lives in the Rust tray menu (`tray.rs`) and cannot share a constant with the frontend across the seam; the frontend's own copy is `UNLOCK_LANDMARK_LABEL` in `copy.ts`. Both name the same action and must stay the same string — recorded here because a change to one that is not made to the other is a defect this page cannot catch on its own. |
| Settings while locked | "FastClip is locked. Enter your PIN to manage PIN protection, export or import." — shown in place of the encryption controls, export and import (both `locked` while the store is not open). |
| Encryption turned on / off / PIN changed | Toasts: "PIN protection is on.", "PIN protection is off.", "PIN changed." |

Nothing above ever interpolates a PIN value. A PIN is never logged, put in a
toast, or passed to `errorMessage.ts`.

## Export and import

| Situation | Message |
| --------- | ------- |
| Export button | "Export clips…" |
| **Export plaintext warning** | "The exported file is not encrypted. Anyone who can open it can read every clip in it, even if FastClip's PIN protection is on. Choose where you save it, and delete it once you no longer need it." Shown at the moment of export, before the file dialog opens. Must not be softened — the file is plaintext regardless of whether PIN protection is on, because encryption protects the store, not this export. |
| Export succeeded | Toast: "Exported 1 clip." / "Exported {n} clips." |
| Import button | "Import clips…" |
| Import succeeded | Toast: "Imported 1 clip." / "Imported {n} clips." |
| Export unavailable while locked | Covered by "Settings while locked" above — the Export and Import buttons are hidden rather than shown disabled, because both return `locked` while the store is not open and spec §4.8 says export is unavailable while locked outright. |

## Failures — every `ClipError` variant

The frontend maps **every** `ClipError.kind` to a sentence. This table is
exhaustive against [contract §4](../architecture/contract.md#4-errors); a
variant with no row here is a defect in this page, not a licence to write one
inline.

| `kind` | Condition | Message |
| ------ | --------- | ------- |
| `not_found` | A `clip_id` names no stored clip — it was deleted elsewhere between the frontend's last list and this call. | "That clip no longer exists." The frontend re-fetches the list; no user action is asked for. |
| `invalid_input` | An argument failed validation. | The [Validation](#validation) sentence for `reason`, keyed off `field` where the field is on screen. |
| `storage` | The store could not be read or written — including a `clips.db` present but whose header cannot be read at all, which is the commonest cause and is a locked file, not damage. | "FastClip could not read or write the clip store. If another program — including another copy of FastClip — has it open, close it and try again. Nothing has been changed." Never "disk write failed": this variant covers reads too, and never suggests re-importing — see [below](#the-unreadable-store-sentence). |
| `io` | A user-chosen export or import file could not be reached. Never the store. | "The file could not be reached." |
| `crypto`, `reason: "bad_key_material"` | The key material is missing, unreadable, or was created by another Windows account or machine. | "This store is bound to a different Windows account or machine and cannot be opened here. Import a backup to recover your clips on this device." |
| `crypto`, `reason: "corrupt"` | The database will not open or fails its integrity check. | "This store could not be opened. Import a backup to recover your clips." |
| `unsupported_version` | An on-disk artefact (schema or key material) is from a newer version of FastClip than this build understands. | "This store is from a newer version of FastClip." |
| `import` | The import file's content was rejected. | See [describeImportError](#import-file-failures) below. |
| `locked` | Encryption is on and the store is not open. | The PIN prompt is shown in place of whatever was requested; no separate sentence — the prompt itself is the answer. |
| `wrong_state` | The command needs the store in a state it is not in. Reachable only behind a control this page keeps hidden in the state that would produce it, so it is a caught defect rather than an expected user path. | "That action is not available right now." |
| `bad_pin` | The PIN was evaluated and did not unwrap the DEK. | "Wrong PIN." plus the attempts-remaining status line ([above](#encryption-and-pin)). Where the caller is not subject to backoff — `disable_encryption`, `change_pin` — `attempts_remaining` is `null` and only "Wrong PIN." is shown. |
| `backoff` | An unlock was attempted while a wait was already running; the PIN was not evaluated. | The backoff sentence ([above](#encryption-and-pin)), rendered with the live countdown wherever one is available. `describeError`'s single-sentence surfaces have no `secondsRemaining` to hand, so their fallback is "Too many attempts. Wait and try again." — silent on duration rather than wrong about it, and still true under the flat 30-second wait (ADR-0011: no doubling, no ceiling). |
| `clipboard` | The clipboard could not be written. `use_count` was not incremented. | "The clip could not be copied." |
| `internal` | An unmodelled failure. | "An unexpected error occurred." |

### The unreadable-store sentence

`storage` covers two conditions that must not be told apart in the message,
because the frontend cannot tell them apart either: a store that could not be
created or reached at all, and a `clips.db` that exists and is intact but is
currently unreadable — most often because another process, including another
running copy of FastClip, has it open.

The second case is **not damage**. Nothing was deleted, nothing was created,
and closing the other process or waiting it out recovers the store whole
([storage.md](../architecture/storage.md#classifying-clipsdb)). Every other
row in this table points at export/import as the recovery path, because after
[ADR-0005](../architecture/adr/0005-sqlite-store.md) that is the only one that
exists — **except this one.** Telling a user whose store is merely locked to
re-import is how they replace a store that was completely intact with an
empty one, which is a worse outcome than the fault itself. The sentence above
says "close it and try again" and never mentions import.

### Import-file failures

`describeImportError` builds a sentence from `reason`, `field` and `index`
(contract §4: `index` is 0-based, `field` names the offending clip field,
either may be `null` when the fault is at the top level).

| `reason` | Whole-file sentence | Per-clip fragment |
| -------- | -------------------- | ------------------ |
| `malformed_json` | "This is not a FastClip export file." | — (top-level only) |
| `unsupported_version` | "This export file is from a newer version of FastClip." | — (top-level only) |
| `missing_field` | — | "is missing a required field" |
| `unknown_field` | — | "has a field FastClip does not recognise" |
| `invalid_value` | — | "has an invalid value" |

The per-clip fragment is assembled as `Clip {index+1} {fragment} ("{field}").`,
or `The file {fragment}.` when `index` is `null`. Neither ever quotes the
clip's `value`.

**`field` is truncated to 60 characters before display, with `…` appended
when it was cut.** [Contract §4](../architecture/contract.md#field-echoes-a-key-from-the-file-and-has-no-length-bound)
delegates this length to this page: `field` is a key copied verbatim from the
import file and has no maximum length, so an unbounded echo can be megabytes
long. 60 characters is enough for a reader to recognise which key of their own
file was rejected — the sentence's job — without a toast rendering a wall of
the user's file content. Truncation counts Unicode code points, not UTF-16
code units, so a cut never lands inside a surrogate pair.

## Security-sensitive strings

Both are claims about what FastClip protects. Overstating either is a
`BLOCK`-level finding.

| String | Constraint | Wording |
| ------ | ---------- | ------- |
| Export warning | States that the exported file is not encrypted, shown at the moment of export. | See [Export and import](#export-and-import) above. |
| README security note | Must describe **the default, which is unencrypted** ([ADR-0004](../architecture/adr/0004-optional-pin-encryption.md)). | See `README.md` — "Security". Does not soften to "your clips are secure", and does not claim encryption is on by default. |

Both are checked against [ADR-0004](../architecture/adr/0004-optional-pin-encryption.md),
which superseded [ADR-0002](../architecture/adr/0002-threat-model.md) on
exactly this point: encryption is opt-in and off by default, so any wording
that reads as "encrypted at rest" without that qualifier is false for every
user who has not opened settings.
