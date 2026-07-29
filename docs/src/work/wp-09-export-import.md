# WP-09 — Export and import

**Objective:** the user can get their clips out and back in. This is the
recovery path that makes the encryption work safe to ship, so it ships first.

**Depends on:** [WP-05](./wp-05-crud.md). Export does not need encryption; it
needs the store and the UI. [WP-07](./wp-07-encryption.md) depends on **this**
package, not the other way round.

**Inputs:** [contract](../architecture/contract.md),
[spec §4.6](../product/spec.md#46-export-and-import-json).

## Work

### backend-dev

Export all clips to a user-chosen `.json` path.

Import: validate the whole file, then merge. Every imported clip is added with
a freshly minted `id`. Import never deletes or overwrites an existing clip, and
there is no replace-all in this version.

A merge that fails halfway leaves the store untouched. Use the atomicity
mechanism the architect specified.

A malformed or partially-valid file is rejected whole, with an error naming
what was wrong and where.

### frontend-dev

Export and import flows. The export warning states plainly that the exported
file is **not encrypted**, shown at the moment of export rather than buried in
a README. Take the exact wording from the [copy deck](../product/copy.md).

### test-engineer

Export, wipe the store, import: every clip returns. Import of a file with one
bad record changes nothing. Import of a file with colliding ids produces
distinct clips. Import of a truncated file is rejected.

### critic

The export file is plaintext by design. Check that this is disclosed in the UI
and that no code path writes an export the user did not request.

## Definition of done

- Round trip through export and import preserves every clip.
- A partially-valid import changes nothing.
- The plaintext warning is visible at export time.

## Risks

An import that half-applies is worse than one that fails. Test the failure path
harder than the success path.
