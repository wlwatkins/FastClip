//! The store as the rest of the application sees it: the one connection, behind
//! the connection mutex, and one guard that every clip-touching command goes
//! through.
//!
//! `Store` holds three mutexes. The order they must be acquired in is fixed by
//! `storage.md` § The three mutexes, and the order they are taken in. That
//! section is the rule's single home; do not restate the order in this module.

use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::crypto::Dek;
use crate::error::ClipError;
use crate::storage::classify::Classification;
use crate::storage::convert::{self, Outcome, Plan};
use crate::storage::paths::StorePaths;
use crate::storage::recovery::{self, Recovered};
use crate::storage::{connection, lock_recovering, schema};

/// Everything about the store that is not the connection itself.
///
/// Held under its own short-lived mutex, one of the three `Store` holds. Where
/// the status mutex sits in the acquisition order is fixed by `storage.md` §
/// The three mutexes, and the order they are taken in.
struct Status {
    /// The four-state classification, established at startup recovery step 3
    /// and replaced only at a conversion's commit.
    classification: Classification,
    /// Whether a DEK is live. Always true for a store that is not encrypted;
    /// every launch of an encrypted store starts locked.
    unlocked: bool,
    /// The live data encryption key, or `None` for a store that is plaintext or
    /// locked.
    ///
    /// **Zeroised whenever it is replaced or cleared**, which `Dek`'s `Drop`
    /// does. ADR-0010 chose to give the key up on `lock` rather than flip a
    /// boolean, and that promise is kept here.
    dek: Option<Dek>,
    /// The fault startup recovery recorded, served by `get_lock_state`.
    fault: Option<ClipError>,
}

/// What the store becomes at a conversion's commit point.
///
/// Both variants leave the store **unlocked**: `enable_encryption` has just been
/// given the PIN and has nothing to prove (contract, `enable_encryption`), and a
/// plaintext store has no lock to be in.
pub enum Becoming {
    /// `enable_encryption`, under this freshly minted key.
    Encrypted(Dek),
    /// `disable_encryption`.
    Plaintext,
}

/// What [`Store::convert`] did.
///
/// Two fields rather than one `Result`, because "did the store change?" and
/// "did the command succeed?" are genuinely different questions here and the
/// caller needs both: `lock_state` is emitted on `committed`, and the command's
/// return value comes from `result`.
pub struct Converted {
    /// Whether the rename happened. Decides whether `lock_state` is emitted.
    pub committed: bool,
    /// What the command returns.
    pub result: Result<(), ClipError>,
}

/// The clip store. Managed by Tauri as application state — there is no global.
pub struct Store {
    paths: Option<StorePaths>,
    connection: Mutex<Option<Connection>>,
    status: Mutex<Status>,
    /// Serialises whole-store conversions **including their key-material
    /// step**. See [`Store::begin_conversion`].
    conversion: Mutex<()>,
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
            conversion: Mutex::new(()),
            status: Mutex::new(Status {
                classification,
                unlocked: !classification.is_encrypted(),
                // No launch starts with a key: an encrypted store is locked
                // until a PIN unwraps one, and a plaintext store has none.
                dek: None,
                fault,
            }),
        }
    }

    /// A store that could not be located at all. Nothing is ever written.
    fn faulted(fault: ClipError) -> Self {
        Self {
            paths: None,
            connection: Mutex::new(None),
            conversion: Mutex::new(()),
            status: Mutex::new(Status {
                classification: Classification::Unreadable,
                unlocked: false,
                dek: None,
                fault: Some(fault),
            }),
        }
    }

    /// Where the store lives, or `None` when the home directory would not
    /// resolve.
    pub fn paths(&self) -> Option<&StorePaths> {
        self.paths.as_ref()
    }

    /// The store directory, or `storage` when it could not be created or
    /// reached at all.
    ///
    /// **This is the only reason `get_settings` can fail** (contract,
    /// [`get_settings`]). It is asked of the directory rather than of
    /// `settings.json`, because nothing about that file's contents — missing,
    /// truncated, unparseable or too new — is an error.
    ///
    /// Checked here rather than read from the fault startup recovery recorded,
    /// because that fault covers a second condition as well: a `clips.db` that
    /// is present but unreadable, with the directory perfectly fine. Reporting
    /// that from `get_settings` would tell the frontend the settings were
    /// unavailable when they are sitting there readable.
    ///
    /// `is_dir()` answers false when the metadata call fails for any reason,
    /// which is the trap `storage.md` § Classifying clips.db names. It is the
    /// right answer *here* — "could not be reached" is exactly that — and it is
    /// safe because this decides an error variant and gates no deletion.
    pub fn directory(&self) -> Result<&StorePaths, ClipError> {
        match self.paths.as_ref() {
            Some(paths) if paths.dir().is_dir() => Ok(paths),
            Some(paths) => {
                log::error!(
                    "the store directory could not be reached: {}",
                    paths.dir().display()
                );
                Err(ClipError::Storage)
            }
            None => Err(ClipError::Storage),
        }
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

    /// The live data encryption key, if the store is encrypted and unlocked.
    ///
    /// Cloned rather than borrowed so the status mutex is not held while the key
    /// is used; the clone zeroises on drop like the original.
    pub fn dek(&self) -> Option<Dek> {
        lock_recovering(&self.status).dek.clone()
    }

    /// **Hold this for the whole of a conversion, from before its first write.**
    ///
    /// [`Store::convert`] takes the connection mutex, which serialises the
    /// database half — but `enable_encryption` writes `keyfile` *before* it
    /// calls `convert`, and that write is outside any lock. This closes it.
    ///
    /// **The race it prevents cannot happen today, and it is guarded anyway
    /// because the outcome is unrecoverable.** Two `enable_encryption` bodies
    /// would have to overlap. Sync Tauri commands cannot: `#[tauri::command]`
    /// without `async` generates `body_blocking`, which calls the function
    /// inline in the invoke handler (`tauri-macros`
    /// `command/wrapper.rs`), and the handler runs on the thread that owns the
    /// WebView2 controller — created in a single-threaded apartment, delivering
    /// `WebMessageReceived` through the same message pump (`wry`
    /// `webview2/mod.rs`). One IPC message is handled at a time. The tray's menu
    /// handler is on that thread too.
    ///
    /// Were they ever to overlap, the sequence destroys a store outright:
    ///
    /// | | |
    /// | --- | --- |
    /// | A | writes `keyfile(DEK_A)`, enters `convert`, takes the connection |
    /// | B | passes `encryption_enabled() == false` — the classification only changes at A's commit — and overwrites `keyfile` with `DEK_B` |
    /// | A | commits the store under `DEK_A`, beside a `keyfile` holding `DEK_B` |
    /// | B | aborts, and its abort deletes `keyfile` |
    ///
    /// The result is an encrypted store with **no key at all**, which no import
    /// and no PIN recovers.
    ///
    /// Re-checking `encryption_enabled()` under the connection mutex would not
    /// have been enough: B has already clobbered `keyfile` by then, and B's
    /// abort still deletes it. The guard has to be taken before the first write,
    /// which means before the state checks, so that a second caller refuses and
    /// returns *without reaching an abort path at all*.
    ///
    /// **Adding `async` to either conversion command removes the serialisation
    /// this reasons about.** The guard is what makes that survivable.
    pub fn begin_conversion(&self) -> MutexGuard<'_, ()> {
        lock_recovering(&self.conversion)
    }

    /// **Convert the whole store between plaintext and encrypted** (WP-07).
    ///
    /// `storage.md` section "Switching encryption on and off". The connection
    /// mutex is held for the entire conversion, so every clip command and `lock`
    /// waits behind it and none of them can observe a half-converted store - the
    /// same guarantee [`Store::with_unlocked_store`] gives, over a much longer
    /// operation.
    ///
    /// The two failure shapes are different and the caller must tell them apart,
    /// which is what [`Converted`] reports:
    ///
    /// | Outcome | Store on disk | `committed` | `result` |
    /// | ------- | ------------- | ----------- | -------- |
    /// | Everything succeeded | converted | `true` | `Ok` |
    /// | Committed, reopen failed | **converted** | `true` | `Err(storage)` |
    /// | Aborted | unchanged | `false` | the error |
    ///
    /// `committed` is what decides whether `lock_state` is emitted, and it is
    /// **not** the same question as whether this returned `Ok`. The middle row is
    /// the whole reason the type has two fields: the store is encrypted, the
    /// command failed, and the frontend must be told the first thing.
    pub fn convert(&self, becoming: Becoming) -> Converted {
        let paths = match self.paths.as_ref() {
            Some(paths) => paths,
            None => {
                return Converted {
                    committed: false,
                    result: Err(ClipError::Storage),
                }
            }
        };

        // The mutexes this function takes follow `storage.md` § The three
        // mutexes, and the order they are taken in. Every status guard taken
        // below is released before the next step.
        let mut held = lock_recovering(&self.connection);

        let source = match held.take() {
            Some(open) => open,
            None => {
                return Converted {
                    committed: false,
                    result: Err(self.fault_for_a_closed_store()),
                }
            }
        };

        // The key each side carries. Enabling reads a plaintext store and writes
        // an encrypted one; disabling is the same steps with the roles swapped.
        let current = self.dek();
        let (source_key, target_key) = match &becoming {
            Becoming::Encrypted(new) => (None, Some(new)),
            Becoming::Plaintext => (current.as_ref(), None),
        };

        let plan = Plan {
            paths,
            source_key,
            target_key,
        };

        // **The classification is written at the rename, not after the command.**
        // Passing it as a callback is what makes that literally true: a
        // conversion whose reopen fails still committed, and the classification
        // must already describe the store as it now is.
        let committed = std::cell::Cell::new(false);
        let outcome = convert::run(&plan, source, || {
            committed.set(true);
            self.apply_commit(&becoming);
        });

        let committed = committed.get();
        let result = match outcome {
            Outcome::Converted(reopened) => {
                *held = Some(reopened);
                Ok(())
            }
            Outcome::CommittedButClosed => {
                // Converted, with no connection. Recording the fault is what
                // makes every clip command serve `storage` for the rest of the
                // session rather than trying to open the store on demand.
                *held = None;
                self.record_fault(ClipError::Storage);
                Err(ClipError::Storage)
            }
            Outcome::Aborted { original, error } => {
                if original.is_none() {
                    // Nothing converted, and the original could not be reopened
                    // either. The store on disk is intact; this process cannot
                    // reach it until it restarts.
                    self.record_fault(ClipError::Storage);
                }
                *held = original;
                Err(error)
            }
        };

        Converted { committed, result }
    }

    /// Write the new state, at the conversion's commit point and nowhere else.
    fn apply_commit(&self, becoming: &Becoming) {
        let mut status = lock_recovering(&self.status);
        match becoming {
            Becoming::Encrypted(dek) => {
                status.classification = Classification::Encrypted;
                status.unlocked = true;
                status.dek = Some(dek.clone());
            }
            Becoming::Plaintext => {
                status.classification = Classification::Plaintext;
                status.unlocked = true;
                // Dropping the key zeroises it.
                status.dek = None;
            }
        }
        // A conversion that committed replaced the store, so whatever startup
        // recovery recorded about the old one no longer describes anything.
        status.fault = None;
    }

    fn record_fault(&self, fault: ClipError) {
        lock_recovering(&self.status).fault = Some(fault);
    }

    /// **Open the encrypted store with a recovered DEK** (`unlock`).
    ///
    /// This is the launch path: startup recovery deliberately does not open an
    /// encrypted store, because no DEK exists until a PIN unwraps one. It is
    /// also the path back from [`Store::lock`], and there is only one — ADR-0010
    /// closed the database precisely so that a second, cheaper way to become
    /// unlocked could not exist.
    ///
    /// The connection mutex is held across the open, so a command that queued
    /// while the store was locked sees either the locked state or the opened
    /// one.
    ///
    /// **The store is left locked if anything fails.** A `crypto { corrupt }`
    /// from a database that will not open must not leave a half-unlocked store
    /// whose DEK is live but whose connection is absent.
    pub fn unlock_with(&self, dek: Dek) -> Result<(), ClipError> {
        let paths = match self.paths.as_ref() {
            Some(paths) => paths,
            None => return Err(ClipError::Storage),
        };

        let mut held = lock_recovering(&self.connection);

        let opened = connection::open_encrypted(&paths.db(), &dek)?;
        schema::check_and_migrate(&opened)?;

        let mut status = lock_recovering(&self.status);
        status.unlocked = true;
        status.dek = Some(dek);
        // The store opened, so whatever was recorded about it before no longer
        // describes anything.
        status.fault = None;
        drop(status);

        *held = Some(opened);
        Ok(())
    }

    /// **Lock the store: close the database and give up the key** (ADR-0010).
    ///
    /// The five steps of `storage.md` section "Locking on demand", in order:
    ///
    /// 1. Acquire the connection mutex — this waits for any command already
    ///    executing, so no transaction is interrupted and no committed write is
    ///    lost.
    /// 2. Read lock state **after** taking the mutex. Reading it first would let
    ///    a `disable_encryption` already running complete in between, and this
    ///    would then lock a store that had just become plaintext.
    /// 3. Checkpoint and close.
    /// 4. Zeroise the DEK and clear the unlocked flag.
    ///
    /// Step 5 — the tray rebuild and the emission — belongs to the command.
    ///
    /// **Once locking begins it cannot fail.** It can be *refused* before it
    /// begins, which is the `Err` below, but there is no failure after that
    /// point: a failed checkpoint or close is absorbed inside
    /// [`connection::checkpoint_and_close`] and the key is given up regardless.
    /// Refusing to lock because a disk operation failed would leave the clips on
    /// screen at the moment the user asked for them not to be.
    ///
    /// **Idempotent.** Locking an already-locked store succeeds and does
    /// nothing, because the caller's postcondition already holds.
    pub fn lock(&self) -> Result<(), ClipError> {
        // 1.
        let mut held = lock_recovering(&self.connection);

        // 2. After the mutex, never before.
        if !self.encryption_enabled() {
            return Err(ClipError::WrongState {
                required: crate::error::RequiredState::Encrypted,
            });
        }
        if self.is_locked() {
            return Ok(());
        }

        // 3. Absorbs its own failures.
        if let Some(open) = held.take() {
            connection::checkpoint_and_close(open);
        }

        // 4. Dropping the key zeroises it.
        let mut status = lock_recovering(&self.status);
        status.unlocked = false;
        status.dek = None;

        Ok(())
    }

    /// Replace the live key material after `change_pin`.
    ///
    /// The DEK does not change — `change_pin` re-wraps the same key rather than
    /// re-encrypting the database — so there is nothing to do to the connection
    /// or the classification. This exists so the invariant "the DEK the store
    /// holds is the DEK the key material unwraps to" is maintained by one place
    /// even though today the two are identical.
    pub fn holds_dek(&self, dek: &Dek) -> bool {
        match lock_recovering(&self.status).dek.as_ref() {
            Some(held) => held == dek,
            None => false,
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

    /// The settings commands need the directory and nothing else. A store whose
    /// database is unreadable still has one, so they keep working while the clip
    /// commands report the fault.
    #[test]
    fn the_directory_is_available_even_when_the_database_is_not() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // A directory where the database should be: unreadable, not absent.
        if let Err(e) = std::fs::create_dir(paths.db()) {
            panic!("could not create the fixture: {e}");
        }

        let store = Store::open(paths);
        assert_eq!(store.startup_fault(), Some(ClipError::Storage));
        assert!(
            store.directory().is_ok(),
            "the settings live beside the database, not inside it"
        );
    }

    #[test]
    fn a_store_with_nowhere_to_live_has_no_directory_either() {
        let store = Store::faulted(ClipError::Storage);
        assert_eq!(store.directory().map(|_| ()), Err(ClipError::Storage));
    }

    /// The one reason `get_settings` may fail: the directory could not be
    /// created or reached at all.
    #[test]
    fn a_directory_that_could_not_be_created_reports_storage() {
        let parent = dir();
        // A file where the store directory should be, so `create_dir_all` fails.
        let occupied = parent.path().join(".fast-clip");
        if let Err(e) = std::fs::write(&occupied, b"not a directory") {
            panic!("could not write the fixture: {e}");
        }

        let store = Store::open(StorePaths::at(occupied));
        assert_eq!(store.directory().map(|_| ()), Err(ClipError::Storage));
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

    /// **The guard that stops two conversions overlapping.**
    ///
    /// [`Store::begin_conversion`] explains why the race cannot happen today and
    /// why it is guarded regardless: the outcome is an encrypted store with no
    /// key at all. This asserts the exclusion itself, so that removing the mutex
    /// — or replacing it with something that does not exclude — fails here
    /// rather than in a user's store.
    #[test]
    fn only_one_conversion_may_hold_the_guard_at_a_time() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let parent = dir();
        let store = Arc::new(store_in(&parent));
        let inside = Arc::new(AtomicUsize::new(0));
        let overlapped = Arc::new(AtomicBool::new(false));

        let threads: Vec<_> = (0..6)
            .map(|_| {
                let store = Arc::clone(&store);
                let inside = Arc::clone(&inside);
                let overlapped = Arc::clone(&overlapped);
                std::thread::spawn(move || {
                    let _guard = store.begin_conversion();
                    if inside.fetch_add(1, Ordering::SeqCst) != 0 {
                        overlapped.store(true, Ordering::SeqCst);
                    }
                    // Long enough that an absent guard would be seen.
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    inside.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();

        for thread in threads {
            if thread.join().is_err() {
                panic!("a conversion thread panicked");
            }
        }

        assert!(
            !overlapped.load(Ordering::SeqCst),
            "two conversions held the guard at once; a second enable_encryption \
             could then overwrite the first's key material and its abort would \
             delete it, leaving an encrypted store with no key"
        );
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
