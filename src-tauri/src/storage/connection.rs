//! Opening and closing the one connection to the live store.
//!
//! At most one connection exists, behind a mutex
//! (`docs/src/architecture/storage.md` § Connections). It is opened at startup
//! recovery step 7, at `unlock`, and at a conversion's reopen — and nowhere
//! else. No command opens the store because it found it closed.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::ClipError;
use crate::storage::{open_error, storage_error};

/// The wait a second FastClip process is worth before the user is told the
/// store failed. SQLite locks between processes and this build has no
/// single-instance guard.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// Open a database and apply the connection pragmas.
///
/// **WP-07:** `PRAGMA key` must be the first statement on an encrypted
/// connection, before `journal_mode`. Chaining anything ahead of it is the
/// documented way to get `file is not a database`, so the key belongs at the
/// top of this function and not at a call site.
pub fn open(path: &Path) -> Result<Connection, ClipError> {
    let connection =
        Connection::open(path).map_err(|e| open_error("the store could not be opened", &e))?;

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
