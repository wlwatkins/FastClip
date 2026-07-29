# User-facing copy

**Status:** Not yet written. Required before G1 completes.

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

## Validation

| Situation | Message |
| --------- | ------- |
| Empty label | |
| Label over 100 characters | |
| Empty value | |
| Value over 10 000 characters | |

## Confirmation

| Situation | Message |
| --------- | ------- |
| Clip copied | |
| Delete confirmation | |
| Clip deleted | |

## Search

| Situation | Message |
| --------- | ------- |
| Search box placeholder | |
| No clips match the query | must make clear it is a filter, not an empty library |
| Reorder disabled while filtering | shown on hover over an inert drag handle |

## Encryption and PIN

| Situation | Message |
| --------- | ------- |
| Settings toggle label | |
| Enable: PIN entry prompt | |
| Enable: PIN confirmation mismatch | |
| **Enable: irreversibility warning** | must say plainly that a forgotten PIN means the clips cannot be recovered, and offer an export first |
| Unlock prompt at launch | |
| Wrong PIN | includes attempts remaining |
| Backoff active | states how long to wait |
| Change PIN | |
| Disable encryption confirmation | must say the store becomes plaintext |
| Tray item while locked | "Unlock FastClip" or equivalent; must not name any clip |
| Export unavailable while locked | |

## Failures

| Situation | Message |
| --------- | ------- |
| Store cannot be decrypted | must point at [import](./spec.md) as the recovery path |
| Store is from a newer version | |
| Store cannot be opened on this machine or account | encrypted stores are account-bound; point at import |
| Disk write failed | |
| Import file malformed | must name what was wrong |
| Import file unreadable | |

## Security-sensitive strings

Both are claims about what FastClip protects. Overstating either is a
`BLOCK`-level finding.

| String | Constraint |
| ------ | ---------- |
| Export warning | States that the exported file is not encrypted, shown at the moment of export. |
| README security note | Must describe **the default, which is unencrypted** ([ADR-0004](../architecture/adr/0004-optional-pin-encryption.md)). Along the lines of *"clips are stored unencrypted unless you turn on PIN protection in settings. Even then, FastClip is not a password manager and does not protect against software running under your account."* Do not soften to "your clips are secure", and do not claim encryption is on. |

Both must match
[ADR-0002](../architecture/adr/0002-threat-model.md).
