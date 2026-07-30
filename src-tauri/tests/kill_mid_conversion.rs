//! **The kill-point table, row by row** (`storage.md` § Failures after the
//! commit point are absorbed, not reported).
//!
//! A conversion replaces the user's whole store, and the one failure it cannot
//! clean up after is the process not returning. Six kill points are enumerated
//! in that table with what each leaves on disk and what recovers it; this file
//! is those six rows, asserted.
//!
//! **The residues are constructed rather than produced by a real kill, and that
//! is deliberate.** Aiming an abort at a chosen step needs a hook in production
//! code, which would be a test affordance on the most dangerous path in the
//! application. The table does not specify timings, it specifies *states* — "a
//! plaintext `clips.db`, no sidecars, a stray `keyfile`, an encrypted
//! `clips.db.new`" — so building each named state in a fresh process and running
//! startup recovery over it tests exactly what the table promises. Nothing in
//! these tests has a destructor that could tidy up: the residue is written by
//! one process and read by the next `Store::open`, which is what a relaunch is.
//!
//! `tests/kill_mid_write.rs` really does abort a process, because what it checks
//! — SQLite's own journal — is a property of the kill itself. What this file
//! checks is the sweep, and the sweep cannot tell how the debris got there.
//!
//! **The fourth row is why the sweep is mandatory rather than housekeeping.** An
//! interrupted `disable_encryption` leaves a complete plaintext copy of every
//! label and value beside a store the user still believes is encrypted. Without
//! the sweep it stays there indefinitely, in breach of acceptance criteria 5
//! and 6.

use std::fs;
use std::path::Path;

use fast_clip_lib::colour::Colour;
use fast_clip_lib::crypto::{keyfile, Dek, KeyMaterial, Pin};
use fast_clip_lib::storage::classify::{classify, Classification};
use fast_clip_lib::storage::{clips, connection, schema, Store, StorePaths};

/// Distinctive enough that finding it in a file is unambiguous.
const SECRET: &str = "kill-mid-conversion-7f2ad3-plaintext-must-not-survive";

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

/// Create a store at `path`, keyed or not, holding three clips.
fn make_store(path: &Path, key: Option<&Dek>) {
    let connection = match key {
        Some(dek) => connection::open_encrypted(path, dek),
        None => connection::open(path),
    };
    let connection = match connection {
        Ok(connection) => connection,
        Err(e) => panic!("the fixture store should open: {e}"),
    };
    if let Err(e) = schema::create(&connection) {
        panic!("the fixture schema should create: {e}");
    }
    for n in 0..3 {
        let inserted = clips::insert(
            &connection,
            &format!("clip {n}"),
            &format!("{SECRET}-{n}"),
            Colour::DEFAULT,
        );
        if let Err(e) = inserted {
            panic!("the fixture clip should insert: {e}");
        }
    }
    connection::checkpoint_and_close(connection);
}

/// Write a `keyfile` for `dek`, as an interrupted enable would have left.
fn write_keyfile(paths: &StorePaths, dek: &Dek) {
    let pin = match Pin::parse("pin", Some("135790".to_owned())) {
        Ok(pin) => pin,
        Err(e) => panic!("the fixture PIN should parse: {e}"),
    };
    let material = match KeyMaterial::create(&pin, dek) {
        Ok(material) => material,
        Err(e) => panic!("the fixture key material should build: {e}"),
    };
    if let Err(e) = keyfile::write(paths, &material) {
        panic!("the fixture key material should write: {e}");
    }
}

/// Leave sidecars beside `clips.db`, as a store open at the moment of the kill
/// would have.
fn leave_sidecars(paths: &StorePaths) {
    for sidecar in [paths.db_wal(), paths.db_shm()] {
        if let Err(e) = fs::write(&sidecar, b"leftover sidecar bytes") {
            panic!("the fixture sidecar should write: {e}");
        }
    }
}

fn delete_sidecars(paths: &StorePaths) {
    for sidecar in [paths.db_wal(), paths.db_shm()] {
        let _ = fs::remove_file(sidecar);
    }
}

/// **No file under the store directory holds a clip value in the clear.**
///
/// A sweep of the whole directory rather than named files, so a file some later
/// package adds is covered without anyone remembering to add it here.
fn assert_no_plaintext_anywhere(paths: &StorePaths) {
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
        assert!(
            !bytes
                .windows(SECRET.len())
                .any(|window| window == SECRET.as_bytes()),
            "{} holds a clip value in the clear",
            entry.path().display()
        );
    }
}

fn assert_no_intermediates(paths: &StorePaths) {
    for intermediate in paths.intermediates() {
        assert!(
            !intermediate.exists(),
            "{} survived startup recovery",
            intermediate.display()
        );
    }
}

fn labels(store: &Store) -> Vec<String> {
    match store.with_unlocked_store(|open| clips::list(open)) {
        Ok(rows) => rows.into_iter().map(|row| row.label).collect(),
        Err(e) => panic!("the clips should list: {e}"),
    }
}

fn dek_of(byte: u8) -> Dek {
    Dek::from_bytes([byte; 32])
}

// ---- Enable ----

/// **Row 1.** Killed before the sidecar delete: the plaintext store is intact
/// with its own legitimate sidecars, and an encrypted `clips.db.new` and a stray
/// `keyfile` are beside it.
///
/// The store is plaintext, which is what it was. Its own sidecars are not
/// intermediates and must be left alone — deleting a live WAL discards committed
/// writes (ADR-0009).
#[test]
fn enable_killed_before_the_sidecar_delete_recovers_the_plaintext_store() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), None);
    leave_sidecars(&paths);
    make_store(&paths.db_new(), Some(&dek));
    write_keyfile(&paths, &dek);

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Plaintext);
    assert!(!store.is_locked());
    assert_eq!(store.startup_fault(), None);
    assert_eq!(labels(&store), vec!["clip 0", "clip 1", "clip 2"]);

    assert_no_intermediates(&paths);
    assert!(
        !paths.keyfile().exists(),
        "a keyfile beside a plaintext store decrypts nothing and is removed"
    );
    store.shutdown();
}

/// **Row 2.** Killed between the sidecar delete and the rename. The missing
/// sidecars cost nothing — they were checkpointed into the database first.
#[test]
fn enable_killed_between_the_sidecar_delete_and_the_rename_recovers_every_clip() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), None);
    delete_sidecars(&paths);
    make_store(&paths.db_new(), Some(&dek));
    write_keyfile(&paths, &dek);

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Plaintext);
    assert_eq!(labels(&store), vec!["clip 0", "clip 1", "clip 2"]);
    assert_no_intermediates(&paths);
    assert!(!paths.keyfile().exists());
    store.shutdown();
}

/// **Row 3.** Killed after the rename, including before the reopen. The store is
/// encrypted and its `keyfile` **must survive** — deleting it here would destroy
/// the only key to a store that is now encrypted.
///
/// This is the row the four-state classification exists for. Treating an
/// encrypted store as absent, or as anything but `encrypted`, is what would
/// take the `keyfile` with it.
#[test]
fn enable_killed_after_the_rename_keeps_the_encrypted_store_and_its_key() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), Some(&dek));
    delete_sidecars(&paths);
    write_keyfile(&paths, &dek);
    // A stray encrypted sidecar the close may have left.
    let stray = paths.dir().join("clips.db.new-wal");
    if let Err(e) = fs::write(&stray, b"stray encrypted sidecar") {
        panic!("the fixture should write: {e}");
    }

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Encrypted);
    assert!(store.is_locked(), "an encrypted store starts locked");
    assert_eq!(
        store.startup_fault(),
        None,
        "being encrypted is not a fault"
    );
    assert!(
        paths.keyfile().exists(),
        "the only key to an encrypted store must never be swept"
    );
    assert_no_intermediates(&paths);
    assert_no_plaintext_anywhere(&paths);
    store.shutdown();
}

// ---- Disable ----

/// **Row 4, and the reason the sweep is mandatory.**
///
/// An interrupted `disable_encryption` leaves a complete plaintext `clips.db.new`
/// holding every label and value, beside an encrypted store the user still
/// believes is encrypted. Startup recovery step 2 deletes it — **before** the
/// classification is read and ungated, so no later branch can suppress it.
#[test]
fn disable_killed_before_the_sidecar_delete_removes_the_plaintext_copy() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), Some(&dek));
    leave_sidecars(&paths);
    write_keyfile(&paths, &dek);
    // The dangerous file: every clip, in the clear.
    make_store(&paths.db_new(), None);

    // The fixture really is a disclosure before recovery runs, or this test
    // would be asserting the absence of something that was never there.
    let before = match fs::read(paths.db_new()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the fixture should be readable: {e}"),
    };
    assert!(
        before
            .windows(SECRET.len())
            .any(|window| window == SECRET.as_bytes()),
        "the fixture is supposed to be a plaintext copy"
    );

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Encrypted);
    assert!(store.is_locked());
    assert!(
        paths.keyfile().exists(),
        "the store is still encrypted, so its key must stay"
    );
    assert_no_intermediates(&paths);
    assert_no_plaintext_anywhere(&paths);
    store.shutdown();
}

/// **Row 5.** The same, without the sidecars.
#[test]
fn disable_killed_between_the_sidecar_delete_and_the_rename_removes_the_plaintext_copy() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), Some(&dek));
    delete_sidecars(&paths);
    write_keyfile(&paths, &dek);
    make_store(&paths.db_new(), None);
    delete_sidecars(&StorePaths::at(paths.dir()));

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Encrypted);
    assert!(paths.keyfile().exists());
    assert_no_intermediates(&paths);
    assert_no_plaintext_anywhere(&paths);
    store.shutdown();
}

/// **Row 6.** Killed after the rename but before the `keyfile` delete. The store
/// is plaintext and the stray `keyfile` is removed at step 5 — it wraps a DEK
/// for a database that no longer exists.
#[test]
fn disable_killed_before_the_keyfile_delete_recovers_a_plaintext_store() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), None);
    delete_sidecars(&paths);
    write_keyfile(&paths, &dek);

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Plaintext);
    assert!(!store.is_locked());
    assert_eq!(labels(&store), vec!["clip 0", "clip 1", "clip 2"]);
    assert!(
        !paths.keyfile().exists(),
        "a keyfile beside a plaintext store is a leftover and is removed"
    );
    store.shutdown();
}

// ---- the property the whole table exists to protect ----

/// **`keyfile.new` is swept too**, whichever conversion left it. A half-written
/// key file is never consulted by any reader, and leaving one would put a file
/// nobody can interpret beside the store.
#[test]
fn a_half_written_keyfile_is_swept_whatever_the_store_is() {
    for encrypted in [false, true] {
        let parent = dir();
        let paths = paths_in(&parent);
        let dek = dek_of(0x2b);

        if encrypted {
            make_store(&paths.db(), Some(&dek));
            write_keyfile(&paths, &dek);
        } else {
            make_store(&paths.db(), None);
        }
        if let Err(e) = fs::write(paths.keyfile_new(), b"half a DPAPI blob") {
            panic!("the fixture should write: {e}");
        }

        let store = Store::open(paths.clone());
        assert!(
            !paths.keyfile_new().exists(),
            "keyfile.new survived (encrypted: {encrypted})"
        );
        // And the real key material was not taken with it.
        assert_eq!(paths.keyfile().exists(), encrypted);
        store.shutdown();
    }
}

/// **An `unreadable` store is the one case where nothing is deleted.**
///
/// Steps 5 and 6 are both gated on a *positive* classification, so a store whose
/// header could not be read keeps its `keyfile` and gets no fresh database. The
/// sweep at step 2 still runs, because an intermediate is debris by definition
/// and `clips.db` is intact beside it at every kill instant.
#[test]
fn an_unreadable_store_keeps_its_key_material_and_is_never_recreated() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    write_keyfile(&paths, &dek);
    // A directory where the database should be: unreadable, not absent.
    if let Err(e) = fs::create_dir(paths.db()) {
        panic!("the fixture should create: {e}");
    }
    if let Err(e) = fs::write(paths.db_new(), b"debris") {
        panic!("the fixture should write: {e}");
    }

    let store = Store::open(paths.clone());

    assert_eq!(store.classification(), Classification::Unreadable);
    assert_eq!(
        store.startup_fault(),
        Some(fast_clip_lib::ClipError::Storage)
    );
    assert!(
        paths.keyfile().exists(),
        "an unreadable store may be an encrypted one; deleting its key destroys it"
    );
    assert!(
        paths.db().is_dir(),
        "nothing may be created over an unreadable store"
    );
    assert!(
        !paths.db_new().exists(),
        "the sweep is ungated and runs before the classification"
    );
}

/// The classifier agrees with what these fixtures claim to be, so a test that
/// passes because the fixture was misbuilt is not possible.
#[test]
fn the_fixtures_are_what_they_say_they_are() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = dek_of(0x2b);

    make_store(&paths.db(), None);
    assert_eq!(classify(&paths.db()), Classification::Plaintext);

    make_store(&paths.db_new(), Some(&dek));
    assert_eq!(classify(&paths.db_new()), Classification::Encrypted);
}
