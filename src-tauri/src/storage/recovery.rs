//! Startup recovery: the seven steps that run before any command is served.
//!
//! `docs/src/architecture/storage.md` § Startup recovery. Each step's failure
//! behaviour is fixed by the table in that section and is repeated here on the
//! step it governs. "Fatal" means the fault is recorded and served from a
//! command — startup always runs to completion, because a backend that refuses
//! to start leaves nothing to show the user.

use std::fs;
use std::io;
use std::path::Path;

use rusqlite::Connection;

use crate::error::ClipError;
use crate::storage::classify::{classify, Classification};
use crate::storage::paths::StorePaths;
use crate::storage::{connection, schema};

/// What startup recovery established.
pub struct Recovered {
    /// The four-state answer to "is the store encrypted?". Held in memory and
    /// never re-derived; a conversion's commit is the only other writer.
    pub classification: Classification,
    /// The live connection, held open. `None` when the store is encrypted (no
    /// DEK exists until a PIN unwraps one) or when a step failed.
    pub connection: Option<Connection>,
    /// The recorded fault, served by `get_lock_state`.
    pub fault: Option<ClipError>,
}

/// Run startup recovery against one store directory.
pub fn recover(paths: &StorePaths) -> Recovered {
    // Step 1 — create `~/.fast-clip/`. **Fatal**: nothing works without it.
    if let Err(error) = fs::create_dir_all(paths.dir()) {
        log::error!("the store directory could not be created: {error}");
        // Not classified: nothing is known about `clips.db`, so nothing may be
        // deleted or created. `Unreadable` is exactly that state.
        return Recovered {
            classification: Classification::Unreadable,
            connection: None,
            fault: Some(ClipError::Storage),
        };
    }

    // Step 2 — sweep the intermediates. **Ungated and before the
    // classification**, so no later branch can suppress it: `clips.db.new` on
    // the disable-encryption path is a complete plaintext copy of every clip,
    // and the classification most likely to coincide with one is also the one
    // that would have suppressed the sweep. Absorbed on failure.
    for path in paths.intermediates() {
        delete_absorbing(&path, "conversion intermediate");
    }

    // Step 3 — classify. A read that fails *is* the `unreadable` classification.
    let classification = classify(&paths.db());

    // Step 4 — stop on `unreadable`. Delete nothing further, create nothing,
    // open nothing. Treating this as absence is what destroys an encrypted
    // store, because steps 5 and 6 are both gated on the classification.
    if classification == Classification::Unreadable {
        log::error!("clips.db is present but could not be read; nothing was deleted or created");
        return Recovered {
            classification,
            connection: None,
            fault: Some(ClipError::Storage),
        };
    }

    // Step 5 — delete a stray `keyfile`, and **only** on a positive `absent` or
    // `plaintext` classification. **Absorbed** on failure: the file it deletes
    // wraps a DEK for a database that is plaintext or absent, so it decrypts
    // nothing and cannot confuse the state machine. Propagating the failure
    // would put a user in front of the failure screen — which points at import
    // as the recovery path — over a completely intact store.
    if classification.permits_keyfile_delete() {
        delete_absorbing(&paths.keyfile(), "keyfile");
    }

    // Step 6 — create the store when it is absent. **Fatal** as `storage`.
    if classification == Classification::Absent {
        if let Err(error) = create_store(&paths.db()) {
            log::error!("clips.db is absent and could not be created: {error}");
            return Recovered {
                classification,
                connection: None,
                fault: Some(ClipError::Storage),
            };
        }
    }

    // Step 7 — open and check `user_version`, and hold the connection.
    // An encrypted store is not opened: there is no DEK until a PIN unwraps
    // one, so the same two faults belong to `unlock`.
    if classification.is_encrypted() {
        log::info!("the store is encrypted; it stays closed until a PIN unwraps the key");
        return Recovered {
            classification,
            connection: None,
            fault: None,
        };
    }

    match open_and_check(&paths.db()) {
        Ok(open) => Recovered {
            classification,
            connection: Some(open),
            fault: None,
        },
        Err(fault) => {
            // **Fatal** as `crypto { corrupt }` or `unsupported_version`.
            // Startup itself still completes, so there is a window in which to
            // show the message.
            log::error!("the store could not be opened at startup: {fault}");
            Recovered {
                classification,
                connection: None,
                fault: Some(fault),
            }
        }
    }
}

/// Delete a file, absorbing every failure.
///
/// The one legitimate cause is another process holding it — a second FastClip
/// mid-conversion — and deleting it there would break that conversion. The next
/// launch tries again, which is why the honest bound on the exposure is "until a
/// launch at which the delete succeeds" rather than "until the next launch".
fn delete_absorbing(path: &Path, what: &str) {
    match fs::remove_file(path) {
        Ok(()) => log::info!("removed a stray {what}"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => log::warn!(
            "a stray {what} could not be removed and will be retried at the next launch: {error}"
        ),
    }
}

/// Create `clips.db` with the schema and `PRAGMA user_version = 1`, then close
/// it. Step 7 reopens it, so what was written is proved readable rather than
/// assumed to be.
fn create_store(path: &Path) -> Result<(), ClipError> {
    let open = connection::open(path)?;
    let created = schema::create(&open);
    connection::checkpoint_and_close(open);
    created
}

/// Open the store and apply the version check.
fn open_and_check(path: &Path) -> Result<Connection, ClipError> {
    let open = connection::open(path)?;
    schema::check_and_migrate(&open)?;
    Ok(open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;
    use crate::storage::clips;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Err(e) = fs::write(path, bytes) {
            panic!("could not write the fixture: {e}");
        }
    }

    /// A store directory that does not exist yet, inside a temporary parent.
    fn paths_in(parent: &tempfile::TempDir) -> StorePaths {
        StorePaths::at(parent.path().join(".fast-clip"))
    }

    #[test]
    fn a_first_run_creates_the_directory_and_an_empty_versioned_store() {
        let parent = dir();
        let paths = paths_in(&parent);
        let recovered = recover(&paths);

        assert_eq!(recovered.classification, Classification::Absent);
        assert!(recovered.fault.is_none(), "{:?}", recovered.fault);
        assert!(paths.dir().is_dir());
        assert!(paths.db().is_file());

        let open = match recovered.connection {
            Some(open) => open,
            None => panic!("the connection should be held after a first run"),
        };
        assert_eq!(schema::read_user_version(&open), Ok(schema::SCHEMA_VERSION));
        assert_eq!(clips::list(&open).map(|c| c.len()), Ok(0));
    }

    #[test]
    fn an_existing_plaintext_store_is_reopened_with_its_clips_in_order() {
        let parent = dir();
        let paths = paths_in(&parent);
        {
            let first = recover(&paths);
            let open = match first.connection {
                Some(open) => open,
                None => panic!("the first run should hold a connection"),
            };
            for label in ["first", "second", "third"] {
                if let Err(e) = clips::insert(&open, label, "a value", Colour::DEFAULT) {
                    panic!("the insert should succeed: {e}");
                }
            }
            connection::checkpoint_and_close(open);
        }

        let second = recover(&paths);
        assert_eq!(second.classification, Classification::Plaintext);
        assert!(second.fault.is_none(), "{:?}", second.fault);
        let open = match second.connection {
            Some(open) => open,
            None => panic!("the second run should hold a connection"),
        };
        let labels: Vec<String> = match clips::list(&open) {
            Ok(rows) => rows.into_iter().map(|c| c.label).collect(),
            Err(e) => panic!("the list should succeed: {e}"),
        };
        assert_eq!(labels, vec!["first", "second", "third"]);
    }

    #[test]
    fn a_stray_keyfile_beside_a_plaintext_store_is_removed() {
        let parent = dir();
        let paths = paths_in(&parent);
        let first = recover(&paths);
        drop(first.connection);
        write(&paths.keyfile(), b"debris from an interrupted conversion");

        let second = recover(&paths);
        assert_eq!(second.classification, Classification::Plaintext);
        assert!(
            !paths.keyfile().exists(),
            "the stray keyfile should be gone"
        );
    }

    #[test]
    fn a_stray_keyfile_is_removed_before_a_fresh_store_is_created_beside_it() {
        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        write(&paths.keyfile(), b"left over from before");

        let recovered = recover(&paths);
        assert_eq!(recovered.classification, Classification::Absent);
        assert!(!paths.keyfile().exists());
        assert!(paths.db().is_file());
    }

    #[test]
    fn the_keyfile_beside_an_encrypted_store_is_never_touched() {
        // The store's only key. Deleting it makes every clip permanently
        // unrecoverable, and there is no reset path.
        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        write(&paths.db(), &[0x1f; 64]); // a random salt, not the SQLite magic
        write(&paths.keyfile(), b"the wrapped DEK");

        let recovered = recover(&paths);
        assert_eq!(recovered.classification, Classification::Encrypted);
        assert!(paths.keyfile().is_file(), "the keyfile must survive");
        assert!(
            recovered.connection.is_none(),
            "an encrypted store stays shut"
        );
        assert!(recovered.fault.is_none(), "being encrypted is not a fault");
    }

    #[test]
    fn an_unreadable_store_deletes_nothing_creates_nothing_and_reports_storage() {
        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // A directory where the database should be: it opens with something
        // other than NotFound, which is the whole distinction.
        if let Err(e) = fs::create_dir(paths.db()) {
            panic!("could not create the fixture: {e}");
        }
        write(&paths.keyfile(), b"the wrapped DEK");

        let recovered = recover(&paths);
        assert_eq!(recovered.classification, Classification::Unreadable);
        assert_eq!(recovered.fault, Some(ClipError::Storage));
        assert!(recovered.connection.is_none());
        assert!(
            paths.keyfile().is_file(),
            "an unreadable classification permits no delete"
        );
        assert!(paths.db().is_dir(), "nothing may be created over it");
    }

    #[test]
    fn the_intermediates_are_swept_even_when_the_store_is_unreadable() {
        // The sweep is ungated for this case exactly: an interrupted
        // disable-encryption leaves a complete plaintext copy of every clip in
        // `clips.db.new`, and gating the sweep would let it survive every launch
        // for as long as another process held the database.
        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        if let Err(e) = fs::create_dir(paths.db()) {
            panic!("could not create the fixture: {e}");
        }
        for path in paths.intermediates() {
            write(&path, b"every label and value, in the clear");
        }

        let recovered = recover(&paths);
        assert_eq!(recovered.classification, Classification::Unreadable);
        for path in paths.intermediates() {
            assert!(!path.exists(), "{} should have been swept", path.display());
        }
    }

    #[test]
    fn committed_writes_left_in_a_wal_survive_the_next_launch() {
        // A WAL beside clips.db is committed data a process kill is entitled to
        // leave, and sweeping it would discard exactly the writes ADR-0009
        // guarantees survive. Leaking the connection without closing it is the
        // closest a unit test gets to a kill: nothing is checkpointed, so the
        // rows exist only in the sidecar when the second launch opens the store.
        let parent = dir();
        let paths = paths_in(&parent);
        let first = recover(&paths);
        let open = match first.connection {
            Some(open) => open,
            None => panic!("the first run should hold a connection"),
        };
        for label in ["first", "second"] {
            if let Err(e) = clips::insert(&open, label, "a value", Colour::DEFAULT) {
                panic!("the insert should succeed: {e}");
            }
        }
        assert!(paths.db_wal().is_file(), "WAL mode should write a sidecar");
        std::mem::forget(open);

        let second = recover(&paths);
        let reopened = match second.connection {
            Some(open) => open,
            None => panic!("the second run should hold a connection"),
        };
        let labels: Vec<String> = match clips::list(&reopened) {
            Ok(rows) => rows.into_iter().map(|c| c.label).collect(),
            Err(e) => panic!("the list should succeed: {e}"),
        };
        assert_eq!(labels, vec!["first", "second"]);
    }

    #[test]
    fn a_store_from_a_newer_build_is_rejected_rather_than_misread() {
        use crate::error::VersionComponent;

        let parent = dir();
        let paths = paths_in(&parent);
        {
            let first = recover(&paths);
            let open = match first.connection {
                Some(open) => open,
                None => panic!("the first run should hold a connection"),
            };
            if let Err(e) = open.pragma_update(None, "user_version", 99) {
                panic!("the fixture should be writable: {e}");
            }
            connection::checkpoint_and_close(open);
        }

        let second = recover(&paths);
        assert_eq!(
            second.fault,
            Some(ClipError::UnsupportedVersion {
                component: VersionComponent::Schema,
                found: 99,
                supported: schema::SCHEMA_VERSION,
            })
        );
        assert!(
            second.connection.is_none(),
            "a store we cannot read stays shut"
        );
    }

    #[test]
    fn a_corrupt_store_is_reported_as_crypto_corrupt_and_not_recreated() {
        use crate::error::CryptoReason;

        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // The SQLite magic — so it classifies as plaintext — over rubbish.
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend_from_slice(&[0u8; 512]);
        write(&paths.db(), &bytes);

        let recovered = recover(&paths);
        assert_eq!(recovered.classification, Classification::Plaintext);
        assert_eq!(
            recovered.fault,
            Some(ClipError::Crypto {
                reason: CryptoReason::Corrupt
            })
        );
        let after = match fs::read(paths.db()) {
            Ok(after) => after,
            Err(e) => panic!("the fixture should still be there: {e}"),
        };
        assert_eq!(after.len(), bytes.len(), "a corrupt store is not replaced");
    }

    #[test]
    fn a_pre_refactor_db_file_is_neither_read_nor_modified() {
        // ADR-0003: the old store is at %LOCALAPPDATA%\FastClip\db, a different
        // directory. Recovery touches nothing outside its own.
        let parent = dir();
        let legacy_dir = parent.path().join("FastClip");
        if let Err(e) = fs::create_dir_all(&legacy_dir) {
            panic!("could not create the fixture directory: {e}");
        }
        let legacy = legacy_dir.join("db");
        let contents = br#"{"data":{"8a1f":{"id":"8a1f","icon":"x","visible":true}}}"#;
        write(&legacy, contents);

        let paths = paths_in(&parent);
        let recovered = recover(&paths);
        let open = match recovered.connection {
            Some(open) => open,
            None => panic!("a first run should hold a connection"),
        };

        assert_eq!(clips::list(&open).map(|c| c.len()), Ok(0));
        assert!(recovered.fault.is_none());
        match fs::read(&legacy) {
            Ok(after) => assert_eq!(after, contents, "the pre-refactor store must be untouched"),
            Err(e) => panic!("the pre-refactor store should still be there: {e}"),
        }
    }
}
