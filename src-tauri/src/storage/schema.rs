//! The schema, its version, and the migration chain.
//!
//! The version lives in `PRAGMA user_version`, not in a meta table: it is a
//! header field readable in one statement, before anything is known about the
//! schema (contract § Versions on disk).
//!
//! Versioning exists from the first release. There is nothing to migrate at
//! version 1; the mechanism is here so that there is one when there is.

use std::cmp::Ordering;

use rusqlite::Connection;

use crate::error::{ClipError, VersionComponent};
use crate::storage::{open_error, storage_error};

/// The schema version this build writes and understands.
pub const SCHEMA_VERSION: i64 = 1;

/// One table, exactly as ratified in `docs/src/architecture/storage.md`.
///
/// `position` is `UNIQUE`, which makes a duplicate position impossible on disk
/// rather than merely unlikely — and which is why every renumber is an
/// offset-then-write pair (see [`crate::storage::clips::renumber`]).
///
/// `STRICT` needs SQLite 3.37 or newer; `connection.rs` asserts the bundled
/// version in its tests rather than dropping the keyword silently.
pub const CREATE_SCHEMA_SQL: &str = "\
CREATE TABLE clips (
    id        TEXT    NOT NULL PRIMARY KEY,
    label     TEXT    NOT NULL,
    value     TEXT    NOT NULL,
    colour    TEXT    NOT NULL,
    use_count INTEGER NOT NULL DEFAULT 0 CHECK (use_count >= 0),
    position  INTEGER NOT NULL UNIQUE CHECK (position >= 0)
) STRICT;

CREATE INDEX clips_ranking ON clips (use_count DESC, position ASC);
";

/// Create the schema and stamp the version, in one transaction.
///
/// `PRAGMA user_version` is written explicitly and inside the transaction, so a
/// kill during creation leaves either no schema or a complete stamped one.
pub fn create(connection: &Connection) -> Result<(), ClipError> {
    let sql =
        format!("BEGIN;\n{CREATE_SCHEMA_SQL}\nPRAGMA user_version = {SCHEMA_VERSION};\nCOMMIT;");
    connection
        .execute_batch(&sql)
        .map_err(|e| storage_error("the schema could not be created", &e))
}

/// Read `PRAGMA user_version`.
pub fn read_user_version(connection: &Connection) -> Result<i64, ClipError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| open_error("user_version could not be read", &e))
}

/// Check the version and apply any migrations up to [`SCHEMA_VERSION`].
///
/// A version **higher** than this build's is rejected, never misread. A version
/// **lower** is migrated and then held, never rejected.
///
/// There is no migration content at version 1 — 0 is the only lower value and
/// nothing is written down about what a version-0 store contains, so nothing is
/// invented here. Such a database is held as it is, and a query against a
/// missing table then reports `storage` rather than damaging anything.
pub fn check_and_migrate(connection: &Connection) -> Result<(), ClipError> {
    let found = read_user_version(connection)?;
    match found.cmp(&SCHEMA_VERSION) {
        Ordering::Equal => Ok(()),
        Ordering::Greater => {
            log::error!(
                "the store is at schema version {found}; this build understands {SCHEMA_VERSION}"
            );
            Err(ClipError::UnsupportedVersion {
                component: VersionComponent::Schema,
                found,
                supported: SCHEMA_VERSION,
            })
        }
        Ordering::Less => {
            log::warn!(
                "the store is at schema version {found}; no migration to {SCHEMA_VERSION} is defined"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ClipError;
    use crate::storage::connection;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn fresh(dir: &tempfile::TempDir) -> Connection {
        let connection = match connection::open(&dir.path().join("clips.db")) {
            Ok(connection) => connection,
            Err(e) => panic!("the database should open: {e}"),
        };
        if let Err(e) = create(&connection) {
            panic!("the schema should be creatable: {e}");
        }
        connection
    }

    #[test]
    fn a_created_database_is_stamped_with_the_schema_version() {
        let dir = dir();
        let connection = fresh(&dir);
        assert_eq!(read_user_version(&connection), Ok(SCHEMA_VERSION));
    }

    #[test]
    fn the_version_survives_a_close_and_reopen() {
        let dir = dir();
        {
            let connection = fresh(&dir);
            connection::checkpoint_and_close(connection);
        }
        let reopened = match connection::open(&dir.path().join("clips.db")) {
            Ok(connection) => connection,
            Err(e) => panic!("the database should reopen: {e}"),
        };
        assert_eq!(read_user_version(&reopened), Ok(SCHEMA_VERSION));
    }

    #[test]
    fn a_newer_schema_version_is_rejected_and_never_misread() {
        let dir = dir();
        let connection = fresh(&dir);
        if let Err(e) = connection.pragma_update(None, "user_version", 2) {
            panic!("the fixture should be writable: {e}");
        }
        assert_eq!(
            check_and_migrate(&connection),
            Err(ClipError::UnsupportedVersion {
                component: VersionComponent::Schema,
                found: 2,
                supported: 1,
            })
        );
    }

    #[test]
    fn the_current_schema_version_passes() {
        let dir = dir();
        let connection = fresh(&dir);
        assert_eq!(check_and_migrate(&connection), Ok(()));
    }

    #[test]
    fn an_older_schema_version_is_held_rather_than_rejected() {
        let dir = dir();
        let connection = fresh(&dir);
        if let Err(e) = connection.pragma_update(None, "user_version", 0) {
            panic!("the fixture should be writable: {e}");
        }
        assert_eq!(check_and_migrate(&connection), Ok(()));
    }

    #[test]
    fn the_table_is_strict() {
        // STRICT rejects a value of the wrong type instead of coercing it. If
        // the keyword were dropped this insert would succeed and type discipline
        // would rest on the Rust layer alone.
        let dir = dir();
        let connection = fresh(&dir);
        let result = connection.execute(
            "INSERT INTO clips (id, label, value, colour, use_count, position)
             VALUES ('x', 'l', 'v', 'c', 'not an integer', 0)",
            [],
        );
        assert!(result.is_err(), "STRICT should refuse a TEXT use_count");
    }

    #[test]
    fn position_is_unique() {
        let dir = dir();
        let connection = fresh(&dir);
        let insert = "INSERT INTO clips (id, label, value, colour, use_count, position)
                      VALUES (?1, 'l', 'v', 'c', 0, 0)";
        if let Err(e) = connection.execute(insert, ["a"]) {
            panic!("the first insert should succeed: {e}");
        }
        assert!(
            connection.execute(insert, ["b"]).is_err(),
            "two clips must not share a position"
        );
    }
}
