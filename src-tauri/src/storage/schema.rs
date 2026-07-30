//! The schema, its version, and the migration chain.
//!
//! The version lives in `PRAGMA user_version`, not in a meta table: it is a
//! header field readable in one statement, before anything is known about the
//! schema (contract § Versions on disk).
//!
//! Versioning exists from the first release. There is nothing to migrate at
//! version 1; the mechanism is here so that there is one when there is.

use rusqlite::Connection;

use crate::error::{ClipError, CryptoReason, VersionComponent};
use crate::storage::{open_error, storage_error};

/// The schema version this build writes and understands.
pub const SCHEMA_VERSION: i64 = 1;

/// The **lowest** version this build knows how to read.
///
/// Anything below it is not an old FastClip store, because no FastClip schema
/// was ever numbered below it and there is nothing to migrate from
/// (`storage.md` § Version 0, and why creation is transactional).
///
/// **Write the rule against this constant rather than against zero.** Today the
/// two coincide — the only value below 1 is 0, which is what `PRAGMA
/// user_version` reports for any SQLite database that never set it. They stop
/// coinciding the moment this build drops support for a version it once wrote,
/// and the rule is meant to survive that: the boundary is "below the lowest
/// version this build knows how to read", not "zero"
/// (`contract.md` § Versions on disk).
pub const MINIMUM_SCHEMA_VERSION: i64 = 1;

/// The floor has to be a real version, and it has to be one this build could
/// have written — otherwise a store created by [`create`] would be refused by
/// [`check_and_migrate`] on the very next launch. Checked at compile time, so
/// the mistake cannot reach a test run, let alone a user's database.
const _: () = assert!(
    MINIMUM_SCHEMA_VERSION >= 1 && MINIMUM_SCHEMA_VERSION <= SCHEMA_VERSION,
    "MINIMUM_SCHEMA_VERSION must be at least 1 and no greater than SCHEMA_VERSION"
);

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
/// Three outcomes, and the two rejections are different faults with different
/// copy (`storage.md` startup step 7):
///
/// | `user_version` | Outcome |
/// | -------------- | ------- |
/// | Above [`SCHEMA_VERSION`] | `unsupported_version { schema }`. A store written by a newer build is rejected, never misread. |
/// | [`MINIMUM_SCHEMA_VERSION`]`..=`[`SCHEMA_VERSION`] | Migrated to the current version and held. There is nothing to migrate at version 1, and this arm is where the chain goes when there is. |
/// | Below [`MINIMUM_SCHEMA_VERSION`] | `crypto { corrupt }`. **Not a lower version — not a FastClip store.** |
///
/// **The last row is the one worth reading twice.** `PRAGMA user_version`
/// defaults to 0 in any SQLite database that never set it, so "migrate anything
/// lower" is not a rule about old FastClip stores — at version 1 there are none
/// — it is a rule that silently accepts any stray SQLite file dropped at
/// `~/.fast-clip/clips.db`. This function used to do exactly that: it logged a
/// warning, returned `Ok`, and every later query against a `clips` table that
/// was never there failed with `storage`, telling the user their store could not
/// be read rather than that it is not their store.
///
/// `crypto { corrupt }` is reused rather than given its own reason because the
/// user's situation and remedy are identical to an unopenable database —
/// contract §4's description covers both, and a distinct reason would need a
/// distinct copy-deck sentence saying the same thing.
///
/// FastClip cannot create this case itself: [`create`] stamps the version inside
/// the same transaction that makes the schema, so a kill during creation leaves
/// no database rather than an unstamped one.
pub fn check_and_migrate(connection: &Connection) -> Result<(), ClipError> {
    let found = read_user_version(connection)?;

    if found > SCHEMA_VERSION {
        log::error!(
            "the store is at schema version {found}; this build understands {SCHEMA_VERSION}"
        );
        return Err(ClipError::UnsupportedVersion {
            component: VersionComponent::Schema,
            found,
            supported: SCHEMA_VERSION,
        });
    }

    if found < MINIMUM_SCHEMA_VERSION {
        log::error!(
            "clips.db reports schema version {found}; \
             the lowest this build can read is {MINIMUM_SCHEMA_VERSION}, \
             so this is a SQLite file that is not a FastClip store"
        );
        return Err(ClipError::Crypto {
            reason: CryptoReason::Corrupt,
        });
    }

    // In range. There is no migration content at version 1; the chain goes here.
    Ok(())
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

    /// **Version 0 is not an old store, it is not a FastClip store.** Any SQLite
    /// database that never set `PRAGMA user_version` reports 0, so accepting it
    /// means accepting a stray file dropped at `~/.fast-clip/clips.db` and then
    /// telling the user their clips could not be read.
    #[test]
    fn a_database_below_the_lowest_readable_version_is_corrupt_rather_than_migrated() {
        let dir = dir();
        let connection = fresh(&dir);
        for below in [MINIMUM_SCHEMA_VERSION - 1, -1] {
            if let Err(e) = connection.pragma_update(None, "user_version", below) {
                panic!("the fixture should be writable: {e}");
            }
            assert_eq!(
                check_and_migrate(&connection),
                Err(ClipError::Crypto {
                    reason: crate::error::CryptoReason::Corrupt,
                }),
                "for user_version {below}"
            );
        }
    }

    /// A SQLite database this application never wrote: no `clips` table, and no
    /// version stamp. It is refused at the version check, before any query can
    /// turn a missing table into `storage`.
    #[test]
    fn a_stray_sqlite_file_is_refused_before_a_query_can_call_it_storage() {
        let dir = dir();
        let path = dir.path().join("clips.db");
        let connection = match connection::open(&path) {
            Ok(connection) => connection,
            Err(e) => panic!("the database should open: {e}"),
        };
        // Someone else's database, with a table of their own and no stamp.
        if let Err(e) = connection.execute_batch("CREATE TABLE notes (body TEXT);") {
            panic!("the fixture should be writable: {e}");
        }

        assert_eq!(read_user_version(&connection), Ok(0));
        assert_eq!(
            check_and_migrate(&connection),
            Err(ClipError::Crypto {
                reason: crate::error::CryptoReason::Corrupt,
            })
        );
    }

    /// The floor is the lowest **readable** version, not zero. When a version 2
    /// ships, this is the assertion that keeps a version-1 store migrating
    /// rather than being called corrupt.
    #[test]
    fn every_version_from_the_floor_to_the_current_one_is_accepted() {
        let dir = dir();
        let connection = fresh(&dir);
        for version in MINIMUM_SCHEMA_VERSION..=SCHEMA_VERSION {
            if let Err(e) = connection.pragma_update(None, "user_version", version) {
                panic!("the fixture should be writable: {e}");
            }
            assert_eq!(
                check_and_migrate(&connection),
                Ok(()),
                "for user_version {version}"
            );
        }
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
             VALUES ('x', 'l', 'v', 'slate', 'not an integer', 0)",
            [],
        );
        assert!(result.is_err(), "STRICT should refuse a TEXT use_count");
    }

    #[test]
    fn position_is_unique() {
        let dir = dir();
        let connection = fresh(&dir);
        let insert = "INSERT INTO clips (id, label, value, colour, use_count, position)
                      VALUES (?1, 'l', 'v', 'slate', 0, 0)";
        if let Err(e) = connection.execute(insert, ["a"]) {
            panic!("the first insert should succeed: {e}");
        }
        assert!(
            connection.execute(insert, ["b"]).is_err(),
            "two clips must not share a position"
        );
    }
}
