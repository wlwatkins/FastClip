//! **The SQLCipher primitives WP-07 rests on, proved in this crate.**
//!
//! ADR-0005's spike proved the link and the encryption cycle, but it did so in a
//! separate crate (`spikes/sqlcipher-spike`) and with a passphrase key. This
//! application uses a **raw** 256-bit key, converts between plaintext and
//! encrypted with `sqlcipher_export()`, and depends on an encrypted file *not*
//! looking like a SQLite database to
//! [`fast_clip_lib::storage::classify`]. None of that was exercised where it
//! ships.
//!
//! This file is evidence rather than a unit test, and it is written before the
//! commands that depend on it — the migration before the feature. Every
//! assertion here is a property some part of WP-07 would otherwise assume:
//!
//! | Property | Depended on by |
//! | -------- | -------------- |
//! | A raw 32-byte key creates and reopens an encrypted database | `enable_encryption`, `unlock` |
//! | The wrong key fails, and fails as a *SQLite* error rather than by returning wrong rows | `unlock`'s `bad_pin` path never reaching the database with a bad DEK |
//! | An encrypted file does not begin with the SQLite magic | `classify`, and therefore every deletion in startup recovery |
//! | `PRAGMA key` must precede every other statement | `connection::open`, whose doc comment already says so |
//! | `sqlcipher_export()` round-trips both ways | `enable_encryption` and `disable_encryption` |
//! | `sqlcipher_export()` does **not** carry `user_version` | storage.md's "write it explicitly" rule |
//!
//! It stays after WP-07 lands. `rusqlite` is pinned to 0.32 by ADR-0005, and if
//! anyone bumps it, this is what says whether the cipher still behaves.

use std::path::Path;

use fast_clip_lib::storage::classify::{classify, Classification};
use rusqlite::Connection;

/// A raw key, as SQLCipher's `x'…'` literal. Not a passphrase: 64 hex
/// characters make SQLCipher use the bytes directly and skip its own KDF, which
/// is what ADR-0004 requires — the entropy is the DEK, and Argon2id is applied
/// to the PIN elsewhere, not here.
fn raw_key(dek: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in dek {
        use std::fmt::Write;
        if write!(hex, "{byte:02x}").is_err() {
            panic!("writing to a String cannot fail");
        }
    }
    format!("x'{hex}'")
}

fn dir() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    }
}

/// Open a database and apply the key **first**, before any other statement.
fn open_keyed(path: &Path, dek: Option<&[u8; 32]>) -> Connection {
    let connection = match Connection::open(path) {
        Ok(connection) => connection,
        Err(e) => panic!("the database should open: {e}"),
    };
    if let Some(dek) = dek {
        // `execute_batch`, not `pragma_update`: both embed the value in SQL
        // text, and this one keeps that text in a variable this file controls.
        // Production code must never log the error from this statement — see
        // `the_key_pragma_is_the_first_statement_or_the_database_is_not_one`.
        if let Err(e) = connection.execute_batch(&format!("PRAGMA key = \"{}\";", raw_key(dek))) {
            panic!("the key pragma should apply: {e}");
        }
    }
    connection
}

fn seed(connection: &Connection, label: &str) {
    let sql = "CREATE TABLE IF NOT EXISTS clips (id INTEGER PRIMARY KEY, label TEXT NOT NULL);";
    if let Err(e) = connection.execute_batch(sql) {
        panic!("the fixture schema should be creatable: {e}");
    }
    if let Err(e) = connection.execute("INSERT INTO clips (label) VALUES (?1)", [label]) {
        panic!("the fixture row should insert: {e}");
    }
}

fn labels(connection: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = connection.prepare("SELECT label FROM clips ORDER BY id")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn user_version(connection: &Connection) -> i64 {
    match connection.query_row("PRAGMA user_version", [], |row| row.get(0)) {
        Ok(version) => version,
        Err(e) => panic!("user_version should be readable: {e}"),
    }
}

fn close(connection: Connection) {
    if let Err((_, e)) = connection.close() {
        panic!("the connection should close: {e}");
    }
}

/// The cycle ADR-0005 required, with a **raw** key rather than a passphrase.
#[test]
fn a_raw_key_encrypts_a_database_that_reopens_and_reads_back() {
    let dir = dir();
    let path = dir.path().join("clips.db");
    let dek = [0x2bu8; 32];

    let connection = open_keyed(&path, Some(&dek));
    seed(&connection, "a label");
    close(connection);

    let reopened = open_keyed(&path, Some(&dek));
    assert_eq!(labels(&reopened), Ok(vec!["a label".to_string()]));
    close(reopened);
}

/// **The other half of the cycle, and the more important half.** A database
/// that reads back with the wrong key would mean it was never encrypted.
#[test]
fn the_wrong_key_cannot_read_the_database_and_neither_can_no_key() {
    let dir = dir();
    let path = dir.path().join("clips.db");
    let dek = [0x2bu8; 32];

    let connection = open_keyed(&path, Some(&dek));
    seed(&connection, "a secret label");
    close(connection);

    let mut wrong = dek;
    wrong[0] ^= 0xff;
    let with_wrong_key = open_keyed(&path, Some(&wrong));
    if let Ok(rows) = labels(&with_wrong_key) {
        panic!("the wrong key read {rows:?} out of an encrypted store");
    }

    // And with no key at all, which is what an attacker holding only the file
    // has: `sqlite3 clips.db` and nothing else.
    let with_no_key = open_keyed(&path, None);
    if let Ok(rows) = labels(&with_no_key) {
        panic!("an unkeyed connection read {rows:?} out of an encrypted store");
    }
}

/// **Acceptance criterion 5, at the byte level.** The classifier reads the
/// first sixteen bytes and calls anything that is not the SQLite magic
/// `Encrypted`. That is the whole basis of `storage.md`'s four-state
/// classification, and every deletion in startup recovery is gated on it.
///
/// It holds because a raw key leaves `cipher_plaintext_header_size` at 0, so the
/// header is encrypted along with everything else. A future SQLCipher option
/// that exposed a plaintext header would silently turn every encrypted store
/// into a `plaintext` classification — and then startup recovery would delete
/// its `keyfile`.
#[test]
fn an_encrypted_database_does_not_look_like_a_sqlite_file_to_the_classifier() {
    let dir = dir();
    let encrypted = dir.path().join("encrypted.db");
    let plain = dir.path().join("plain.db");
    let dek = [0x2bu8; 32];

    let connection = open_keyed(&encrypted, Some(&dek));
    seed(&connection, "a label");
    close(connection);

    let connection = open_keyed(&plain, None);
    seed(&connection, "a label");
    close(connection);

    assert_eq!(classify(&encrypted), Classification::Encrypted);
    assert_eq!(classify(&plain), Classification::Plaintext);

    // Stated as bytes as well as through the classifier, because the classifier
    // could be changed and this property would still have to hold.
    let header = match std::fs::read(&encrypted) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the encrypted file should be readable as bytes: {e}"),
    };
    assert!(
        !header.starts_with(b"SQLite format 3\0"),
        "an encrypted store must not carry a plaintext SQLite header"
    );
    assert!(
        !header.windows(14).any(|w| w == b"a secret label"),
        "no plaintext from the database may appear in the file"
    );
}

/// The ordering rule `connection::open`'s doc comment states, proved rather than
/// cited. Chaining `journal_mode` ahead of `PRAGMA key` is the documented way to
/// get `file is not a database`, and it is the defect ADR-0005 rejected `sea-orm`
/// over.
#[test]
fn the_key_pragma_is_the_first_statement_or_the_database_is_not_one() {
    let dir = dir();
    let path = dir.path().join("clips.db");
    let dek = [0x2bu8; 32];

    let connection = open_keyed(&path, Some(&dek));
    seed(&connection, "a label");
    close(connection);

    // Wrong order: a statement that touches the file before the key is applied.
    let out_of_order = match Connection::open(&path) {
        Ok(connection) => connection,
        Err(e) => panic!("the database should open: {e}"),
    };
    let early: Result<String, _> =
        out_of_order.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0));
    let keyed = out_of_order.execute_batch(&format!("PRAGMA key = \"{}\";", raw_key(&dek)));

    assert!(
        early.is_err() || keyed.is_err() || labels(&out_of_order).is_err(),
        "keying after a statement that touched the file appeared to work; \
         connection::open's ordering rule would then be untested folklore"
    );
}

/// **The conversion `enable_encryption` performs**, reduced to its two
/// statements. Proving it here means the command is assembled from a mechanism
/// that already works, rather than debugged against a user's store.
#[test]
fn sqlcipher_export_converts_plaintext_to_encrypted_and_carries_every_row() {
    let dir = dir();
    let source = dir.path().join("clips.db");
    let target = dir.path().join("clips.db.new");
    let dek = [0x2bu8; 32];

    let plain = open_keyed(&source, None);
    seed(&plain, "first");
    seed(&plain, "second");
    if let Err(e) = plain.pragma_update(None, "user_version", 1) {
        panic!("the fixture version should be writable: {e}");
    }

    // ATTACH takes bound parameters, which keeps the path out of the SQL text.
    // The key cannot be bound — PRAGMA and the ATTACH KEY clause are parsed, not
    // executed — so it is formatted in, and that is why its error must never be
    // logged.
    let attach = format!("ATTACH DATABASE ?1 AS clips_new KEY \"{}\";", raw_key(&dek));
    if let Err(e) = plain.execute(&attach, [target.to_string_lossy()]) {
        panic!("the encrypted target should attach: {e}");
    }
    if let Err(e) = plain.query_row("SELECT sqlcipher_export('clips_new')", [], |_| Ok(())) {
        panic!("the export should run: {e}");
    }

    // **`user_version` does not cross.** This is the assertion storage.md's
    // "write it explicitly" rule exists for; if it ever starts crossing, the
    // explicit write becomes redundant rather than load-bearing, and someone
    // will delete it.
    let carried: i64 = match plain.query_row("PRAGMA clips_new.user_version", [], |row| row.get(0))
    {
        Ok(version) => version,
        Err(e) => panic!("the target version should be readable: {e}"),
    };
    assert_eq!(
        carried, 0,
        "sqlcipher_export carried user_version across, which storage.md says it does not"
    );

    if let Err(e) =
        plain.execute_batch("PRAGMA clips_new.user_version = 1; DETACH DATABASE clips_new;")
    {
        panic!("the version write and detach should succeed: {e}");
    }
    close(plain);

    // The target is a usable encrypted store holding every row and the version.
    assert_eq!(classify(&target), Classification::Encrypted);
    let converted = open_keyed(&target, Some(&dek));
    assert_eq!(
        labels(&converted),
        Ok(vec!["first".to_string(), "second".to_string()])
    );
    assert_eq!(user_version(&converted), 1);
    close(converted);
}

/// The same mechanism with the roles swapped — `disable_encryption`. Written as
/// its own test rather than a parameter, because the two differ in which side
/// carries the key and that is the part worth reading.
#[test]
fn sqlcipher_export_converts_encrypted_back_to_plaintext() {
    let dir = dir();
    let source = dir.path().join("clips.db");
    let target = dir.path().join("clips.db.new");
    let dek = [0x2bu8; 32];

    let encrypted = open_keyed(&source, Some(&dek));
    seed(&encrypted, "first");
    seed(&encrypted, "second");
    if let Err(e) = encrypted.pragma_update(None, "user_version", 1) {
        panic!("the fixture version should be writable: {e}");
    }

    // `KEY ''` is how SQLCipher is told the target is plaintext.
    if let Err(e) = encrypted.execute(
        "ATTACH DATABASE ?1 AS plain KEY '';",
        [target.to_string_lossy()],
    ) {
        panic!("the plaintext target should attach: {e}");
    }
    if let Err(e) = encrypted.query_row("SELECT sqlcipher_export('plain')", [], |_| Ok(())) {
        panic!("the export should run: {e}");
    }
    if let Err(e) = encrypted.execute_batch("PRAGMA plain.user_version = 1; DETACH DATABASE plain;")
    {
        panic!("the version write and detach should succeed: {e}");
    }
    close(encrypted);

    assert_eq!(classify(&target), Classification::Plaintext);
    let converted = open_keyed(&target, None);
    assert_eq!(
        labels(&converted),
        Ok(vec!["first".to_string(), "second".to_string()])
    );
    assert_eq!(user_version(&converted), 1);
    close(converted);
}

/// **A file with a live SQLite handle cannot be deleted on Windows**, which is
/// review 003's finding F2: the conversion's abort path deletes `clips.db.new`,
/// and `storage.md` does not require the connection step 3 opened to be closed
/// first. Asserted rather than argued, because the whole finding rests on it.
///
/// If this ever starts passing the other way, F2 is void and the sentence
/// `storage.md` is missing does not need writing.
#[test]
fn an_open_database_cannot_be_deleted_which_is_why_the_abort_must_close_first() {
    let dir = dir();
    let path = dir.path().join("clips.db.new");
    let dek = [0x2bu8; 32];

    let connection = open_keyed(&path, Some(&dek));
    seed(&connection, "a label");

    assert!(
        std::fs::remove_file(&path).is_err(),
        "the abort path could delete an open database, so F2 does not apply here"
    );

    close(connection);
    assert!(
        std::fs::remove_file(&path).is_ok(),
        "and it can be deleted once the handle is gone"
    );
}
