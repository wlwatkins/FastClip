# WP-03 — Storage: ordered and crash-safe

**Objective:** replace the `HashMap` store with an ordered, crash-safe one.
No encryption yet.

**Depends on:** WP-01.

**Inputs:** [storage design](../architecture/storage.md), contract, spec §4.3.

## Work

### backend-dev

Replace the `HashMap<Uuid, Clip>` with the ordered structure the architect
specified. List order becomes deterministic and user-controlled rather than an
artefact of hash iteration.

Make `save()` crash-safe: write to a temporary file, flush, rename. It
currently writes over the live database, so a crash mid-write destroys every
clip.

Add the format version field now, before there is data in the wild to migrate.

Remove `icon`, `visible` and `clear_time` from the `Clip` struct. Deserialising
existing files must ignore them rather than fail.

Replace `expect()` in `DataBase::new()` with typed errors.

Decide the SurrealDB question: it is declared in `Cargo.toml` and imported
nowhere. Use it or remove it, and record the reasoning.

### test-engineer

Round-trip serialisation. Insert, update, remove and list against a temporary
directory. Ordering stable across a save and load cycle. A truncated file fails
cleanly rather than panicking. A file containing the three removed fields loads
and drops them.

### critic

Weight data loss first. Can any sequence of calls leave the store truncated,
partially written, or reordered?

## Definition of done

- Order is stable across restarts.
- A process killed mid-save leaves the previous store intact.
- No `unwrap()` or `expect()` reachable after startup.
- `surrealdb` is used or gone.

## Risks

This package changes the on-disk format. Getting the version field wrong here
makes WP-07's migration harder than it needs to be.
