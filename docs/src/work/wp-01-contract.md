# WP-01 — Contract ratification

**Objective:** close every open question in the IPC contract so both developers
can be dispatched against a single, unambiguous interface.

**Depends on:** nothing. This is the first package.

**Inputs:** the [specification](../product/spec.md), the
[contract](../architecture/contract.md), ADR-0001 and ADR-0002.

## Work

### architect

Close the five open questions in contract §6: upsert semantics, order
representation, whether the window's copy path reuses the tray's backend
clipboard write, the on-disk format version field, and import atomicity.

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

- Contract §6 is empty.
- Every command lists its error variants.
- An ADR exists for order representation.
- `critic` returns `ACCEPT`.

## Risks

A contract published with an unresolved question is implemented twice,
differently, and the divergence surfaces at WP-05. This package exists to
prevent that, so a fast pass through it is a false economy.
