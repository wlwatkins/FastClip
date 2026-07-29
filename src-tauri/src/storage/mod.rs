//! The clip store: SQLite under `~/.fast-clip/`, ordered, crash-safe and
//! schema-versioned.
//!
//! Crash safety is SQLite's journal, not a hand-rolled temp-file-and-rename
//! (ADR-0005, ADR-0009). The one thing this layer must never do is leave the
//! store half-written, so every multi-statement mutation runs inside one
//! `BEGIN IMMEDIATE` transaction and every renumber is an offset-then-write
//! pair that cannot transiently duplicate a `position`.

pub mod classify;
pub mod clips;
pub mod connection;
pub mod paths;
pub mod recovery;
pub mod schema;
mod store;

pub use classify::Classification;
pub use clips::ClipRow;
pub use paths::StorePaths;
pub use schema::SCHEMA_VERSION;
pub use store::Store;

use std::sync::{Mutex, MutexGuard};

use crate::error::{ClipError, CryptoReason};

/// Map a database failure to `storage`, logging the cause.
///
/// `context` is a static description of the statement, never a bound parameter:
/// no clip `label` or `value` may reach a log line at any level (ADR-0002).
pub(crate) fn storage_error(context: &str, error: &rusqlite::Error) -> ClipError {
    log::error!("{context}: {error}");
    ClipError::Storage
}

/// Map a failure met while *opening* a database.
///
/// A file that is not a database, or one SQLite reports as corrupt, is
/// `crypto { corrupt }` — the variant whose copy points the user at import.
/// Everything else at open time is `storage`, because sending "your store is
/// damaged, re-import it" to someone whose file was merely locked by another
/// process is how an intact store gets replaced.
pub(crate) fn open_error(context: &str, error: &rusqlite::Error) -> ClipError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::NotADatabase) | Some(rusqlite::ErrorCode::DatabaseCorrupt) => {
            log::error!("{context}: {error}");
            ClipError::Crypto {
                reason: CryptoReason::Corrupt,
            }
        }
        _ => storage_error(context, error),
    }
}

/// Take a mutex, recovering rather than panicking if a previous holder panicked.
///
/// `unwrap()` here would be a decision to crash the application because
/// something else already went wrong. Recovery is sound for both mutexes in this
/// module: `rusqlite::Transaction` rolls back on drop, so a panic that poisons
/// the connection mutex cannot leave a transaction open.
pub(crate) fn lock_recovering<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::error!("a store mutex was poisoned by an earlier panic; recovering it");
            poisoned.into_inner()
        }
    }
}
