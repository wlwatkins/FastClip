# WP-01 — Contract ratification

**Objective:** close every open question in the IPC contract so both developers
can be dispatched against a single, unambiguous interface.

**Depends on:** nothing. This is the first package.

**Inputs:** the [specification](../product/spec.md), the
[contract](../architecture/contract.md), and **all six accepted ADRs**
([index](../architecture/adr/index.md)). ADR-0004 and ADR-0005 define the
encryption and storage surface you are about to ratify; do not skip them.

## Work

### architect

Close the six open questions in
[contract §6](../architecture/contract.md#6-open-questions-for-the-architect):
upsert semantics, order representation, the on-disk format version field,
import atomicity, the lock-state surface, and whether the copy command returns
before or after the `use_count` write.

Also choose the SQLite crate, on the evidence of the
[WP-02](./wp-02-toolchain.md) spike, and record it in
[ADR-0005](../architecture/adr/0005-sqlite-store.md).

Define the discriminated error type, including the `crypto` and `import`
variants. Specify the new command surface required by reordering, tray copy,
export and import — names, arguments, wire casing, errors, events.

Write an ADR for the order representation. It closes off an option either way.

Where a question turns out to be a product decision, escalate rather than
answer it.

### critic

Review the ratified contract before any code is written against it. Check that
every command has a defined error set, that no `TBD` remains, and that the
contract does not contradict the specification.

### Others

No work. Do not dispatch them.

## Definition of done

- [Contract §6](../architecture/contract.md#6-open-questions-for-the-architect) is empty.
- Every command lists its error variants.
- An ADR exists for order representation.
- `critic` returns `ACCEPT`.

## Risks

A contract published with an unresolved question is implemented twice,
differently, and the divergence surfaces at WP-05. This package exists to
prevent that, so a fast pass through it is a false economy.
