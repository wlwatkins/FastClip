//! Row-level operations on the `clips` table.
//!
//! This is the store's type, not the wire's. It carries `use_count`, which
//! never crosses the IPC seam (ADR-0008), and it is deliberately **not**
//! `Serialize`: the row type cannot become a payload by accident.
//!
//! Validation of `label`, `value` and `colour` belongs at the IPC boundary and
//! is not repeated here — one rule, one owner (storage.md § Schema).

use std::fmt;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::error::ClipError;
use crate::storage::storage_error;

const COLUMNS: &str = "id, label, value, colour, use_count, position";

/// One row of the `clips` table.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipRow {
    pub id: Uuid,
    pub label: String,
    pub value: String,
    pub colour: String,
    /// Backend-owned. Ranks the tray menu and has no other consumer.
    pub use_count: i64,
    /// Dense, `0..N-1`, no gaps (ADR-0007).
    pub position: i64,
}

/// A length, standing in for text that must never be formatted into a log line.
struct Redacted(usize);

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted, {} chars>", self.0)
    }
}

/// Hand-written so that no `{:?}` anywhere — a log line, a panic message, an
/// assertion — can print what the user stored. ADR-0002 forbids a clip `value`
/// reaching a log at any level; the `label` is redacted on the same reasoning.
impl fmt::Debug for ClipRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipRow")
            .field("id", &self.id)
            .field("label", &Redacted(self.label.chars().count()))
            .field("value", &Redacted(self.value.chars().count()))
            .field("colour", &self.colour)
            .field("use_count", &self.use_count)
            .field("position", &self.position)
            .finish()
    }
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
            colour,
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
    colour: &str,
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

/// Rewrite a clip's content. `id`, `position` and `use_count` are unchanged.
///
/// An unknown id is `not_found`; it never creates a clip.
pub fn update(
    connection: &Connection,
    id: Uuid,
    label: &str,
    value: &str,
    colour: &str,
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
        match insert(connection, label, "a value", "unset") {
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

        if let Err(e) = update(&connection, id, "renamed", "new value", "other") {
            panic!("the update should succeed: {e}");
        }

        let listed = rows(&connection);
        assert_eq!(listed[1].label, "renamed");
        assert_eq!(listed[1].value, "new value");
        assert_eq!(listed[1].colour, "other");
        assert_eq!(listed[1].position, 1);
        assert_eq!(listed[1].use_count, 1);
    }

    #[test]
    fn an_update_on_an_unknown_id_is_not_found_and_creates_nothing() {
        let (_dir, connection) = store();
        let missing = Uuid::new_v4();
        assert_eq!(
            update(&connection, missing, "l", "v", "c"),
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
            colour: "unset".into(),
            use_count: 4,
            position: 2,
        };
        let rendered = format!("{row:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(!rendered.contains("Support greeting"), "{rendered}");
        assert!(rendered.contains("<redacted, 7 chars>"), "{rendered}");
        assert!(rendered.contains("use_count: 4"), "{rendered}");
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
}
