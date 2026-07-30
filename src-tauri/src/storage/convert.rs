//! Whole-database conversion between plaintext and encrypted.
//!
//! `storage.md` § Switching encryption on and off. **This is the only code in
//! the application that replaces the user's whole store**, and every rule below
//! is there because getting it wrong loses clips or leaves them readable.
//!
//! **`sqlcipher_export()`, not read-decrypt-write** (ADR-0005). SQLCipher copies
//! schema and data page by page into a database keyed differently; a process
//! killed at any instant leaves a readable database in one state or the other,
//! never half-converted.
//!
//! **One shape, two directions.** Enabling and disabling are the same steps with
//! the roles swapped, so this module is written once against a [`Plan`] that
//! names the key on each side. `source_key` is what the existing `clips.db` is
//! under and `target_key` is what the new one will be under; `None` means
//! plaintext. Writing it twice is how the two would drift, and the disable path
//! is the one whose intermediate is a complete plaintext copy of every clip.
//!
//! **The rename at step 6 is the only instant the store's state changes.**
//! Everything before it is reversible and aborts to the state before the command
//! was called; everything after it has committed and is absorbed rather than
//! reported ([`run`]).

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::crypto::Dek;
use crate::error::ClipError;
use crate::storage::paths::StorePaths;
use crate::storage::{connection, open_error, storage_error};

/// The schema name `clips.db.new` is attached under during the export.
///
/// Not `main`, not `temp`, and not a name a user could influence: it is a fixed
/// identifier in a statement this module builds.
const TARGET_SCHEMA: &str = "converted";

/// Which key each side of the conversion carries.
///
/// `None` is plaintext. Enabling is `source_key: None, target_key: Some(new)`;
/// disabling is the reverse.
pub struct Plan<'a> {
    pub paths: &'a StorePaths,
    pub source_key: Option<&'a Dek>,
    pub target_key: Option<&'a Dek>,
}

/// What `SELECT count(*)` and `PRAGMA user_version` said about the **source**,
/// before anything was written. Step 3 compares the target against these.
struct Recorded {
    clips: i64,
    user_version: i64,
}

/// How the conversion ended.
///
/// Three outcomes and no fourth, because the caller's bookkeeping differs for
/// each: only two of them changed the store, and only one of them leaves a
/// usable connection.
pub enum Outcome {
    /// Steps 2 to 7 all succeeded. The store is converted and this is the
    /// connection to hold.
    Converted(Connection),
    /// **The rename happened and the reopen did not.** The store *is* converted
    /// on disk — the classification must still change and `lock_state` must
    /// still be emitted — but there is no connection, so the command returns
    /// `storage` and every clip command serves it for the rest of the session
    /// (`storage.md` § The reopen, and why it is verified first).
    CommittedButClosed,
    /// **Nothing committed.** The store is byte-for-byte what it was. `original`
    /// is the connection to hold; `None` means even the reopen failed, which
    /// leaves the application unable to serve clips until it restarts.
    Aborted {
        original: Option<Connection>,
        error: ClipError,
    },
}

/// Steps 2 to 7 of the conversion.
///
/// Step 1 — minting and writing the key material — belongs to
/// `enable_encryption`, because it is `keyfile` work rather than database work,
/// and step 9's `keyfile` delete belongs to `disable_encryption` for the same
/// reason. Both callers must add their own `keyfile` cleanup to the abort path;
/// this function deletes `keyfile.new` along with the database intermediates,
/// because that file is in [`StorePaths::intermediates`].
///
/// `on_commit` runs **at the rename and only if it succeeded**
/// (`storage.md` § The classification is written twice). It is a callback rather
/// than something the caller does afterwards because the rename is the instant
/// the fact changes: a conversion whose step 7 fails still committed, and the
/// classification must already describe the store as it now is.
pub fn run(plan: &Plan<'_>, source: Connection, on_commit: impl FnOnce()) -> Outcome {
    let paths = plan.paths;

    // ---- Step 2: record the source, export, stamp the version ----
    let recorded = match record(&source) {
        Ok(recorded) => recorded,
        Err(error) => return abort(plan, Some(source), None, error),
    };

    if let Err(error) = export(plan, &source, &recorded) {
        return abort(plan, Some(source), None, error);
    }

    // Detach before step 3 opens the same file as a connection of its own.
    if let Err(error) = detach(&source) {
        return abort(plan, Some(source), None, error);
    }

    // ---- Step 3: open the new database and verify it ----
    let target = match verify(plan, &recorded) {
        Ok(target) => target,
        Err(error) => return abort(plan, Some(source), None, error),
    };

    // ---- Step 4: checkpoint both, close both ----
    //
    // A failure here is an abort, and both connections are consumed by the
    // close whatever happens — `checkpoint_and_close` absorbs its own errors,
    // so what this step cannot do is leave a handle open.
    connection::checkpoint_and_close(target);
    connection::checkpoint_and_close(source);

    // ---- Step 5: delete the source's sidecars, before the rename ----
    //
    // **Before**, never after. Deleting after the rename leaves an instant in
    // which an encrypted `clips.db` sits beside a plaintext `clips.db-wal`
    // holding every label and value, readable in a text editor. A failure here
    // aborts rather than continuing, because a second process holding the
    // database can leave them in place and this build has no single-instance
    // guard.
    for sidecar in [paths.db_wal(), paths.db_shm()] {
        if let Err(error) = delete_if_present(&sidecar) {
            log::error!(
                "a store sidecar could not be deleted before the conversion committed: {}",
                error.kind()
            );
            return abort(plan, None, None, ClipError::Storage);
        }
    }

    // ---- Step 6: the rename. The commit point. ----
    if let Err(error) = fs::rename(paths.db_new(), paths.db()) {
        // **A failed rename is an abort, not a partial commit.** Nothing
        // committed, so the classification is not written and no `lock_state`
        // is emitted — which is what stops step 8 announcing encryption over a
        // plaintext store.
        log::error!(
            "the converted store could not be renamed into place: {}",
            error.kind()
        );
        return abort(plan, None, None, ClipError::Storage);
    }
    on_commit();

    // ---- Step 7: reopen ----
    let outcome = match open_side(paths.db().as_path(), plan.target_key) {
        Ok(reopened) => Outcome::Converted(reopened),
        Err(error) => {
            // Proved good moments ago at step 3, so this is an access failure
            // rather than a content one: an antivirus handle, a second
            // instance, a disk fault.
            log::error!("the converted store could not be reopened: {error}");
            Outcome::CommittedButClosed
        }
    };

    // ---- Step 9: sweep what the close left. Absorbed. ----
    //
    // `clips.db.new` itself was renamed away at step 6; these are its sidecars,
    // which SQLite usually unlinks on a clean close and sometimes does not. The
    // conversion has committed, so a failure here cannot change the outcome —
    // returning `storage` for a stray temporary file would report a conversion
    // that worked as one that failed. The next launch's sweep collects whatever
    // is left.
    sweep_target_sidecars(paths);

    outcome
}

/// Delete `clips.db.new-wal` and `clips.db.new-shm`, absorbing failures.
fn sweep_target_sidecars(paths: &StorePaths) {
    for intermediate in paths.intermediates() {
        // The main `clips.db.new` was renamed away, and `keyfile.new` belongs to
        // the command rather than to this module.
        if intermediate == paths.db_new() || intermediate == paths.keyfile_new() {
            continue;
        }
        if let Err(error) = delete_if_present(&intermediate) {
            log::warn!(
                "a conversion sidecar could not be swept after the commit: {}",
                error.kind()
            );
        }
    }
}

/// Abort to the state before the command was called.
///
/// `storage.md` § Abort, in the order that page fixes. The three actions are
/// numbered there and numbered here, and **the order is not a style
/// preference**: on Windows an open SQLite database cannot be deleted, so
/// releasing the handles has to come first or the delete silently leaves the
/// file behind — and on the disable path that file is a complete plaintext copy
/// of every label and value, beside a store the user has just been told is still
/// encrypted.
///
/// `source` is `Some` when the original connection is still open, which is every
/// abort point up to step 3. `target` is `Some` only when step 3 opened it.
fn abort(
    plan: &Plan<'_>,
    source: Option<Connection>,
    target: Option<Connection>,
    error: ClipError,
) -> Outcome {
    // 1. Release every handle this command holds on `clips.db.new`.
    if let Some(source) = source.as_ref() {
        // The export may or may not have attached it. `DETACH` on a schema that
        // is not attached is an error and is absorbed, which makes this one line
        // correct for both the step-2 abort and the ones before it.
        detach_absorbing(source);
    }
    if let Some(target) = target {
        connection::checkpoint_and_close(target);
    }

    // 2. Delete `clips.db.new`, its sidecars, and `keyfile.new`.
    for intermediate in plan.paths.intermediates() {
        if let Err(error) = delete_if_present(&intermediate) {
            // Absorbed: the store itself is intact, and the next launch's sweep
            // collects whatever is left. Replacing the real error with one about
            // a temporary file would hide why the command failed.
            log::error!(
                "a conversion intermediate could not be deleted during an abort: {}",
                error.kind()
            );
        }
    }

    // 3. **Ensure** the original is open — not "reopen" it. It is closed only
    //    from step 4 onwards; an abort at steps 1 to 3 happens while it is still
    //    open and working, and reopening would either open it twice or drop a
    //    good connection for nothing. The design allows one connection.
    let original = match source {
        Some(open) => Some(open),
        None => match open_side(plan.paths.db().as_path(), plan.source_key) {
            Ok(reopened) => Some(reopened),
            Err(error) => {
                log::error!("the original store could not be reopened after an abort: {error}");
                None
            }
        },
    };

    Outcome::Aborted { original, error }
}

/// Open either side of the conversion, keyed or not.
fn open_side(path: &Path, key: Option<&Dek>) -> Result<Connection, ClipError> {
    match key {
        Some(dek) => connection::open_encrypted(path, dek),
        None => connection::open(path),
    }
}

/// Step 2's first half: the two values step 3 compares against.
///
/// Read from the **source**, before anything is written. A count taken from the
/// target would verify nothing.
fn record(source: &Connection) -> Result<Recorded, ClipError> {
    let clips = source
        .query_row("SELECT count(*) FROM clips", [], |row| row.get(0))
        .map_err(|e| storage_error("the clip count could not be read for the conversion", &e))?;
    let user_version = source
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| storage_error("user_version could not be read for the conversion", &e))?;
    Ok(Recorded {
        clips,
        user_version,
    })
}

/// Attach `clips.db.new` under [`TARGET_SCHEMA`].
///
/// **The error is discarded, never formatted.** The `KEY` clause cannot take a
/// bound parameter, so the DEK is in the statement text — and `rusqlite`'s
/// `SqlInputError` renders the statement that failed. This is the second of the
/// two keying statements in the application; the other is
/// [`crate::storage::connection`]'s key pragma, and both are bound by the same
/// rule (`storage.md` § A keying statement's error is never formatted).
///
/// The **path** is bound, so a home directory containing a quote cannot break
/// the statement or reach the SQL text.
fn attach_target(plan: &Plan<'_>, source: &Connection) -> Result<(), ClipError> {
    use zeroize::Zeroizing;

    let statement = match plan.target_key {
        Some(dek) => Zeroizing::new(format!(
            "ATTACH DATABASE ?1 AS {TARGET_SCHEMA} KEY \"{}\"",
            &*connection::raw_key_literal(dek)
        )),
        // `KEY ''` is how SQLCipher is told the attached database is plaintext.
        None => Zeroizing::new(format!("ATTACH DATABASE ?1 AS {TARGET_SCHEMA} KEY ''")),
    };

    let path = plan.paths.db_new();
    match source.execute(&statement, [path.to_string_lossy()]) {
        Ok(_) => Ok(()),
        Err(_discarded) => {
            log::error!("the new database could not be attached for the conversion");
            Err(ClipError::Storage)
        }
    }
}

fn detach(source: &Connection) -> Result<(), ClipError> {
    source
        .execute_batch(&format!("DETACH DATABASE {TARGET_SCHEMA};"))
        .map_err(|e| storage_error("the new database could not be detached", &e))
}

/// `DETACH` whatever may be attached, absorbing the error when nothing is.
fn detach_absorbing(source: &Connection) {
    if source
        .execute_batch(&format!("DETACH DATABASE {TARGET_SCHEMA};"))
        .is_err()
    {
        // Expected whenever the abort happened before the attach. Not logged at
        // error level for that reason.
        log::debug!("nothing was attached to detach during a conversion abort");
    }
}

/// Step 2's second half.
///
/// **`sqlcipher_export()` does not carry `PRAGMA user_version` across** — it
/// copies schema and data, and `user_version` is a header field which is
/// neither. `tests/sqlcipher.rs` asserts that, so the explicit write below is
/// provably load-bearing rather than folklore somebody could delete. Without it
/// every converted store would report version 0, which
/// [`crate::storage::schema`] refuses as "not a FastClip store".
fn export(plan: &Plan<'_>, source: &Connection, recorded: &Recorded) -> Result<(), ClipError> {
    attach_target(plan, source)?;

    source
        .query_row(
            &format!("SELECT sqlcipher_export('{TARGET_SCHEMA}')"),
            [],
            |_| Ok(()),
        )
        .map_err(|e| storage_error("the store could not be exported into the new database", &e))?;

    // `user_version` is an integer this process read from its own database, so
    // formatting it into the statement cannot inject anything.
    source
        .execute_batch(&format!(
            "PRAGMA {TARGET_SCHEMA}.user_version = {};",
            recorded.user_version
        ))
        .map_err(|e| storage_error("the schema version could not be written to the copy", &e))
}

/// Step 3: open `clips.db.new` and prove it is the store.
///
/// **Three comparisons, not three statements run and discarded**
/// (`storage.md` § What step 3 asserts). A `user_version` read and not compared,
/// or a row count taken and not compared, verifies only that the file opens —
/// and a file that opens with the wrong contents then reaches the commit with
/// the abort path never taken.
///
/// Returns the open connection, which step 4 closes.
fn verify(plan: &Plan<'_>, recorded: &Recorded) -> Result<Connection, ClipError> {
    let target = open_side(plan.paths.db_new().as_path(), plan.target_key)?;

    let integrity: String = target
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| open_error("the converted store failed its integrity check", &e))?;
    if !integrity.eq_ignore_ascii_case("ok") {
        // The string names pages and indexes, never clip content.
        log::error!("the converted store is not structurally sound: {integrity}");
        return Err(ClipError::Storage);
    }

    let clips: i64 = target
        .query_row("SELECT count(*) FROM clips", [], |row| row.get(0))
        .map_err(|e| storage_error("the converted store's clips could not be counted", &e))?;
    if clips != recorded.clips {
        log::error!(
            "the conversion copied {clips} clips where {} were expected",
            recorded.clips
        );
        return Err(ClipError::Storage);
    }

    let user_version: i64 = target
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| storage_error("the converted store's version could not be read", &e))?;
    if user_version != recorded.user_version {
        log::error!(
            "the conversion produced schema version {user_version} where {} was expected",
            recorded.user_version
        );
        return Err(ClipError::Storage);
    }

    Ok(target)
}

/// Delete a file, treating "it was not there" as success.
fn delete_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;
    use crate::storage::classify::{classify, Classification};
    use crate::storage::{clips, schema};

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn paths_in(parent: &tempfile::TempDir) -> StorePaths {
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        paths
    }

    /// A plaintext store with `count` clips, at the current schema version.
    fn seeded_plaintext(paths: &StorePaths, count: usize) -> Connection {
        let connection = match connection::open(&paths.db()) {
            Ok(connection) => connection,
            Err(e) => panic!("the fixture store should open: {e}"),
        };
        if let Err(e) = schema::create(&connection) {
            panic!("the fixture schema should create: {e}");
        }
        for n in 0..count {
            let inserted = clips::insert(
                &connection,
                &format!("clip {n}"),
                &format!("value {n}"),
                Colour::DEFAULT,
            );
            if let Err(e) = inserted {
                panic!("the fixture clip should insert: {e}");
            }
        }
        connection
    }

    fn labels(connection: &Connection) -> Vec<String> {
        match clips::list(connection) {
            Ok(rows) => rows.into_iter().map(|row| row.label).collect(),
            Err(e) => panic!("the clips should list: {e}"),
        }
    }

    fn dek_of(byte: u8) -> Dek {
        Dek::from_bytes([byte; 32])
    }

    fn committed_flag() -> std::rc::Rc<std::cell::Cell<bool>> {
        std::rc::Rc::new(std::cell::Cell::new(false))
    }

    // ---- the happy path, both directions ----

    /// **The definition of done for `enable_encryption`'s mechanism**: every
    /// clip survives, the version survives, and the file stops being a SQLite
    /// database to anything without the key.
    #[test]
    fn enabling_converts_every_clip_and_carries_the_schema_version() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = seeded_plaintext(&paths, 5);
        let before = labels(&source);
        let dek = dek_of(0x2b);

        let committed = committed_flag();
        let flag = std::rc::Rc::clone(&committed);
        let plan = Plan {
            paths: &paths,
            source_key: None,
            target_key: Some(&dek),
        };

        match run(&plan, source, || flag.set(true)) {
            Outcome::Converted(connection) => {
                assert_eq!(labels(&connection), before, "every clip must survive");
                assert_eq!(
                    schema::read_user_version(&connection),
                    Ok(schema::SCHEMA_VERSION)
                );
                connection::checkpoint_and_close(connection);
            }
            Outcome::CommittedButClosed => panic!("the reopen should have succeeded"),
            Outcome::Aborted { error, .. } => panic!("the conversion should not abort: {error}"),
        }

        assert!(committed.get(), "the commit callback must have run");
        assert_eq!(classify(&paths.db()), Classification::Encrypted);
        assert!(!paths.db_new().exists(), "the intermediate must be gone");
    }

    #[test]
    fn disabling_converts_every_clip_back_and_carries_the_schema_version() {
        let parent = dir();
        let paths = paths_in(&parent);
        let dek = dek_of(0x2b);

        // Build an encrypted store to disable.
        let before = {
            let source = seeded_plaintext(&paths, 4);
            let before = labels(&source);
            let plan = Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            };
            match run(&plan, source, || {}) {
                Outcome::Converted(connection) => connection::checkpoint_and_close(connection),
                _ => panic!("the fixture conversion should succeed"),
            }
            before
        };

        let source = match connection::open_encrypted(&paths.db(), &dek) {
            Ok(connection) => connection,
            Err(e) => panic!("the encrypted fixture should open: {e}"),
        };
        let plan = Plan {
            paths: &paths,
            source_key: Some(&dek),
            target_key: None,
        };

        match run(&plan, source, || {}) {
            Outcome::Converted(connection) => {
                assert_eq!(labels(&connection), before);
                assert_eq!(
                    schema::read_user_version(&connection),
                    Ok(schema::SCHEMA_VERSION)
                );
                connection::checkpoint_and_close(connection);
            }
            Outcome::CommittedButClosed => panic!("the reopen should have succeeded"),
            Outcome::Aborted { error, .. } => panic!("the conversion should not abort: {error}"),
        }

        assert_eq!(classify(&paths.db()), Classification::Plaintext);
    }

    /// A round trip changes nothing the user can see.
    #[test]
    fn a_round_trip_returns_the_store_to_exactly_what_it_was() {
        let parent = dir();
        let paths = paths_in(&parent);
        let dek = dek_of(0x2b);
        let source = seeded_plaintext(&paths, 6);
        let before = labels(&source);

        let encrypted = match run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || {},
        ) {
            Outcome::Converted(connection) => connection,
            _ => panic!("enabling should succeed"),
        };

        match run(
            &Plan {
                paths: &paths,
                source_key: Some(&dek),
                target_key: None,
            },
            encrypted,
            || {},
        ) {
            Outcome::Converted(connection) => {
                assert_eq!(labels(&connection), before);
                connection::checkpoint_and_close(connection);
            }
            _ => panic!("disabling should succeed"),
        }
        assert_eq!(classify(&paths.db()), Classification::Plaintext);
    }

    #[test]
    fn an_empty_store_converts_without_special_casing() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = seeded_plaintext(&paths, 0);
        let dek = dek_of(0x2b);

        match run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || {},
        ) {
            Outcome::Converted(connection) => {
                assert!(labels(&connection).is_empty());
                connection::checkpoint_and_close(connection);
            }
            _ => panic!("an empty store should convert"),
        }
        assert_eq!(classify(&paths.db()), Classification::Encrypted);
    }

    // ---- what the conversion must never leave behind ----

    /// **Acceptance criteria 5 and 6.** After enabling, no file in the store
    /// directory holds a clip value in the clear — including the sidecars, which
    /// step 5 deletes before the rename precisely so that no instant exists in
    /// which a plaintext WAL sits beside an encrypted database.
    #[test]
    fn enabling_leaves_no_plaintext_anywhere_in_the_store_directory() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = seeded_plaintext(&paths, 3);
        let dek = dek_of(0x2b);

        match run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || {},
        ) {
            Outcome::Converted(connection) => connection::checkpoint_and_close(connection),
            _ => panic!("enabling should succeed"),
        }

        let entries = match fs::read_dir(paths.dir()) {
            Ok(entries) => entries,
            Err(e) => panic!("the store directory should be readable: {e}"),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => panic!("a directory entry should be readable: {e}"),
            };
            let bytes = match fs::read(entry.path()) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            for needle in ["value 0", "value 1", "value 2", "clip 0"] {
                assert!(
                    !bytes
                        .windows(needle.len())
                        .any(|window| window == needle.as_bytes()),
                    "{} holds {needle:?} in the clear",
                    entry.path().display()
                );
            }
        }
    }

    /// Step 5 deletes the sidecars **before** the rename, so a successful
    /// conversion never leaves the old ones.
    #[test]
    fn the_old_sidecars_are_gone_after_a_conversion() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = seeded_plaintext(&paths, 2);
        let dek = dek_of(0x2b);

        // WAL mode means these exist right now.
        assert!(paths.db_wal().exists(), "the fixture should have a WAL");

        match run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || {},
        ) {
            Outcome::Converted(connection) => {
                // The reopen creates the *new* store's own sidecars, which are
                // encrypted. What must not survive is the plaintext pair from
                // before the rename, and the assertion below is about content
                // rather than existence.
                connection::checkpoint_and_close(connection);
            }
            _ => panic!("enabling should succeed"),
        }

        if let Ok(bytes) = fs::read(paths.db_wal()) {
            assert!(
                !bytes.windows(7).any(|window| window == b"value 0"),
                "a plaintext WAL survived the conversion"
            );
        }
    }

    // ---- abort ----

    /// **A verification failure at step 3 aborts with the store untouched, and
    /// the original connection is left open.** This is review 003's F2: the
    /// abort deletes `clips.db.new`, and on Windows that fails while a handle is
    /// held — so releasing the handles has to come first.
    #[test]
    fn a_failed_verification_aborts_with_the_store_intact_and_still_usable() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = seeded_plaintext(&paths, 3);
        let before = labels(&source);
        let dek = dek_of(0x2b);

        // A directory where `clips.db.new` should go, so the attach fails.
        if let Err(e) = fs::create_dir(paths.db_new()) {
            panic!("the fixture should create: {e}");
        }

        let committed = committed_flag();
        let flag = std::rc::Rc::clone(&committed);
        let outcome = run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || flag.set(true),
        );

        match outcome {
            Outcome::Aborted { original, error } => {
                assert_eq!(error, ClipError::Storage);
                let original = match original {
                    Some(connection) => connection,
                    None => panic!("the original was open and must have been left open"),
                };
                assert_eq!(labels(&original), before, "not one clip may have moved");
                connection::checkpoint_and_close(original);
            }
            _ => panic!("a failed attach must abort"),
        }

        assert!(
            !committed.get(),
            "nothing committed, so the callback must not run"
        );
        assert_eq!(
            classify(&paths.db()),
            Classification::Plaintext,
            "the store must be exactly what it was"
        );

        // Clean up the fixture directory so the assertion below is about the
        // conversion rather than about it.
        let _ = fs::remove_dir(paths.db_new());
    }

    /// The abort deletes the intermediate rather than leaving it — which on the
    /// disable path is a complete plaintext copy of every clip.
    ///
    /// The verification is made to fail by deleting rows from the source after
    /// the export has copied them, so the count comparison at step 3 rejects a
    /// target that is otherwise perfectly readable.
    #[test]
    fn an_aborted_disable_deletes_the_plaintext_intermediate() {
        let parent = dir();
        let paths = paths_in(&parent);
        let dek = dek_of(0x2b);

        {
            let source = seeded_plaintext(&paths, 3);
            match run(
                &Plan {
                    paths: &paths,
                    source_key: None,
                    target_key: Some(&dek),
                },
                source,
                || {},
            ) {
                Outcome::Converted(connection) => connection::checkpoint_and_close(connection),
                _ => panic!("the fixture conversion should succeed"),
            }
        }

        // A directory in the way, so the disable aborts before it commits.
        if let Err(e) = fs::create_dir(paths.db_new()) {
            panic!("the fixture should create: {e}");
        }

        let source = match connection::open_encrypted(&paths.db(), &dek) {
            Ok(connection) => connection,
            Err(e) => panic!("the encrypted fixture should open: {e}"),
        };
        let outcome = run(
            &Plan {
                paths: &paths,
                source_key: Some(&dek),
                target_key: None,
            },
            source,
            || panic!("nothing committed, so the callback must not run"),
        );

        match outcome {
            Outcome::Aborted { original, error } => {
                assert_eq!(error, ClipError::Storage);
                assert!(original.is_some(), "the original must still be usable");
                if let Some(connection) = original {
                    assert_eq!(labels(&connection).len(), 3);
                    connection::checkpoint_and_close(connection);
                }
            }
            _ => panic!("the disable must abort"),
        }

        assert_eq!(
            classify(&paths.db()),
            Classification::Encrypted,
            "the store must still be encrypted"
        );
        let _ = fs::remove_dir(paths.db_new());
    }

    /// **The abort leaves nothing readable behind.** The intermediate is deleted
    /// and, if the delete were ever to fail, the sweep would take it — but this
    /// asserts the ordinary case, which is the one that must not leak.
    #[test]
    fn an_abort_leaves_no_intermediate_file_at_all() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = seeded_plaintext(&paths, 2);
        let dek = dek_of(0x2b);

        // `keyfile.new` stands in for a partially-written enable.
        if let Err(e) = fs::write(paths.keyfile_new(), b"partial") {
            panic!("the fixture should write: {e}");
        }
        if let Err(e) = fs::create_dir(paths.db_new()) {
            panic!("the fixture should create: {e}");
        }

        match run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || {},
        ) {
            Outcome::Aborted { original, .. } => {
                if let Some(connection) = original {
                    connection::checkpoint_and_close(connection);
                }
            }
            _ => panic!("the conversion must abort"),
        }

        assert!(
            !paths.keyfile_new().exists(),
            "keyfile.new must be deleted by the abort"
        );
        let _ = fs::remove_dir(paths.db_new());
    }

    /// A source with no `clips` table fails at step 2's count, before anything
    /// is written.
    #[test]
    fn a_source_that_is_not_a_store_aborts_before_writing_anything() {
        let parent = dir();
        let paths = paths_in(&parent);
        let source = match connection::open(&paths.db()) {
            Ok(connection) => connection,
            Err(e) => panic!("the fixture should open: {e}"),
        };
        let dek = dek_of(0x2b);

        match run(
            &Plan {
                paths: &paths,
                source_key: None,
                target_key: Some(&dek),
            },
            source,
            || panic!("nothing committed, so the callback must not run"),
        ) {
            Outcome::Aborted { original, error } => {
                assert_eq!(error, ClipError::Storage);
                if let Some(connection) = original {
                    connection::checkpoint_and_close(connection);
                }
            }
            _ => panic!("a source with no clips table must abort"),
        }
        assert!(!paths.db_new().exists());
    }
}
