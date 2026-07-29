//! The store as the rest of the application sees it: one connection behind one
//! mutex, and one guard that every clip-touching command goes through.

use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::ClipError;
use crate::storage::classify::Classification;
use crate::storage::paths::StorePaths;
use crate::storage::recovery::{self, Recovered};
use crate::storage::{connection, lock_recovering};

/// Everything about the store that is not the connection itself.
///
/// Held under its own short-lived mutex. **Lock ordering is connection first,
/// then status**, and the status mutex is never held across an acquisition of
/// the connection mutex.
struct Status {
    /// The four-state classification, established at startup recovery step 3
    /// and replaced only at a conversion's commit.
    classification: Classification,
    /// Whether a DEK is live. Always true for a store that is not encrypted;
    /// every launch of an encrypted store starts locked.
    unlocked: bool,
    /// The fault startup recovery recorded, served by `get_lock_state`.
    fault: Option<ClipError>,
}

/// The clip store. Managed by Tauri as application state — there is no global.
pub struct Store {
    paths: Option<StorePaths>,
    connection: Mutex<Option<Connection>>,
    status: Mutex<Status>,
}

impl Store {
    /// Resolve `~/.fast-clip/` and run startup recovery.
    ///
    /// Infallible by construction: a backend that refuses to start leaves
    /// nothing to show the user, so every fault is recorded and served from the
    /// command that declares it.
    pub fn open_default() -> Self {
        match StorePaths::from_home() {
            Ok(paths) => Self::open(paths),
            Err(fault) => Self::faulted(fault),
        }
    }

    /// Run startup recovery against one store directory.
    pub fn open(paths: StorePaths) -> Self {
        let Recovered {
            classification,
            connection,
            fault,
        } = recovery::recover(&paths);

        Self {
            paths: Some(paths),
            connection: Mutex::new(connection),
            status: Mutex::new(Status {
                classification,
                unlocked: !classification.is_encrypted(),
                fault,
            }),
        }
    }

    /// A store that could not be located at all. Nothing is ever written.
    fn faulted(fault: ClipError) -> Self {
        Self {
            paths: None,
            connection: Mutex::new(None),
            status: Mutex::new(Status {
                classification: Classification::Unreadable,
                unlocked: false,
                fault: Some(fault),
            }),
        }
    }

    /// Where the store lives, or `None` when the home directory would not
    /// resolve.
    pub fn paths(&self) -> Option<&StorePaths> {
        self.paths.as_ref()
    }

    pub fn classification(&self) -> Classification {
        lock_recovering(&self.status).classification
    }

    /// Whether the store on disk is encrypted. Read from the classification,
    /// which comes from the database header — never from the presence of a
    /// `keyfile`, which would give two files that can disagree.
    pub fn encryption_enabled(&self) -> bool {
        lock_recovering(&self.status).classification.is_encrypted()
    }

    /// Whether the store is encrypted and not open.
    pub fn is_locked(&self) -> bool {
        let status = lock_recovering(&self.status);
        status.classification.is_encrypted() && !status.unlocked
    }

    /// The fault startup recovery recorded, if any.
    pub fn startup_fault(&self) -> Option<ClipError> {
        lock_recovering(&self.status).fault.clone()
    }

    /// **The shared lock guard.** Every command marked "works while locked: no"
    /// in the contract runs its work inside this, and none of them checks lock
    /// state itself.
    ///
    /// Lock state is checked twice: on entry, and again after the connection
    /// mutex is acquired. The second check is what makes a concurrent `lock`
    /// produce `locked` rather than a torn write or an `internal` — a command
    /// that passed the first check and then waited behind `lock` would otherwise
    /// wake to a closed connection and report `storage` for what is an ordinary
    /// lock.
    ///
    /// The observable rule: such a command either completes in full or returns
    /// `locked`. Never partially, and never a third outcome *from the lock*.
    pub fn with_unlocked_store<T>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, ClipError>,
    ) -> Result<T, ClipError> {
        // Check one: before queuing for the connection.
        if self.is_locked() {
            return Err(ClipError::Locked);
        }

        let mut held = lock_recovering(&self.connection);

        // Check two: after the wait. `lock` takes this same mutex, so anything
        // that locked the store while this command queued is visible now.
        if self.is_locked() {
            return Err(ClipError::Locked);
        }

        match held.as_mut() {
            Some(open) => work(open),
            None => Err(self.fault_for_a_closed_store()),
        }
    }

    /// Why the store is closed, for a caller that reached it unlocked.
    fn fault_for_a_closed_store(&self) -> ClipError {
        match self.startup_fault() {
            Some(fault) => fault,
            None => {
                log::error!("the store is closed and no fault was recorded");
                ClipError::Storage
            }
        }
    }

    /// Checkpoint and close at shutdown. Absorbed on failure — there is nothing
    /// left to tell the user.
    pub fn shutdown(&self) {
        let taken = lock_recovering(&self.connection).take();
        if let Some(open) = taken {
            connection::checkpoint_and_close(open);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;
    use crate::storage::clips;
    use std::sync::Arc;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn store_in(parent: &tempfile::TempDir) -> Store {
        Store::open(StorePaths::at(parent.path().join(".fast-clip")))
    }

    #[test]
    fn a_fresh_store_serves_work_and_reports_no_fault() {
        let parent = dir();
        let store = store_in(&parent);
        assert!(!store.is_locked());
        assert!(!store.encryption_enabled());
        assert_eq!(store.startup_fault(), None);

        let count = store.with_unlocked_store(|open| {
            clips::insert(open, "first", "a value", Colour::DEFAULT)?;
            clips::count(open)
        });
        assert_eq!(count, Ok(1));
    }

    #[test]
    fn an_encrypted_store_returns_locked_from_the_guard_and_is_never_opened() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        if let Err(e) = std::fs::write(paths.db(), [0x1f; 64]) {
            panic!("could not write the fixture: {e}");
        }

        let store = Store::open(paths);
        assert!(store.encryption_enabled());
        assert!(store.is_locked());
        assert_eq!(
            store.with_unlocked_store(|open| clips::count(open)),
            Err(ClipError::Locked)
        );
    }

    #[test]
    fn a_recorded_startup_fault_is_what_the_guard_returns() {
        use crate::error::CryptoReason;

        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend_from_slice(&[0u8; 512]);
        if let Err(e) = std::fs::write(paths.db(), &bytes) {
            panic!("could not write the fixture: {e}");
        }

        let store = Store::open(paths);
        assert_eq!(
            store.with_unlocked_store(|open| clips::count(open)),
            Err(ClipError::Crypto {
                reason: CryptoReason::Corrupt
            })
        );
    }

    #[test]
    fn a_store_with_nowhere_to_live_reports_storage_rather_than_panicking() {
        let store = Store::faulted(ClipError::Storage);
        assert!(store.paths().is_none());
        assert_eq!(store.startup_fault(), Some(ClipError::Storage));
        assert_eq!(
            store.with_unlocked_store(|open| clips::count(open)),
            Err(ClipError::Storage)
        );
    }

    #[test]
    fn concurrent_writers_queue_behind_the_mutex_and_leave_the_order_dense() {
        let parent = dir();
        let store = Arc::new(store_in(&parent));

        let threads: Vec<_> = (0..8)
            .map(|n| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    store.with_unlocked_store(|open| {
                        clips::insert(open, &format!("clip {n}"), "a value", Colour::DEFAULT)
                    })
                })
            })
            .collect();

        for thread in threads {
            match thread.join() {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => panic!("a concurrent insert failed: {e}"),
                Err(_) => panic!("a writer thread panicked"),
            }
        }

        let positions = store.with_unlocked_store(|open| {
            Ok(clips::list(open)?
                .iter()
                .map(|c| c.position)
                .collect::<Vec<i64>>())
        });
        assert_eq!(positions, Ok((0..8).collect::<Vec<i64>>()));
    }

    #[test]
    fn shutdown_closes_the_connection_and_later_work_reports_the_fault() {
        let parent = dir();
        let store = store_in(&parent);
        if let Err(e) =
            store.with_unlocked_store(|open| clips::insert(open, "a", "b", Colour::DEFAULT))
        {
            panic!("the insert should succeed: {e}");
        }
        store.shutdown();

        // No fault was recorded, so this is the defensive branch: `storage`,
        // never a panic and never a silently empty list.
        assert_eq!(
            store.with_unlocked_store(|open| clips::count(open)),
            Err(ClipError::Storage)
        );
    }
}
