# ADR 0007 — List order is a property of the list, not of a clip

**Status:** Accepted at G0b.
**Deciders:** architect

## Context

[Contract §6](../contract.md#6-open-questions-for-the-architect) question 2 asked
for a choice between "a `position` field per clip" and "an ordered list plus
`reorder(ids: string[])`".

That question was already answered in part, and the two answers looked like a
contradiction:

| Source | Says |
| ------ | ---- |
| [Spec §3](../../product/spec.md#3-data-model) | "Ordering is a property of the list, not of a clip." |
| [ADR-0005](./0005-sqlite-store.md) | "Clips live in a table with a `position` column for user ordering." |
| [Contract](../contract.md) | Where the contract and the specification disagree, the contract is the defect. |

They contradict only if the wire and the store are treated as one layer. SQL has
no intrinsic row order, so a table needs a column to hold one. An IPC payload is
an array, which already has one.

## Decision

**Order is carried by array position on the wire, and by a dense `position`
column in the store.** One fact, one owner, two representations.

| Layer | Representation | Written by |
| ----- | -------------- | ---------- |
| Wire | Index within the `Clip[]` array of `list_clips` and `update_clips`. The `Clip` type has **no** order field. | backend only |
| Store | `position INTEGER NOT NULL UNIQUE`, dense, `0..N-1`, no gaps. | backend only |

The reorder command is:

```ts
await invoke("reorder_clips", { order: ["<uuid>", "<uuid>", …] });
```

`order` is the complete list of every clip id in the new display order. It is
not a delta and not a move. The backend rejects it unless it is an exact
permutation of the stored id set — same length, same members, no duplicates —
with `invalid_input { field: "order", reason: "not_a_permutation" }`.

The `position` column is rewritten wholesale inside one transaction, so a crash
leaves the old order or the new one and never a mixture
([spec §4.3](../../product/spec.md#43-reorder)).

## Rationale

**One fact cannot disagree with itself.** A `position` on each clip is N facts.
Nothing in SQL or in serde prevents two rows claiming position 4, and the defect
surfaces as buttons swapping under the cursor — the same class of bug as the
`HashMap` iteration order this refactor exists to remove.

**A full permutation is checkable; a delta is not.** The backend can prove the
incoming order is the same set it holds. A stale client — one that computed a
drag against a list from before another clip was created — is rejected whole
rather than half-applied.

**The store's column is derived, not authoritative.** Only the backend writes
it, only the backend reads it, and it is regenerated from the array on every
reorder. It is the materialisation of the list's order, which is what a database
column has to be when the list is the fact.

### Alternatives rejected

| Alternative | Rejected because |
| ----------- | ---------------- |
| `position` field on the wire `Clip` | Contradicts [spec §3](../../product/spec.md#3-data-model). Makes order N facts that can disagree, and forces every create and update to carry an order the frontend would have to compute. |
| `move_clip { clip_id, new_index }` delta | Cannot be validated against the stored set. Index semantics are ambiguous once the list has changed underneath, and two calls arriving out of order leave an order nobody chose. |
| Linked list (`after_id` on each clip) | A broken link is unrepairable and a cycle is not detectable in one query. Worse than a `position` column on every count that matters here. |
| Sparse positions (gaps of 1000) so a move is one `UPDATE` | Only pays off with a delta command, which is rejected above. With a full permutation the backend rewrites every row regardless, so sparsity buys nothing and gives up the "dense `0..N-1`" invariant a test can assert. |

## Consequences

- **The reorder payload is N ids.** At 36 bytes per id, 200 clips is about 7 KB
  per drop. Sent once per drag, not per pixel.
- **`UNIQUE(position)` blocks an in-place permutation.** Setting row A to
  position 2 while row B still holds it fails. Within the one transaction, the
  backend first offsets every position by `+N`, then writes the final values.
  The constraint is kept because it makes a duplicate position impossible on
  disk rather than merely unlikely.
- **Reordering is impossible from a filtered list**, which is why
  [spec §4.7](../../product/spec.md#47-search) disables drag handles while a
  query is active. The frontend cannot construct a full permutation from a
  subset it can see. The specification's rule and this representation reinforce
  each other.
- **A rejected reorder needs a defined recovery.** On
  `invalid_input { field: "order", reason: "not_a_permutation" }` the frontend
  discards the drag and renders the most recent `update_clips` payload. It does
  not retry with the same order.
- **[WP-06](../../work/wp-06-reorder.md)'s stated risk is removed structurally.**
  Its note "if the ADR chose a `position` field per clip, N records can disagree"
  no longer applies; the test that replaces it asserts the dense `0..N-1`
  invariant after every mutation, including delete and import.
- **Deleting a clip renumbers the rows after it.** A delete is a position
  rewrite as well as a row removal, in the same transaction.

## Revisit if

Clip counts reach the thousands, where sending the whole order on every drop
stops being trivial. The replacement is a fractional index or an `after_id`
scheme, and it trades away the whole-set validation that makes a stale client
detectable.
