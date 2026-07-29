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

## Failures

| Situation | Message |
| --------- | ------- |
| Store cannot be decrypted | must point at [import](./spec.md) as the recovery path |
| Store is from a newer version | |
| Disk write failed | |
| Import file malformed | must name what was wrong |
| Import file unreadable | |

## Security-sensitive strings

Both are claims about what FastClip protects. Overstating either is a
`BLOCK`-level finding.

| String | Constraint |
| ------ | ---------- |
| Export warning | States that the exported file is not encrypted, shown at the moment of export. |
| README security note | "Encrypted at rest, but not a password manager; does not protect against software running under your account." Do not soften to "your clips are secure". |

Both must match
[ADR-0002](../architecture/adr/0002-threat-model.md).
