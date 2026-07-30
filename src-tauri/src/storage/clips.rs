//! Row-level operations on the `clips` table.
//!
//! This is the store's type, not the wire's. It carries `use_count`, which
//! never crosses the IPC seam (ADR-0008), and it is deliberately **not**
//! `Serialize`: the row type cannot become a payload by accident.
//!
//! Validation of `label` and `value` belongs at the IPC boundary and is not
//! repeated here — one rule, one owner (storage.md § Schema). `colour` needs no
//! validation at all here, because [`Colour`] is a closed enum and an
//! unrenderable token has no representation to reach this layer with.
//!
//! The `colour` column stays `TEXT` (WP-03, deliberately not enumerated): a
//! token is stored by name so the palette can be retuned without rewriting a
//! row. Reading one back is a parse, exactly like the `id` column's, and a value
//! this build cannot interpret is `storage` rather than a panic.

use std::collections::HashSet;
use std::fmt;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::colour::Colour;
use crate::error::{ClipError, InvalidReason};
use crate::redact::Redacted;
use crate::storage::storage_error;

const COLUMNS: &str = "id, label, value, colour, use_count, position";

/// One row of the `clips` table.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipRow {
    pub id: Uuid,
    pub label: String,
    pub value: String,
    pub colour: Colour,
    /// Backend-owned. Ranks the tray menu and has no other consumer.
    pub use_count: i64,
    /// Dense, `0..N-1`, no gaps (ADR-0007).
    pub position: i64,
}

/// Hand-written so that no `{:?}` anywhere — a log line, a panic message, an
/// assertion — can print what the user stored. ADR-0002 forbids a clip `value`
/// reaching a log at any level; the `label` is redacted on the same reasoning.
impl fmt::Debug for ClipRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipRow")
            .field("id", &self.id)
            .field("label", &Redacted::of(&self.label))
            .field("value", &Redacted::of(&self.value))
            .field("colour", &self.colour)
            .field("use_count", &self.use_count)
            .field("position", &self.position)
            .finish()
    }
}

/// A clip's content, before the backend has minted an identity for it.
///
/// The input shape of [`import`]. It borrows rather than owns because the caller
/// already holds the validated clips and an import of ten thousand rows should
/// not copy every `value` to hand them over.
///
/// **No `Debug`, by omission and on purpose.** It holds a `label` and a `value`,
/// so a derive here would be the one line that puts a user's clip into a log
/// (ADR-0002). Nothing formats it; [`ClipRow`] is the type that does, and it
/// redacts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ClipContent<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub colour: Colour,
}

/// One entry of the tray ranking: an id and a label, and nothing else.
///
/// **There is no `value` field, and [`ranked_for_tray`] does not select the
/// `value` column.** Spec §4.5 puts labels in the tray menu; a clip `value` has
/// no route into a native menu string, structurally rather than by every caller
/// remembering not to put one there.
#[derive(Clone, PartialEq, Eq)]
pub struct TrayClip {
    pub id: Uuid,
    pub label: String,
}

/// Hand-written for the same reason [`ClipRow`]'s is: a `{:?}` anywhere must not
/// print what the user stored (ADR-0002).
impl fmt::Debug for TrayClip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrayClip")
            .field("id", &self.id)
            .field("label", &Redacted::of(&self.label))
            .finish()
    }
}

/// The tray ranking: at most `limit` clips, descending `use_count`, with the
/// user's list order breaking ties and filling the remaining slots.
///
/// **One `ORDER BY` expresses the whole of spec §4.5**, and it is one query
/// rather than two passes because "the ten most used" and "ten clips ranked by
/// use, ties broken by list order" are different rules and only the second one
/// is specified. A fresh install has every `use_count` at 0, so every clip ties
/// and `position ASC` alone decides — which is exactly the specification's
/// "the user's list order fills the remaining slots", and why the menu is
/// populated before any clip has been used once.
///
/// An eleventh clip is therefore absent until its count strictly exceeds that of
/// something above it, or ties with it and sits earlier in the list.
pub fn ranked_for_tray(connection: &Connection, limit: usize) -> Result<Vec<TrayClip>, ClipError> {
    // `usize` cannot exceed `i64` on any target this ships to, and saturating
    // rather than unwrapping keeps the failure — if one ever existed — a larger
    // menu instead of a panic on the tray path.
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);

    let mut statement = connection
        .prepare("SELECT id, label FROM clips ORDER BY use_count DESC, position ASC LIMIT ?1")
        .map_err(|e| storage_error("the tray ranking could not be prepared", &e))?;

    let rows = statement
        .query_map([limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| storage_error("the tray ranking could not be read", &e))?;

    let mut ranked = Vec::new();
    for row in rows {
        let (id, label) =
            row.map_err(|e| storage_error("a tray ranking row could not be read", &e))?;
        ranked.push(TrayClip {
            id: parse_id(&id)?,
            label,
        });
    }
    Ok(ranked)
}

/// The stored form of an id: lowercase, hyphenated, 36 characters.
fn id_text(id: Uuid) -> String {
    id.as_hyphenated().to_string()
}

fn parse_id(text: &str) -> Result<Uuid, ClipError> {
    match Uuid::parse_str(text) {
        Ok(id) => Ok(id),
        Err(_) => {
            // The text itself is not logged: a corrupt id column could hold a
            // fragment of anything.
            log::error!("a stored clip has an id that is not a UUID");
            Err(ClipError::Storage)
        }
    }
}

/// Every clip in display order.
pub fn list(connection: &Connection) -> Result<Vec<ClipRow>, ClipError> {
    let sql = format!("SELECT {COLUMNS} FROM clips ORDER BY position ASC");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|e| storage_error("the clip list could not be prepared", &e))?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| storage_error("the clip list could not be read", &e))?;

    let mut clips = Vec::new();
    for row in rows {
        let (id, label, value, colour, use_count, position) =
            row.map_err(|e| storage_error("a clip row could not be read", &e))?;
        clips.push(ClipRow {
            id: parse_id(&id)?,
            label,
            value,
            colour: Colour::from_stored(&colour)?,
            use_count,
            position,
        });
    }
    Ok(clips)
}

/// Every clip id in display order. The renumber input.
pub fn ids_in_order(connection: &Connection) -> Result<Vec<Uuid>, ClipError> {
    let mut statement = connection
        .prepare("SELECT id FROM clips ORDER BY position ASC")
        .map_err(|e| storage_error("the clip ids could not be prepared", &e))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| storage_error("the clip ids could not be read", &e))?;

    let mut ids = Vec::new();
    for row in rows {
        let text = row.map_err(|e| storage_error("a clip id could not be read", &e))?;
        ids.push(parse_id(&text)?);
    }
    Ok(ids)
}

/// How many clips are stored.
pub fn count(connection: &Connection) -> Result<i64, ClipError> {
    connection
        .query_row("SELECT count(*) FROM clips", [], |row| row.get(0))
        .map_err(|e| storage_error("the clip count could not be read", &e))
}

/// Append a clip and return the id the backend minted for it.
///
/// The position is the current maximum plus one, computed inside the same
/// statement so that no interleaving can hand two clips the same position.
pub fn insert(
    connection: &Connection,
    label: &str,
    value: &str,
    colour: Colour,
) -> Result<Uuid, ClipError> {
    let id = Uuid::new_v4();
    connection
        .execute(
            "INSERT INTO clips (id, label, value, colour, use_count, position)
             VALUES (?1, ?2, ?3, ?4, 0, (SELECT COALESCE(MAX(position), -1) + 1 FROM clips))",
            (id_text(id), label, value, colour),
        )
        .map_err(|e| storage_error("a clip could not be inserted", &e))?;
    Ok(id)
}

/// Append a validated import in one transaction (WP-09).
///
/// **Phase two of the import** (contract, `import_clips`). Phase one validated
/// the whole file in memory and is over; this is the write, and it is one
/// `BEGIN IMMEDIATE` transaction so that a failure at any clip leaves the store
/// exactly as it was. `BEGIN IMMEDIATE` rather than the default deferred
/// transaction, so the write lock is taken at the start and a concurrent writer
/// cannot make a lock upgrade fail halfway through.
///
/// The atomicity is SQLite's. A staging table or a backup-and-restore around
/// the merge would reimplement it worse (ADR-0005, closed question 10).
///
/// **Merge only.** Every clip is appended with a freshly minted id and
/// `use_count` 0. Nothing is deleted, nothing is overwritten, and there is no
/// deduplication — importing the same file twice produces two copies of every
/// clip, which follows from "import never overwrites" and is asserted below so
/// it is not mistaken for a defect.
///
/// Positions continue from the current maximum, so the user's existing order is
/// untouched and no renumber is needed: the new rows occupy positions nothing
/// else holds (`storage.md` § Position is dense).
pub fn import(
    connection: &mut Connection,
    imported: &[ClipContent<'_>],
) -> Result<usize, ClipError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| storage_error("an import transaction could not be opened", &e))?;

    let base: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM clips",
            [],
            |row| row.get(0),
        )
        .map_err(|e| storage_error("the append position could not be read", &e))?;

    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO clips (id, label, value, colour, use_count, position)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5)",
            )
            .map_err(|e| storage_error("the import statement could not be prepared", &e))?;

        for (offset, clip) in imported.iter().enumerate() {
            let position = match i64::try_from(offset) {
                Ok(offset) => base.saturating_add(offset),
                Err(_) => {
                    log::error!("an import carried more clips than a position can count");
                    return Err(ClipError::Storage);
                }
            };
            statement
                .execute((
                    id_text(Uuid::new_v4()),
                    clip.label,
                    clip.value,
                    clip.colour,
                    position,
                ))
                // Dropping the transaction rolls back every clip inserted so
                // far. A half-applied import is worse than one that fails.
                .map_err(|e| storage_error("a clip could not be imported", &e))?;
        }
    }

    transaction
        .commit()
        .map_err(|e| storage_error("an import could not be committed", &e))?;

    Ok(imported.len())
}

/// Rewrite a clip's content. `id`, `position` and `use_count` are unchanged.
///
/// An unknown id is `not_found`; it never creates a clip.
pub fn update(
    connection: &Connection,
    id: Uuid,
    label: &str,
    value: &str,
    colour: Colour,
) -> Result<(), ClipError> {
    let changed = connection
        .execute(
            "UPDATE clips SET label = ?2, value = ?3, colour = ?4 WHERE id = ?1",
            (id_text(id), label, value, colour),
        )
        .map_err(|e| storage_error("a clip could not be updated", &e))?;

    if changed == 0 {
        return Err(ClipError::NotFound {
            clip_id: id_text(id),
        });
    }
    Ok(())
}

/// One clip's `value`, for the clipboard.
///
/// Step 1 of the copy sequence (contract `copy_clip`): an unknown id is
/// `not_found` and nothing else happens — in particular the clipboard is not
/// written and no count moves.
///
/// It returns the `value` alone rather than the row, so that the copy path never
/// holds a `label` it has no use for.
pub fn value_of(connection: &Connection, id: Uuid) -> Result<String, ClipError> {
    let found = connection.query_row(
        "SELECT value FROM clips WHERE id = ?1",
        [id_text(id)],
        |row| row.get::<_, String>(0),
    );
    match found {
        Ok(value) => Ok(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(ClipError::NotFound {
            clip_id: id_text(id),
        }),
        Err(e) => Err(storage_error("a clip value could not be read", &e)),
    }
}

/// Increment one clip's `use_count` by exactly one, committed before returning.
pub fn increment_use_count(connection: &Connection, id: Uuid) -> Result<(), ClipError> {
    let changed = connection
        .execute(
            "UPDATE clips SET use_count = use_count + 1 WHERE id = ?1",
            [id_text(id)],
        )
        .map_err(|e| storage_error("a use_count could not be incremented", &e))?;

    if changed == 0 {
        return Err(ClipError::NotFound {
            clip_id: id_text(id),
        });
    }
    Ok(())
}

/// Delete a clip and renumber the remainder, in one transaction.
///
/// A crash at any instant leaves either the clip present with a dense order, or
/// the clip gone with a dense order. Never a gap, and never a duplicate.
pub fn delete(connection: &mut Connection, id: Uuid) -> Result<(), ClipError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| storage_error("a delete transaction could not be opened", &e))?;

    let removed = transaction
        .execute("DELETE FROM clips WHERE id = ?1", [id_text(id)])
        .map_err(|e| storage_error("a clip could not be deleted", &e))?;

    if removed == 0 {
        // Dropping the transaction rolls it back.
        return Err(ClipError::NotFound {
            clip_id: id_text(id),
        });
    }

    let remaining = ids_in_order(&transaction)?;
    renumber(&transaction, &remaining)?;

    transaction
        .commit()
        .map_err(|e| storage_error("a delete could not be committed", &e))
}

/// `invalid_input { field: "order", reason: "not_a_permutation" }`.
///
/// One constructor, so the rejection cannot acquire a second spelling. It names
/// the argument and never the ids it was given — an id is not a secret, but the
/// frontend's recovery is to resynchronise, and there is nothing in a list of
/// ids it can act on that `update_clips` does not already carry.
fn not_a_permutation() -> ClipError {
    ClipError::InvalidInput {
        field: "order".to_owned(),
        reason: InvalidReason::NotAPermutation,
    }
}

/// Whether `order` is an exact permutation of `stored`: same length, same
/// members, no duplicates (ADR-0007).
///
/// `stored` comes from the `id` column, which is the primary key, so it holds no
/// duplicates of its own. Equal lengths plus "every element of `order` is in
/// `stored`, and distinct" is therefore exactly the permutation property.
fn is_permutation_of(stored: &[Uuid], order: &[Uuid]) -> bool {
    if stored.len() != order.len() {
        return false;
    }
    let known: HashSet<Uuid> = stored.iter().copied().collect();
    let mut seen: HashSet<Uuid> = HashSet::with_capacity(order.len());
    order
        .iter()
        .all(|id| known.contains(id) && seen.insert(*id))
}

/// Apply a complete permutation of the stored id set, in one transaction.
///
/// **The argument is validated against the store inside the transaction**, so
/// the set it is checked against is the set that is then rewritten. An `order`
/// that omits a clip, adds one, or repeats one is rejected whole with
/// [`not_a_permutation`] and nothing is written — never applied to the part of
/// the list that did match (contract, `reorder_clips`; ADR-0007).
///
/// The rejection is `invalid_input` and never `not_found`, even when the only
/// fault is one unknown id: the whole argument is wrong, not one element of it.
///
/// A crash at any instant leaves the old order or the new one, dense either way.
/// The transaction is what gives that; the offset-then-write pair inside
/// [`renumber`] is what keeps `UNIQUE(position)` from failing on the way.
pub fn reorder(connection: &mut Connection, order: &[Uuid]) -> Result<(), ClipError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| storage_error("a reorder transaction could not be opened", &e))?;

    let stored = ids_in_order(&transaction)?;
    if !is_permutation_of(&stored, order) {
        // Dropping the transaction rolls it back. Nothing was written in any
        // case: the check precedes the first `UPDATE`.
        return Err(not_a_permutation());
    }

    renumber(&transaction, order)?;

    transaction
        .commit()
        .map_err(|e| storage_error("a reorder could not be committed", &e))
}

/// Write `ordered` into `position` `0..N-1`, as an offset-then-write pair.
///
/// **The obvious statement is wrong.** `UPDATE clips SET position = position - 1
/// WHERE position > ?` is correct only if SQLite happens to visit the rows in
/// ascending `position` order. Visited descending — which a full table scan
/// does after any reorder, because rowid order then bears no relation to
/// position order — the first decremented row collides with the row below it
/// and `UNIQUE` fails on the spot. SQLite has no deferred unique constraint
/// outside foreign keys, and visitation order is a property of the query plan.
///
/// So every row is first offset clear of the whole occupied range, and only then
/// given its final value. Both ranges are disjoint from the ranges they are
/// leaving, so no intermediate state can duplicate a position **whatever order
/// the rows are visited in**.
///
/// Caller's obligation: `ordered` is every stored id, exactly once.
pub fn renumber(transaction: &Transaction<'_>, ordered: &[Uuid]) -> Result<(), ClipError> {
    if ordered.is_empty() {
        return Ok(());
    }

    let stored = count(transaction)?;
    if stored != ordered.len() as i64 {
        // Not reachable from a validated permutation. It is checked because the
        // alternative is renumbering a subset and leaving the order sparse.
        log::error!("renumber was given {} ids for {stored} rows", ordered.len());
        return Err(ClipError::Internal);
    }

    let max: i64 = transaction
        .query_row("SELECT COALESCE(MAX(position), -1) FROM clips", [], |row| {
            row.get(0)
        })
        .map_err(|e| storage_error("the maximum position could not be read", &e))?;

    // Step one: shift every row past the highest occupied position. Source
    // range `0..=max`, target range `max+1..=2max+1` — disjoint.
    let offset = max + 1;
    transaction
        .execute("UPDATE clips SET position = position + ?1", [offset])
        .map_err(|e| storage_error("positions could not be offset", &e))?;

    // Step two: write the final values. Target range `0..len-1`, which is at
    // most `max` and so disjoint from every row's current position.
    let mut statement = transaction
        .prepare("UPDATE clips SET position = ?2 WHERE id = ?1")
        .map_err(|e| storage_error("the renumber statement could not be prepared", &e))?;
    for (position, id) in ordered.iter().enumerate() {
        let changed = statement
            .execute((id_text(*id), position as i64))
            .map_err(|e| storage_error("a position could not be written", &e))?;
        if changed == 0 {
            log::error!("renumber was given an id that is not in the store");
            return Err(ClipError::Internal);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{connection, schema};

    fn store() -> (tempfile::TempDir, Connection) {
        let dir = match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        };
        let connection = match connection::open(&dir.path().join("clips.db")) {
            Ok(connection) => connection,
            Err(e) => panic!("the database should open: {e}"),
        };
        if let Err(e) = schema::create(&connection) {
            panic!("the schema should be creatable: {e}");
        }
        (dir, connection)
    }

    fn add(connection: &Connection, label: &str) -> Uuid {
        match insert(connection, label, "a value", Colour::DEFAULT) {
            Ok(id) => id,
            Err(e) => panic!("the insert should succeed: {e}"),
        }
    }

    fn rows(connection: &Connection) -> Vec<ClipRow> {
        match list(connection) {
            Ok(rows) => rows,
            Err(e) => panic!("the list should succeed: {e}"),
        }
    }

    fn positions(connection: &Connection) -> Vec<i64> {
        rows(connection).iter().map(|c| c.position).collect()
    }

    #[test]
    fn an_empty_store_lists_nothing_and_is_not_an_error() {
        let (_dir, connection) = store();
        assert_eq!(rows(&connection).len(), 0);
    }

    #[test]
    fn inserts_append_and_carry_dense_positions() {
        let (_dir, connection) = store();
        add(&connection, "first");
        add(&connection, "second");
        add(&connection, "third");

        let listed = rows(&connection);
        let labels: Vec<&str> = listed.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["first", "second", "third"]);
        assert_eq!(positions(&connection), vec![0, 1, 2]);
        assert!(listed.iter().all(|c| c.use_count == 0));
    }

    #[test]
    fn an_update_changes_content_and_leaves_position_and_use_count_alone() {
        let (_dir, connection) = store();
        add(&connection, "first");
        let id = add(&connection, "second");
        if let Err(e) = increment_use_count(&connection, id) {
            panic!("the increment should succeed: {e}");
        }

        if let Err(e) = update(&connection, id, "renamed", "new value", Colour::Violet) {
            panic!("the update should succeed: {e}");
        }

        let listed = rows(&connection);
        assert_eq!(listed[1].label, "renamed");
        assert_eq!(listed[1].value, "new value");
        assert_eq!(listed[1].colour, Colour::Violet);
        assert_eq!(listed[1].position, 1);
        assert_eq!(listed[1].use_count, 1);
    }

    #[test]
    fn an_update_on_an_unknown_id_is_not_found_and_creates_nothing() {
        let (_dir, connection) = store();
        let missing = Uuid::new_v4();
        assert_eq!(
            update(&connection, missing, "l", "v", Colour::Teal),
            Err(ClipError::NotFound {
                clip_id: missing.as_hyphenated().to_string()
            })
        );
        assert_eq!(rows(&connection).len(), 0);
    }

    #[test]
    fn a_delete_on_an_unknown_id_is_not_found_and_changes_nothing() {
        let (_dir, mut connection) = store();
        add(&connection, "first");
        let missing = Uuid::new_v4();
        assert_eq!(
            delete(&mut connection, missing),
            Err(ClipError::NotFound {
                clip_id: missing.as_hyphenated().to_string()
            })
        );
        assert_eq!(positions(&connection), vec![0]);
    }

    #[test]
    fn a_delete_renumbers_the_remainder_densely() {
        let (_dir, mut connection) = store();
        let first = add(&connection, "first");
        add(&connection, "second");
        add(&connection, "third");

        if let Err(e) = delete(&mut connection, first) {
            panic!("the delete should succeed: {e}");
        }

        let listed = rows(&connection);
        let labels: Vec<&str> = listed.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["second", "third"]);
        assert_eq!(positions(&connection), vec![0, 1]);
    }

    #[test]
    fn a_delete_renumbers_when_position_order_is_the_reverse_of_row_order() {
        // The case the naive `position = position - 1` statement fails on: after
        // a reorder the rows are visited in an order that has nothing to do with
        // their positions, and a decrement then collides with the row below.
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..6)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let reversed: Vec<Uuid> = ids.iter().rev().copied().collect();
        {
            let transaction =
                match connection.transaction_with_behavior(TransactionBehavior::Immediate) {
                    Ok(transaction) => transaction,
                    Err(e) => panic!("the transaction should open: {e}"),
                };
            if let Err(e) = renumber(&transaction, &reversed) {
                panic!("the renumber should succeed: {e}");
            }
            if let Err(e) = transaction.commit() {
                panic!("the renumber should commit: {e}");
            }
        }
        assert_eq!(
            ids_in_order(&connection),
            Ok(reversed.clone()),
            "the reversal should have been applied"
        );

        // Delete from the middle of the reversed order.
        if let Err(e) = delete(&mut connection, reversed[2]) {
            panic!("the delete should succeed: {e}");
        }

        assert_eq!(positions(&connection), vec![0, 1, 2, 3, 4]);
        let expected: Vec<Uuid> = reversed
            .iter()
            .filter(|id| **id != reversed[2])
            .copied()
            .collect();
        assert_eq!(ids_in_order(&connection), Ok(expected));
    }

    #[test]
    fn deleting_every_clip_one_at_a_time_never_breaks_the_dense_invariant() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..8)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        for (deleted, id) in ids.iter().enumerate() {
            if let Err(e) = delete(&mut connection, *id) {
                panic!("the delete should succeed: {e}");
            }
            let remaining = ids.len() - deleted - 1;
            let expected: Vec<i64> = (0..remaining as i64).collect();
            assert_eq!(positions(&connection), expected);
        }
        assert_eq!(rows(&connection).len(), 0);
    }

    #[test]
    fn an_insert_after_a_delete_appends_rather_than_reusing_a_position() {
        let (_dir, mut connection) = store();
        let first = add(&connection, "first");
        add(&connection, "second");
        if let Err(e) = delete(&mut connection, first) {
            panic!("the delete should succeed: {e}");
        }
        add(&connection, "third");

        let listed = rows(&connection);
        let labels: Vec<&str> = listed.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["second", "third"]);
        assert_eq!(positions(&connection), vec![0, 1]);
    }

    #[test]
    fn use_count_increments_by_exactly_one() {
        let (_dir, connection) = store();
        let id = add(&connection, "first");
        for expected in 1..=3 {
            if let Err(e) = increment_use_count(&connection, id) {
                panic!("the increment should succeed: {e}");
            }
            assert_eq!(rows(&connection)[0].use_count, expected);
        }
    }

    #[test]
    fn a_value_is_read_back_exactly_as_it_was_stored() {
        let (_dir, connection) = store();
        let id = match insert(&connection, "first", "line one\nline two", Colour::DEFAULT) {
            Ok(id) => id,
            Err(e) => panic!("the insert should succeed: {e}"),
        };
        assert_eq!(value_of(&connection, id), Ok("line one\nline two".into()));
    }

    #[test]
    fn the_value_of_an_unknown_clip_is_not_found() {
        let (_dir, connection) = store();
        let missing = Uuid::new_v4();
        assert_eq!(
            value_of(&connection, missing),
            Err(ClipError::NotFound {
                clip_id: missing.as_hyphenated().to_string()
            })
        );
    }

    #[test]
    fn incrementing_an_unknown_clip_is_not_found() {
        let (_dir, connection) = store();
        let missing = Uuid::new_v4();
        assert_eq!(
            increment_use_count(&connection, missing),
            Err(ClipError::NotFound {
                clip_id: missing.as_hyphenated().to_string()
            })
        );
    }

    #[test]
    fn ids_are_stored_lowercase_and_hyphenated() {
        let (_dir, connection) = store();
        let id = add(&connection, "first");
        let stored: String =
            match connection.query_row("SELECT id FROM clips", [], |row| row.get(0)) {
                Ok(stored) => stored,
                Err(e) => panic!("the id should be readable: {e}"),
            };
        assert_eq!(stored.len(), 36);
        assert_eq!(stored, stored.to_lowercase());
        assert_eq!(parse_id(&stored), Ok(id));
    }

    #[test]
    fn the_debug_output_never_carries_a_label_or_a_value() {
        let row = ClipRow {
            id: Uuid::nil(),
            label: "Support greeting".into(),
            value: "hunter2".into(),
            colour: Colour::Amber,
            use_count: 4,
            position: 2,
        };
        let rendered = format!("{row:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(!rendered.contains("Support greeting"), "{rendered}");
        assert!(rendered.contains("<redacted, 7 chars>"), "{rendered}");
        assert!(rendered.contains("use_count: 4"), "{rendered}");
    }

    /// The column is `TEXT` holding the token's name, never hex and never an
    /// ordinal. A palette retune must not require rewriting a single row
    /// (`palette.md` requirement 5).
    #[test]
    fn a_colour_is_stored_as_its_token_name_in_text() {
        let (_dir, connection) = store();
        if let Err(e) = insert(&connection, "first", "a value", Colour::Pink) {
            panic!("the insert should succeed: {e}");
        }

        let stored: String =
            match connection.query_row("SELECT colour FROM clips", [], |row| row.get(0)) {
                Ok(stored) => stored,
                Err(e) => panic!("the colour should be readable: {e}"),
            };
        assert_eq!(stored, "pink");
        assert_eq!(rows(&connection)[0].colour, Colour::Pink);
    }

    /// A development store written by a WP-04..WP-09 build holds
    /// `colour = 'unset'` in every row. WP-10 deletes that token, so the rows
    /// become unreadable. The failure must be the `storage` variant
    /// `list_clips` declares, not a panic and not a silent substitution — the
    /// remedy is in the log line, and it is to delete `~/.fast-clip/`.
    #[test]
    fn a_stored_colour_this_build_cannot_interpret_is_storage_rather_than_a_panic() {
        let (_dir, connection) = store();
        if let Err(e) = connection.execute(
            "INSERT INTO clips (id, label, value, colour, use_count, position)
             VALUES (?1, 'a label', 'a value', 'unset', 0, 0)",
            (Uuid::new_v4().as_hyphenated().to_string(),),
        ) {
            panic!("the fixture row should insert: {e}");
        }

        assert_eq!(list(&connection), Err(ClipError::Storage));
        // Nothing was repaired, deleted or defaulted on the way past.
        assert_eq!(count(&connection), Ok(1));
    }

    // ---- reorder (WP-06) ----

    fn rejected() -> ClipError {
        ClipError::InvalidInput {
            field: "order".into(),
            reason: InvalidReason::NotAPermutation,
        }
    }

    #[test]
    fn a_reorder_applies_the_permutation_and_keeps_the_order_dense() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..5)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let moved = vec![ids[3], ids[0], ids[4], ids[1], ids[2]];
        assert_eq!(reorder(&mut connection, &moved), Ok(()));

        assert_eq!(ids_in_order(&connection), Ok(moved));
        assert_eq!(positions(&connection), vec![0, 1, 2, 3, 4]);
    }

    /// The case `UNIQUE(position)` makes hostile: every row moves, and a naive
    /// in-place write collides on the first statement.
    #[test]
    fn a_full_reversal_is_applied_without_a_unique_violation() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..8)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let reversed: Vec<Uuid> = ids.iter().rev().copied().collect();
        assert_eq!(reorder(&mut connection, &reversed), Ok(()));
        assert_eq!(ids_in_order(&connection), Ok(reversed));
        assert_eq!(positions(&connection), (0..8).collect::<Vec<i64>>());
    }

    /// Reordering twice in a row is the ordinary case — a second drag lands on
    /// rows whose rowid order no longer resembles their position order.
    #[test]
    fn reordering_repeatedly_never_breaks_the_dense_invariant() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..6)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let mut current = ids.clone();
        for _ in 0..6 {
            // Rotate by one: every row moves on every pass.
            current.rotate_left(1);
            assert_eq!(reorder(&mut connection, &current), Ok(()));
            assert_eq!(ids_in_order(&connection), Ok(current.clone()));
            assert_eq!(positions(&connection), (0..6).collect::<Vec<i64>>());
        }
        // Six rotations of six ids returns the original order.
        assert_eq!(ids_in_order(&connection), Ok(ids));
    }

    #[test]
    fn an_order_that_omits_a_clip_is_rejected_whole() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..3)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        assert_eq!(reorder(&mut connection, &ids[..2]), Err(rejected()));
        // Not one row moved. A partial application is the failure this command
        // exists to make impossible.
        assert_eq!(ids_in_order(&connection), Ok(ids));
        assert_eq!(positions(&connection), vec![0, 1, 2]);
    }

    /// A clip created between the frontend's last list and the drop. The stale
    /// order is refused rather than applied to the subset it does cover.
    #[test]
    fn an_order_that_names_an_unknown_clip_is_not_a_permutation_rather_than_not_found() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..3)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let stale = vec![ids[2], ids[1], Uuid::new_v4()];
        assert_eq!(reorder(&mut connection, &stale), Err(rejected()));
        assert_eq!(ids_in_order(&connection), Ok(ids));
        assert_eq!(positions(&connection), vec![0, 1, 2]);
    }

    #[test]
    fn an_order_that_repeats_a_clip_is_rejected_even_at_the_right_length() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..3)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        // Right length, right membership for two of three, one id twice. Without
        // the duplicate check this would renumber two rows and leave the third
        // holding an offset position outside `0..N-1`.
        let duplicated = vec![ids[0], ids[1], ids[1]];
        assert_eq!(reorder(&mut connection, &duplicated), Err(rejected()));
        assert_eq!(ids_in_order(&connection), Ok(ids));
        assert_eq!(positions(&connection), vec![0, 1, 2]);
    }

    #[test]
    fn an_order_longer_than_the_store_is_rejected() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..2)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let mut too_long = ids.clone();
        too_long.push(Uuid::new_v4());
        assert_eq!(reorder(&mut connection, &too_long), Err(rejected()));
        assert_eq!(ids_in_order(&connection), Ok(ids));
    }

    /// An empty store and an empty order agree, so the permutation is exact and
    /// there is nothing to write. Not reachable from the frontend, which never
    /// drags in an empty list, but it must not be an error and must not renumber.
    #[test]
    fn an_empty_order_against_an_empty_store_succeeds_and_writes_nothing() {
        let (_dir, mut connection) = store();
        assert_eq!(reorder(&mut connection, &[]), Ok(()));
        assert_eq!(count(&connection), Ok(0));
    }

    /// The mirror of the case above: an empty order against a store that holds
    /// clips is a length mismatch, not an instruction to clear the order.
    #[test]
    fn an_empty_order_against_a_populated_store_is_rejected() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..2)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        assert_eq!(reorder(&mut connection, &[]), Err(rejected()));
        assert_eq!(ids_in_order(&connection), Ok(ids));
        assert_eq!(positions(&connection), vec![0, 1]);
    }

    /// **The half-applied order, made to happen.** The renumber runs in full and
    /// the transaction is then abandoned without committing, which is what a
    /// process killed between the offset write and the commit leaves behind. The
    /// store must read back as the order it held before, dense — not the new
    /// order, and not the intermediate `+N` positions the offset step wrote.
    #[test]
    fn a_reorder_abandoned_before_its_commit_leaves_the_old_order_intact() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..5)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let reversed: Vec<Uuid> = ids.iter().rev().copied().collect();
        {
            let transaction =
                match connection.transaction_with_behavior(TransactionBehavior::Immediate) {
                    Ok(transaction) => transaction,
                    Err(e) => panic!("the transaction should open: {e}"),
                };
            if let Err(e) = renumber(&transaction, &reversed) {
                panic!("the renumber should succeed: {e}");
            }
            // Inside the transaction the new order is already visible.
            assert_eq!(ids_in_order(&transaction), Ok(reversed.clone()));
            // No commit. Dropping rolls it back.
        }

        assert_eq!(ids_in_order(&connection), Ok(ids));
        assert_eq!(positions(&connection), vec![0, 1, 2, 3, 4]);
    }

    /// A reorder and a delete compose: the order the user set survives the row
    /// removal, and the remainder is renumbered against it rather than against
    /// insertion order.
    #[test]
    fn a_delete_after_a_reorder_preserves_the_users_order_for_the_remainder() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..4)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let moved = vec![ids[2], ids[0], ids[3], ids[1]];
        if let Err(e) = reorder(&mut connection, &moved) {
            panic!("the reorder should succeed: {e}");
        }
        if let Err(e) = delete(&mut connection, ids[3]) {
            panic!("the delete should succeed: {e}");
        }

        assert_eq!(ids_in_order(&connection), Ok(vec![ids[2], ids[0], ids[1]]));
        assert_eq!(positions(&connection), vec![0, 1, 2]);
    }

    /// An insert after a reorder appends to the end of the user's order, because
    /// the position is the maximum plus one and the maximum is dense.
    #[test]
    fn an_insert_after_a_reorder_appends_to_the_end_of_the_users_order() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..3)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let reversed: Vec<Uuid> = ids.iter().rev().copied().collect();
        if let Err(e) = reorder(&mut connection, &reversed) {
            panic!("the reorder should succeed: {e}");
        }
        let appended = add(&connection, "newest");

        let mut expected = reversed;
        expected.push(appended);
        assert_eq!(ids_in_order(&connection), Ok(expected));
        assert_eq!(positions(&connection), vec![0, 1, 2, 3]);
    }

    /// The predicate on its own, including the shapes the store cannot produce.
    #[test]
    fn the_permutation_check_accepts_only_an_exact_rearrangement() {
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let other = Uuid::new_v4();

        assert!(is_permutation_of(&ids, &ids));
        let reversed: Vec<Uuid> = ids.iter().rev().copied().collect();
        assert!(is_permutation_of(&ids, &reversed));
        assert!(is_permutation_of(&[], &[]));

        assert!(!is_permutation_of(&ids, &ids[..3]));
        assert!(!is_permutation_of(&ids, &[ids[0], ids[1], ids[2], other]));
        assert!(!is_permutation_of(&ids, &[ids[0], ids[1], ids[2], ids[2]]));
        assert!(!is_permutation_of(&ids, &[]));
        assert!(!is_permutation_of(&[], &[other]));
    }

    #[test]
    fn renumber_refuses_a_partial_id_set_rather_than_leaving_a_sparse_order() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..3)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();

        let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate)
        {
            Ok(transaction) => transaction,
            Err(e) => panic!("the transaction should open: {e}"),
        };
        assert_eq!(renumber(&transaction, &ids[..2]), Err(ClipError::Internal));
        drop(transaction);

        assert_eq!(positions(&connection), vec![0, 1, 2]);
    }

    // ---- import (WP-09) ----

    fn content<'a>(label: &'a str, value: &'a str) -> ClipContent<'a> {
        ClipContent {
            label,
            value,
            colour: Colour::Teal,
        }
    }

    #[test]
    fn an_import_appends_after_the_existing_clips_and_keeps_the_order_dense() {
        let (_dir, mut connection) = store();
        add(&connection, "mine one");
        add(&connection, "mine two");

        let imported = [content("theirs one", "v1"), content("theirs two", "v2")];
        assert_eq!(import(&mut connection, &imported), Ok(2));

        let listed = rows(&connection);
        let labels: Vec<&str> = listed.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["mine one", "mine two", "theirs one", "theirs two"]
        );
        assert_eq!(positions(&connection), vec![0, 1, 2, 3]);
    }

    /// An import continues from the current maximum, so it needs no
    /// offset-then-write pair: the new rows occupy positions nothing else holds
    /// (`storage.md` § Position is dense). The case that would catch a mistake
    /// is an import after a reorder, when rowid order and position order have
    /// nothing to do with each other.
    #[test]
    fn an_import_after_a_reorder_appends_to_the_end_of_the_users_order() {
        let (_dir, mut connection) = store();
        let ids: Vec<Uuid> = (0..4)
            .map(|n| add(&connection, &format!("clip {n}")))
            .collect();
        let reversed: Vec<Uuid> = ids.iter().rev().copied().collect();
        if let Err(e) = reorder(&mut connection, &reversed) {
            panic!("the reorder should succeed: {e}");
        }

        assert_eq!(import(&mut connection, &[content("appended", "v")]), Ok(1));

        let mut expected: Vec<String> = reversed
            .iter()
            .enumerate()
            .map(|(n, _)| format!("clip {}", 3 - n))
            .collect();
        expected.push("appended".into());
        let labels: Vec<String> = rows(&connection).iter().map(|c| c.label.clone()).collect();
        assert_eq!(labels, expected);
        assert_eq!(positions(&connection), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn an_empty_import_succeeds_and_writes_nothing() {
        let (_dir, mut connection) = store();
        add(&connection, "mine");
        assert_eq!(import(&mut connection, &[]), Ok(0));
        assert_eq!(count(&connection), Ok(1));
        assert_eq!(positions(&connection), vec![0]);
    }

    /// Every imported clip is a distinct row with a fresh id and a `use_count`
    /// of zero, even when the file held the same content twice. Import never
    /// deduplicates (contract, `import_clips`).
    #[test]
    fn identical_clips_in_one_import_become_distinct_rows_at_a_zero_count() {
        let (_dir, mut connection) = store();
        let same = content("same", "same value");
        assert_eq!(import(&mut connection, &[same, same, same]), Ok(3));

        let listed = rows(&connection);
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().all(|row| row.use_count == 0));
        let ids: HashSet<Uuid> = listed.iter().map(|row| row.id).collect();
        assert_eq!(ids.len(), 3);
    }

    /// **The half-applied import, made to happen.** A clip whose insert fails
    /// mid-run must take the whole import with it: the row that collides is the
    /// last one, so every clip before it has already been written inside the
    /// transaction when the failure lands.
    #[test]
    fn an_import_that_fails_part_way_rolls_back_every_clip_before_it() {
        let (_dir, mut connection) = store();
        add(&connection, "mine");

        // A `CHECK (use_count >= 0)` cannot be tripped from here, so the fault
        // is injected where a real one would land: a unique `position` already
        // taken by a row this import is about to write over. The trigger fires
        // on the third insert.
        if let Err(e) = connection.execute_batch(
            "CREATE TRIGGER refuse_the_third BEFORE INSERT ON clips
             WHEN NEW.position = 3
             BEGIN SELECT RAISE(ABORT, 'refused'); END",
        ) {
            panic!("the fixture trigger should be creatable: {e}");
        }

        let imported = [
            content("one", "v"),
            content("two", "v"),
            content("three", "v"),
            content("four", "v"),
        ];
        assert_eq!(import(&mut connection, &imported), Err(ClipError::Storage));

        // Not one imported clip survived, and the user's own is untouched.
        let listed = rows(&connection);
        let labels: Vec<&str> = listed.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["mine"]);
        assert_eq!(positions(&connection), vec![0]);
    }

    // ---- the tray ranking (WP-08) ----

    fn ranked(connection: &Connection, limit: usize) -> Vec<String> {
        match ranked_for_tray(connection, limit) {
            Ok(ranked) => ranked.into_iter().map(|clip| clip.label).collect(),
            Err(e) => panic!("the tray ranking should succeed: {e}"),
        }
    }

    fn copy_it(connection: &Connection, id: Uuid, times: usize) {
        for _ in 0..times {
            if let Err(e) = increment_use_count(connection, id) {
                panic!("the increment should succeed: {e}");
            }
        }
    }

    #[test]
    fn an_empty_store_ranks_nothing_and_is_not_an_error() {
        let (_dir, connection) = store();
        assert_eq!(ranked(&connection, 10), Vec::<String>::new());
    }

    /// Spec §4.5's second clause: with every count at 0 the ranking *is* the
    /// user's list order, so a fresh install has a populated menu and a clip
    /// added today is reachable before it has been used once.
    #[test]
    fn an_unused_store_ranks_in_list_order() {
        let (_dir, connection) = store();
        add(&connection, "first");
        add(&connection, "second");
        add(&connection, "third");

        assert_eq!(ranked(&connection, 10), vec!["first", "second", "third"]);
    }

    #[test]
    fn a_higher_count_outranks_an_earlier_position() {
        let (_dir, connection) = store();
        add(&connection, "first");
        let second = add(&connection, "second");
        add(&connection, "third");

        copy_it(&connection, second, 1);
        assert_eq!(ranked(&connection, 10), vec!["second", "first", "third"]);
    }

    /// "Ties in `use_count` break by list order" — not by insertion order, not
    /// by id, and not by whichever row SQLite reached first.
    #[test]
    fn equal_counts_break_by_list_order() {
        let (_dir, mut connection) = store();
        let first = add(&connection, "first");
        let second = add(&connection, "second");
        let third = add(&connection, "third");

        copy_it(&connection, third, 2);
        copy_it(&connection, first, 2);
        copy_it(&connection, second, 2);

        assert_eq!(ranked(&connection, 10), vec!["first", "second", "third"]);

        // And the tie follows the list when the list moves.
        if let Err(e) = reorder(&mut connection, &[third, second, first]) {
            panic!("the reorder should succeed: {e}");
        }
        assert_eq!(ranked(&connection, 10), vec!["third", "second", "first"]);
    }

    /// At most ten, and the eleventh is absent until its count overtakes
    /// something above it.
    #[test]
    fn the_eleventh_clip_is_absent_until_its_count_overtakes_another() {
        let (_dir, connection) = store();
        let mut ids = Vec::new();
        for n in 0..12 {
            ids.push(add(&connection, &format!("clip {n}")));
        }

        let listed = ranked(&connection, 10);
        assert_eq!(listed.len(), 10);
        assert_eq!(listed.first().map(String::as_str), Some("clip 0"));
        assert_eq!(listed.last().map(String::as_str), Some("clip 9"));
        assert!(!listed.iter().any(|label| label == "clip 10"));
        assert!(!listed.iter().any(|label| label == "clip 11"));

        // One copy of the eleventh puts it at the top and pushes the last
        // unused clip out.
        copy_it(&connection, ids[10], 1);
        let listed = ranked(&connection, 10);
        assert_eq!(listed.first().map(String::as_str), Some("clip 10"));
        assert_eq!(listed.len(), 10);
        assert!(!listed.iter().any(|label| label == "clip 9"));
    }

    #[test]
    fn fewer_clips_than_the_limit_returns_all_of_them() {
        let (_dir, connection) = store();
        add(&connection, "only");
        assert_eq!(ranked(&connection, 10), vec!["only"]);
    }

    /// The tray shows labels. The query does not select `value` at all, so this
    /// asserts the shape of the type rather than a filter someone could relax.
    #[test]
    fn the_ranking_carries_no_clip_value_and_its_debug_redacts_the_label() {
        let (_dir, connection) = store();
        add(&connection, "a secret label");

        let ranked = match ranked_for_tray(&connection, 10) {
            Ok(ranked) => ranked,
            Err(e) => panic!("the tray ranking should succeed: {e}"),
        };
        let rendered = format!("{ranked:?}");
        assert!(!rendered.contains("a secret label"), "{rendered}");
        assert!(!rendered.contains("a value"), "{rendered}");
    }
}
