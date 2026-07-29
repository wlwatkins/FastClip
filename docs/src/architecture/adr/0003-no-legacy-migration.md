# ADR 0003 — No migration from pre-refactor data

**Status:** Accepted
**Deciders:** owner

## Context

The pre-refactor build stores clips as plaintext JSON at
`%LOCALAPPDATA%\FastClip\db`, with three fields the refactor deletes (`icon`,
`visible`, `clear_time`) and a `colour` field holding free-form rgba strings
from a colour picker.

The refactor changes the on-disk format twice: to an ordered structure
([WP-03](../../work/wp-03-storage.md)), then to an encrypted one
([WP-07](../../work/wp-07-encryption.md)). Carrying existing data across both
was planned, and was identified as the highest-risk work in the project — a
one-time conversion running on a machine nobody can inspect, against a file
nobody wrote.

## Decision

**No migration. Pre-refactor stores are not read.**

The new store is written to `~/.fast-clip/`. The existing
`%LOCALAPPDATA%\FastClip\db` is left where it is, untouched and unread.

## Rationale

FastClip is a proof of concept with no distributed user base. The owner has
stated that losing existing clips is acceptable.

Against that, migration would have cost:

- the most dangerous code in the project, running exactly once per user
- a checked-in fixture of a real pre-refactor store, and tests for empty,
  already-migrated, truncated, partially-written and unknown-field cases
- perceptual nearest-token resolution to convert arbitrary rgba strings to
  palette tokens
- a failure path surfacing "your database could not be converted" to the user,
  and a recovery route when it fired

Every one of those is now unnecessary. The new format is clean from its first
release rather than shaped by what it had to read.

## Consequences

- **Anyone with existing clips loses them on upgrade.** They are not deleted —
  the old file remains on disk — but the application will not show them. If
  that ever matters, the old file is plaintext JSON and can be read by hand.
- The new store lives in a **different directory** — `~/.fast-clip/`, not
  `%LOCALAPPDATA%\FastClip\`. Reusing the old path would mean either
  overwriting a user's file or reading it, and the point of this decision is to
  do neither. A separate directory makes that structural rather than a naming
  convention.
- Deserialisation can be **strict**. `deny_unknown_fields` is now available,
  and an unknown field in the store is a defect rather than a legacy artefact.
- The palette needs **no migration map**. Tokens are chosen for legibility
  alone, with no obligation to approximate an existing colour.
- WP-07 shrinks to encryption. WP-03 no longer carries a compatibility
  requirement.
- Spec acceptance criterion 3, which required a verified upgrade path, is
  removed.

## Revisit if

FastClip acquires users other than the owner before the rewrite ships. At that
point the trade changes, because the cost is no longer paid by the person who
chose it.
