//! Opening and closing the one connection to the live store.
//!
//! At most one connection exists, behind a mutex
//! (`docs/src/architecture/storage.md` § Connections). It is opened at startup
//! recovery step 7, at `unlock`, and at a conversion's reopen — and nowhere
//! else. No command opens the store because it found it closed.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::crypto::Dek;
use crate::error::ClipError;
use crate::storage::{open_error, storage_error};

/// The wait a second FastClip process is worth before the user is told the
/// store failed. SQLite locks between processes and this build has no
/// single-instance guard.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// The DEK as SQLCipher's raw-key literal, `x'<64 hex>'`.
///
/// **A raw key bypasses SQLCipher's own KDF**, which is what `storage.md`
/// requires: the entropy is the DEK, and Argon2id is applied to the PIN
/// elsewhere. Sixty-four hex characters is what makes SQLCipher read it as key
/// bytes rather than a passphrase.
///
/// `Zeroizing`, because this string *is* the key. Shared with
/// [`crate::storage::convert`], whose `ATTACH … KEY` clause is the only other
/// statement that carries it — and which is bound by the same rule about never
/// formatting its error.
pub(crate) fn raw_key_literal(dek: &Dek) -> Zeroizing<String> {
    use std::fmt::Write;

    let mut literal = Zeroizing::new(String::with_capacity(68));
    // Cannot fail: writing to a `String`. Absorbed rather than unwrapped, and a
    // truncated literal would fail at the statement that uses it.
    let _ = literal.write_str("x'");
    for byte in dek.expose() {
        let _ = write!(literal, "{byte:02x}");
    }
    let _ = literal.write_str("'");
    literal
}

/// The `PRAGMA key` statement for a raw 256-bit DEK.
fn key_pragma(dek: &Dek) -> Zeroizing<String> {
    Zeroizing::new(format!("PRAGMA key = \"{}\";", &*raw_key_literal(dek)))
}

/// Apply the key to a connection, **without ever rendering the error**.
///
/// `storage.md` § A keying statement's error is never formatted. `PRAGMA key`
/// and the `KEY` clause of `ATTACH` cannot take a bound parameter, so the DEK is
/// embedded in the SQL text — and `rusqlite::Error::SqlInputError`'s `Display`
/// is `"{msg} in {sql} at offset {offset}"`, which renders that SQL. The
/// idiomatic `log::error!("…: {error}")` used correctly everywhere else in this
/// crate would therefore write **the DEK in hex** into
/// `~/.fast-clip/fast-clip.log`, in plaintext, beside the database it decrypts
/// and readable while the store is locked.
///
/// So the error value is discarded rather than bound. There is nothing to learn
/// from it that is worth that: this statement does not touch the file, so it
/// fails only if the statement itself was malformed, and the first statement
/// that *does* touch the file reports a wrong key as `crypto { corrupt }` with
/// an error that carries no key material.
///
/// **The exact shape of the hazard, measured rather than assumed.** It was
/// demonstrated by temporarily logging a formatted error here and watching
/// `tests/log_content.rs` catch the key in hex:
///
/// - It needs the **key-carrying statement itself** to be the one that fails to
///   prepare. A syntax error in a *later* statement of the same `execute_batch`
///   does **not** leak, because rusqlite prepares statements one at a time and
///   the error carries only the failing one.
/// - Given that, all three entry points leak: `execute_batch`, `prepare` and
///   `execute` each produce an error whose `Display` renders the statement.
///
/// The narrower shape does not make the rule optional. It makes it *harder to
/// spot*, because the leak needs a malformed key statement — which is exactly
/// what a future edit to [`key_pragma`] could introduce, on the one code path
/// where the error would then be printed.
///
/// **Do not "improve" this by logging the error.** `tests/log_content.rs` greps
/// a real log file for the DEK and will catch it, which is the standing guard —
/// but the reason it is needed is that nothing about the code would look wrong.
fn apply_key(connection: &Connection, dek: &Dek) -> Result<(), ClipError> {
    match connection.execute_batch(&key_pragma(dek)) {
        Ok(()) => Ok(()),
        Err(_discarded) => {
            log::error!("the key could not be applied to the store connection");
            Err(ClipError::Storage)
        }
    }
}

/// Open a plaintext database and apply the connection pragmas.
pub fn open(path: &Path) -> Result<Connection, ClipError> {
    open_inner(path, None)
}

/// Open a SQLCipher database with the DEK.
///
/// Used by `unlock`, by a conversion's verification and reopen, and nowhere
/// else. A wrong key is not detected here — SQLCipher accepts any 32 bytes and
/// fails on the first statement that reads a page, which is the `journal_mode`
/// call below and which reports `crypto { corrupt }`.
pub fn open_encrypted(path: &Path, dek: &Dek) -> Result<Connection, ClipError> {
    open_inner(path, Some(dek))
}

fn open_inner(path: &Path, dek: Option<&Dek>) -> Result<Connection, ClipError> {
    let connection =
        Connection::open(path).map_err(|e| open_error("the store could not be opened", &e))?;

    // **The key is the first statement, before `busy_timeout` and before
    // `journal_mode`.** Chaining anything ahead of it is the documented way to
    // get `file is not a database` — the defect ADR-0005 rejected `sea-orm`
    // over — and `tests/sqlcipher.rs` proves it rather than citing it.
    if let Some(dek) = dek {
        apply_key(&connection, dek)?;
    }

    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|e| storage_error("busy_timeout could not be set", &e))?;

    // `Connection::open` does not touch the file, so this is the first
    // statement that can discover the file is not a database.
    let mode: String = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(|e| open_error("journal_mode could not be set to WAL", &e))?;
    if !mode.eq_ignore_ascii_case("wal") {
        // A read-only file or directory refuses WAL and reports the old mode
        // rather than an error. Continuing would give up the crash safety
        // ADR-0009 depends on, silently.
        log::error!("journal_mode is {mode} rather than wal; the store is not crash-safe");
        return Err(ClipError::Storage);
    }

    // ADR-0009: a process kill cannot lose a committed write; a power cut can
    // lose the last few. Read that ADR before changing this.
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| storage_error("synchronous could not be set to NORMAL", &e))?;

    Ok(connection)
}

/// Checkpoint the WAL into the database and close.
///
/// Used at shutdown, and by `lock` and the conversions in later packages. A
/// failure is logged and absorbed: refusing to close because a disk operation
/// failed helps nobody, and the committed data is already in the WAL either way.
pub fn checkpoint_and_close(connection: Connection) {
    // `wal_checkpoint` returns a row (busy, log, checkpointed), so it is a query
    // rather than an update. A non-zero `busy` means another connection held the
    // database and the WAL survives, which costs nothing but the sidecar.
    match connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(0) => {}
        Ok(busy) => log::warn!("the WAL checkpoint did not complete (busy = {busy})"),
        Err(error) => log::warn!("the WAL could not be checkpointed before closing: {error}"),
    }
    if let Err((_, error)) = connection.close() {
        log::warn!("the store connection did not close cleanly: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    #[test]
    fn a_new_connection_is_in_wal_mode() {
        let dir = dir();
        let connection = match open(&dir.path().join("clips.db")) {
            Ok(connection) => connection,
            Err(e) => panic!("the database should open: {e}"),
        };
        let mode: String = match connection.query_row("PRAGMA journal_mode", [], |row| row.get(0)) {
            Ok(mode) => mode,
            Err(e) => panic!("journal_mode should be readable: {e}"),
        };
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn synchronous_is_normal() {
        let dir = dir();
        let connection = match open(&dir.path().join("clips.db")) {
            Ok(connection) => connection,
            Err(e) => panic!("the database should open: {e}"),
        };
        // 1 is NORMAL. ADR-0009.
        let synchronous: i64 =
            match connection.query_row("PRAGMA synchronous", [], |row| row.get(0)) {
                Ok(value) => value,
                Err(e) => panic!("synchronous should be readable: {e}"),
            };
        assert_eq!(synchronous, 1);
    }

    // ---- the keyed connection (WP-07) ----

    fn dek_of(byte: u8) -> Dek {
        Dek::from_bytes([byte; 32])
    }

    fn seed(connection: &Connection, value: &str) {
        let sql = "CREATE TABLE IF NOT EXISTS t (v TEXT);";
        if let Err(e) = connection.execute_batch(sql) {
            panic!("the fixture should be creatable: {e}");
        }
        if let Err(e) = connection.execute("INSERT INTO t (v) VALUES (?1)", [value]) {
            panic!("the fixture row should insert: {e}");
        }
    }

    fn read_back(connection: &Connection) -> Result<String, rusqlite::Error> {
        connection.query_row("SELECT v FROM t", [], |row| row.get(0))
    }

    /// An encrypted connection is a working one, and the pragmas the plaintext
    /// path applies are applied here too — the key going first must not cost
    /// WAL mode or `synchronous = NORMAL`.
    #[test]
    fn an_encrypted_connection_gets_the_same_pragmas_as_a_plaintext_one() {
        let dir = dir();
        let path = dir.path().join("clips.db");
        let connection = match open_encrypted(&path, &dek_of(0x2b)) {
            Ok(connection) => connection,
            Err(e) => panic!("the encrypted database should open: {e}"),
        };

        let mode: String = match connection.query_row("PRAGMA journal_mode", [], |row| row.get(0)) {
            Ok(mode) => mode,
            Err(e) => panic!("journal_mode should be readable: {e}"),
        };
        assert_eq!(mode.to_lowercase(), "wal");

        let synchronous: i64 =
            match connection.query_row("PRAGMA synchronous", [], |row| row.get(0)) {
                Ok(value) => value,
                Err(e) => panic!("synchronous should be readable: {e}"),
            };
        assert_eq!(synchronous, 1);
    }

    #[test]
    fn a_store_written_with_a_key_reads_back_with_the_same_key() {
        let dir = dir();
        let path = dir.path().join("clips.db");
        {
            let connection = match open_encrypted(&path, &dek_of(0x2b)) {
                Ok(connection) => connection,
                Err(e) => panic!("the encrypted database should open: {e}"),
            };
            seed(&connection, "a value");
            checkpoint_and_close(connection);
        }

        let reopened = match open_encrypted(&path, &dek_of(0x2b)) {
            Ok(connection) => connection,
            Err(e) => panic!("the encrypted database should reopen: {e}"),
        };
        assert_eq!(read_back(&reopened), Ok("a value".to_string()));
    }

    /// **The wrong key does not open the store**, and it reports the variant
    /// contract §2 names for it rather than `storage`.
    #[test]
    fn the_wrong_key_is_crypto_corrupt_and_the_plaintext_path_cannot_open_it_either() {
        use crate::error::CryptoReason;

        let dir = dir();
        let path = dir.path().join("clips.db");
        {
            let connection = match open_encrypted(&path, &dek_of(0x2b)) {
                Ok(connection) => connection,
                Err(e) => panic!("the encrypted database should open: {e}"),
            };
            seed(&connection, "a value");
            checkpoint_and_close(connection);
        }

        let corrupt = Err(ClipError::Crypto {
            reason: CryptoReason::Corrupt,
        });
        assert_eq!(open_encrypted(&path, &dek_of(0x2c)).map(drop), corrupt);
        assert_eq!(open(&path).map(drop), corrupt, "and with no key at all");
    }

    /// The raw-key form SQLCipher expects: `x'` then exactly 64 hex characters.
    /// A passphrase would put the DEK through SQLCipher's own KDF, which
    /// `storage.md` says it must bypass.
    #[test]
    fn the_key_pragma_is_the_raw_key_form_with_sixty_four_hex_characters() {
        let statement = key_pragma(&Dek::from_bytes([0xAB; 32]));
        assert_eq!(
            &*statement,
            &format!("PRAGMA key = \"x'{}'\";", "ab".repeat(32))
        );

        let hex: String = statement
            .chars()
            .skip_while(|c| *c != '\'')
            .skip(1)
            .take_while(|c| *c != '\'')
            .collect();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_file_that_is_not_a_database_is_crypto_corrupt_rather_than_storage() {
        use crate::error::CryptoReason;

        let dir = dir();
        let path = dir.path().join("clips.db");
        // The SQLite magic followed by rubbish: the header classifies as
        // plaintext, and the fault only appears when SQLite reads a page.
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend_from_slice(&[0u8; 512]);
        if let Err(e) = std::fs::write(&path, &bytes) {
            panic!("could not write the fixture: {e}");
        }

        match open(&path) {
            Err(ClipError::Crypto {
                reason: CryptoReason::Corrupt,
            }) => {}
            other => panic!("expected crypto/corrupt, got {other:?}"),
        }
    }

    #[test]
    fn the_bundled_sqlite_is_new_enough_for_strict_tables() {
        // STRICT requires SQLite 3.37. The schema drops to untyped columns
        // without it, and storage.md requires that to be reported rather than
        // absorbed, so it is asserted here instead.
        let version = rusqlite::version_number();
        assert!(
            version >= 3_037_000,
            "bundled SQLite is {} — STRICT tables need 3.37.0",
            rusqlite::version()
        );
    }
}
