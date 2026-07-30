//! Lock state, and the three commands that change it: `get_lock_state`,
//! `unlock` and `lock`.
//!
//! `docs/src/architecture/contract.md` §2 is authoritative. Where this file and
//! that page disagree, this file is the defect.
//!
//! **This command reports; it does not discover.** The two faults that can only
//! be found by opening the database — `crypto { corrupt }` and
//! `unsupported_version { schema }` — are found once, by startup recovery, which
//! records the fault and leaves the store with no connection
//! (`storage.md` § Startup recovery). Every command that needs the store relays
//! that same recorded value, and this one relays it too. It is first in the
//! startup sequence, which is why the frontend meets a broken store here; that
//! is a sequencing fact and not an exclusive right to carry it
//! (contract § Opening the database).
//!
//! It therefore never opens the clip database, and cannot fail for a reason
//! that did not already exist before it was called. **It does read `keyfile`**
//! when the store is locked, because the failed-attempt count and the backoff
//! deadline live inside that file's DPAPI blob and there is nowhere else to
//! get them (contract, `get_lock_state`).
//!
//! **Called once at startup, and never polled.** Every later change arrives on
//! the `lock_state` event (contract, closed question 11).
//!
//! `unlock` and `lock` are a pair, and ADR-0010 is why they are shaped as they
//! are: `lock` closes the database and zeroises the DEK rather than flipping a
//! boolean, which makes `unlock` afterwards the launch path exactly. There is
//! one way in, and it costs one Argon2id unwrap.

use tauri::{AppHandle, Runtime, State};

use crate::commands::clips::snapshot;
use crate::commands::events;
use crate::commands::wire::LockState;
use crate::crypto::{kdf, keyfile, wrap, KeyMaterial, Pin};
use crate::error::{ClipError, RequiredState};
use crate::storage::{Store, StorePaths};
use crate::tray;

/// The lock state, or the fault startup recovery recorded.
///
/// Errors: `crypto`, `unsupported_version`, `storage` — every one of them
/// relayed verbatim from startup recovery, so the value the frontend sees here
/// is identical to the value any other command would give it.
///
/// **A first run succeeds.** Startup recovery created `~/.fast-clip/` and an
/// empty `clips.db` at schema version 1 before any command could be served, so
/// there is no absent store for this command to describe.
#[tauri::command(rename_all = "snake_case")]
pub fn get_lock_state(store: State<'_, Store>) -> Result<LockState, ClipError> {
    match store.startup_fault() {
        Some(fault) => Err(fault),
        None => lock_state(&store),
    }
}

/// Unwrap the DEK with a PIN and open the store.
///
/// **This is the launch path, and it is the only one.** Startup recovery
/// deliberately leaves an encrypted store closed, because no DEK exists until a
/// PIN unwraps one; `unlock` is also the way back from [`lock`], on exactly the
/// same path — read `keyfile` and its version byte, unwrap, reopen, emit, rebuild
/// the tray. ADR-0010 closed the database so that no second, cheaper route could
/// exist.
///
/// **Nothing is ever wiped** ([ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md)).
/// A wrong PIN costs an attempt and nothing else; twenty wrong PINs leave the
/// wrapped DEK byte-for-byte what it was.
///
/// The order below is fixed, and the backoff check comes **before** the PIN is
/// evaluated so that a correct PIN entered early neither succeeds nor resets the
/// wait:
///
/// 1. Validate the argument.
/// 2. `wrong_state { encrypted }` when encryption is off — there is nothing to
///    unlock. `wrong_state { locked }` when already unlocked; it is not a silent
///    success, because that would accept a wrong PIN.
/// 3. Read `keyfile`. Missing or from another account → `crypto`; too new →
///    `unsupported_version { key_material }`.
/// 4. A wait already running → `backoff { retry_after_ms }`, **PIN not
///    evaluated**.
/// 5. Derive and unwrap. The AEAD tag decides, so `attempts_remaining` is exact
///    rather than a guess about whether the output looks like a key.
/// 6. On success open the database and emit; on failure count the attempt.
#[tauri::command(rename_all = "snake_case")]
pub fn unlock<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    pin: Option<String>,
) -> Result<(), ClipError> {
    let pin = Pin::parse("pin", pin)?;

    if !store.encryption_enabled() {
        return Err(ClipError::WrongState {
            required: RequiredState::Encrypted,
        });
    }
    if !store.is_locked() {
        return Err(ClipError::WrongState {
            required: RequiredState::Locked,
        });
    }

    let paths = store.directory()?.clone();
    let material = keyfile::read(&paths)?;

    // Step 4. Before the PIN is looked at.
    if let Some(remaining) = material.backoff_remaining_ms(keyfile::now_unix_ms()) {
        return Err(ClipError::Backoff {
            retry_after_ms: remaining,
        });
    }

    // Step 5.
    let wrapping = kdf::derive(&pin, &material.kdf)?;
    let dek = match wrap::unwrap(&wrapping, &material.wrapped_dek)? {
        Some(dek) => dek,
        None => return Err(record_wrong_pin(&paths, material)),
    };

    // Step 6. The PIN was right, so the counter is **neither incremented nor
    // reset** if the database then refuses to open: the `?` below returns
    // before `reset_attempts`, leaving the count exactly as it was on disk.
    //
    // That is the right outcome and it is deliberate rather than incidental. A
    // correct PIN over an unopenable store is not a failed attempt, so it must
    // not be charged one; and clearing the counter would be recording a
    // successful unlock that did not happen. The contract does not name this
    // case, and leaving the count untouched is compatible with everything it
    // does say.
    store.unlock_with(dek)?;
    reset_attempts(&paths, material);

    // A constant, not a derivation: the state after a successful unlock is known
    // exactly, and nothing fallible may run after the store has changed state
    // (see [`lock`] and `LockState::UNLOCKED`).
    events::emit_lock_state(&app, &LockState::UNLOCKED);
    let listed: Result<Vec<crate::commands::wire::Clip>, ClipError> =
        store.with_unlocked_store(|open| snapshot(open));
    match listed {
        Ok(listed) => events::emit_update_clips(&app, &listed),
        Err(error) => {
            // The store opened a moment ago, so this is not reachable by any
            // modelled condition. The unlock still succeeded and the frontend's
            // `lock_state` already told it so; it will recover the list.
            log::error!("the clip list could not be read after unlocking: {error}");
        }
    }
    tray::refresh(&app);
    Ok(())
}

/// Count a wrong PIN and say how many attempts are left.
///
/// **The number reported is the one actually on disk.** If the counter write
/// fails the increment did not persist, and reporting it anyway would put the
/// frontend and the file into disagreement about the one value the backoff is
/// derived from (`storage.md` § Every write to `keyfile` is atomic. Every one.).
///
/// A failed write does not fail the command either: the answer to the user is
/// still "that PIN was wrong".
fn record_wrong_pin(paths: &StorePaths, material: KeyMaterial) -> ClipError {
    let mut updated = material.clone();
    updated.record_failure(keyfile::now_unix_ms());

    let persisted = match keyfile::write(paths, &updated) {
        Ok(()) => true,
        Err(error) => {
            log::error!("a failed unlock attempt could not be recorded: {error}");
            false
        }
    };

    let on_disk = if persisted { &updated } else { &material };
    let attempts_remaining = on_disk.attempts_remaining();

    ClipError::BadPin {
        attempts_remaining: Some(attempts_remaining),
        // The attempt that exhausts the allowance reports the wait it just
        // armed, so the frontend has a duration to count down from without a
        // constant of its own. Flat 30 seconds, every time (ADR-0011).
        retry_after_ms: if attempts_remaining == 0 {
            Some(keyfile::BACKOFF_MS)
        } else {
            None
        },
    }
}

/// Clear the counter after a successful unlock.
///
/// **Absorbed.** The user typed the right PIN; refusing the unlock because a
/// number could not be cleared would deny them their store to protect a counter.
/// A stale counter costs a backoff they have not earned on some later launch,
/// which is recoverable.
fn reset_attempts(paths: &StorePaths, material: KeyMaterial) {
    if material.failed_attempts == 0 && material.locked_until_unix_ms.is_none() {
        return;
    }
    let mut cleared = material;
    cleared.record_success();
    if let Err(error) = keyfile::write(paths, &cleared) {
        log::error!("the unlock attempt counter could not be cleared: {error}");
    }
}

/// Lock the store at the user's request (ADR-0010).
///
/// **The only way the store becomes locked while FastClip is running.** Manual
/// only: no idle timeout, no lock-on-minimise. The PIN is not an argument — the
/// caller is already unlocked, and demanding a secret to give up access protects
/// nothing.
///
/// The mechanism is [`Store::lock`]: checkpoint, close the connection, zeroise
/// the DEK. **Not a boolean.** With the connection open and the key live,
/// "locked" would mean only that the backend declines to answer, and any defect
/// anywhere in the backend would re-expose every clip with no PIN.
///
/// **The tray is rebuilt before this returns.** Clip labels left in a native menu
/// after a manual lock breach
/// [criterion 10](../../../docs/src/product/spec.md#8-acceptance-criteria)
/// exactly as labels left in the window would, and this is the call site where
/// forgetting it would do that.
///
/// **It does not emit `update_clips`.** No event carries an empty list; the
/// frontend discards its own on `locked: true`. An empty `Clip[]` is
/// indistinguishable from a store whose clips were all deleted.
///
/// **Errors: `wrong_state { encrypted }` and nothing else.** Contract §2 states
/// the set in full and says that is the only variant. `lock` cannot return
/// `storage`, and it cannot return `crypto` either — releasing the key must not
/// be refused because a disk operation failed.
///
/// **That is why the payload below is a constant and not derived.** Every step
/// after `Store::lock` returns runs against a store that is *already* locked:
/// the connection is closed and the DEK is zeroised. A fallible step there —
/// and deriving the payload reads `keyfile`, which is fallible — would return an
/// undeclared error **and skip the emission**, leaving `locked` false on the
/// frontend with the clip list, any open form and the search query still
/// rendered over a store that has given up its key. The window is real: ADR-0010
/// names a sync client restoring the file, deletion, and DPAPI refusing as the
/// reasons `unlock` had to widen its error set at all.
///
/// So nothing after the lock takes effect may fail. `tray::refresh` absorbs its
/// own failures (review 003's F4, answered at WP-08),
/// [`events::emit_lock_state`] absorbs its own, and
/// [`LockState::LOCKED`] cannot fail because it reads nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn lock<R: Runtime>(app: AppHandle<R>, store: State<'_, Store>) -> Result<(), ClipError> {
    // The only step that may refuse, and it refuses *before* locking begins.
    store.lock()?;

    // Step 5, in the order `storage.md` fixes: rebuild, emit, return. Neither
    // of these can fail, and neither may be given a `?`.
    tray::refresh(&app);
    events::emit_lock_state(&app, &LockState::LOCKED);
    Ok(())
}

/// **The lock state, derived from the store. One function, one spelling.**
///
/// `get_lock_state` answers with this and every `lock_state` emission carries
/// it, so the value the frontend polls once at startup and the value pushed to
/// it afterwards cannot disagree. A second derivation is how they would: an
/// earlier draft of `enable_encryption` built its own payload and reported
/// `attempts_remaining: 5` over an unlocked store, where contract §1 says the
/// field is `null` whenever `locked` is false.
///
/// With encryption off — the default — every field is fixed:
/// `{ false, false, null, null }`.
///
/// **`keyfile` is read only when the store is locked**, and that is contract §1
/// rather than an optimisation: `attempts_remaining` is `null` whenever `locked`
/// is false, so there is nothing to read it for. It also keeps an unlocked
/// session from failing over key material it does not need — after
/// `enable_encryption` the store is open and usable, and a damaged `keyfile`
/// should surface at the next `unlock`, not turn the settings view into a
/// failure screen.
///
/// A read that fails while locked **is** reported, because then the counter is
/// the answer: `crypto { bad_key_material }` at launch is how a user learns the
/// store belongs to another Windows account (contract, `get_lock_state`).
pub(crate) fn lock_state(store: &Store) -> Result<LockState, ClipError> {
    if !store.encryption_enabled() {
        return Ok(LockState::UNENCRYPTED);
    }

    if !store.is_locked() {
        return Ok(LockState {
            encryption_enabled: true,
            locked: false,
            attempts_remaining: None,
            retry_after_ms: None,
        });
    }

    let paths = store.directory()?;
    let material = keyfile::read(paths)?;

    Ok(LockState {
        encryption_enabled: true,
        locked: true,
        attempts_remaining: Some(material.attempts_remaining()),
        retry_after_ms: material.backoff_remaining_ms(keyfile::now_unix_ms()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyMaterial;
    use crate::error::{CryptoReason, VersionComponent};
    use crate::storage::{schema, StorePaths};

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn paths_in(parent: &tempfile::TempDir) -> StorePaths {
        StorePaths::at(parent.path().join(".fast-clip"))
    }

    fn fixture_dir(paths: &StorePaths) {
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
    }

    fn write(paths: &StorePaths, bytes: &[u8]) {
        if let Err(e) = std::fs::write(paths.db(), bytes) {
            panic!("could not write the fixture: {e}");
        }
    }

    /// A real encrypted store with key material, at the KDF's binding floor.
    ///
    /// The floor rather than the shipped tuple: what these tests exercise is the
    /// counter, not the cost, and the shipped 128 MiB tuple would add seconds to
    /// each one.
    fn encrypted_fixture(paths: &StorePaths) -> (crate::crypto::Dek, KeyMaterial) {
        use crate::crypto::kdf::{KdfParams, MIN_M_COST_KIB, MIN_T_COST, P_COST};
        use crate::crypto::Dek;
        use crate::storage::{connection, schema};

        fixture_dir(paths);

        let dek = match Dek::generate() {
            Ok(dek) => dek,
            Err(e) => panic!("a DEK should generate: {e}"),
        };
        let pin = match Pin::parse("pin", Some("135790".to_owned())) {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture PIN should parse: {e}"),
        };
        let mut params = match KdfParams::generate() {
            Ok(params) => params,
            Err(e) => panic!("parameters should generate: {e}"),
        };
        params.m_cost_kib = MIN_M_COST_KIB;
        params.t_cost = MIN_T_COST;
        params.p_cost = P_COST;

        let wrapping = match kdf::derive(&pin, &params) {
            Ok(key) => key,
            Err(e) => panic!("the wrapping key should derive: {e}"),
        };
        let wrapped_dek = match wrap::wrap(&wrapping, &dek) {
            Ok(wrapped) => wrapped,
            Err(e) => panic!("the DEK should wrap: {e}"),
        };
        let material = KeyMaterial {
            kdf: params,
            wrapped_dek,
            failed_attempts: 0,
            locked_until_unix_ms: None,
        };
        if let Err(e) = keyfile::write(paths, &material) {
            panic!("the fixture key material should write: {e}");
        }

        let connection = match connection::open_encrypted(&paths.db(), &dek) {
            Ok(connection) => connection,
            Err(e) => panic!("the fixture store should open: {e}"),
        };
        if let Err(e) = schema::create(&connection) {
            panic!("the fixture schema should create: {e}");
        }
        connection::checkpoint_and_close(connection);

        (dek, material)
    }

    /// The answer a new user gets, and the one the whole startup sequence is
    /// built around.
    #[test]
    fn a_fresh_store_is_unencrypted_and_unlocked() {
        let parent = dir();
        let store = Store::open(paths_in(&parent));
        assert_eq!(lock_state(&store), Ok(LockState::UNENCRYPTED));
        assert_eq!(store.startup_fault(), None);
    }

    /// The relay. Startup recovery recorded this; the command repeats it
    /// verbatim rather than deciding anything.
    #[test]
    fn a_corrupt_database_is_relayed_as_the_recorded_crypto_fault() {
        let parent = dir();
        let paths = paths_in(&parent);
        fixture_dir(&paths);
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend_from_slice(&[0u8; 512]);
        write(&paths, &bytes);

        let store = Store::open(paths);
        assert_eq!(
            store.startup_fault(),
            Some(ClipError::Crypto {
                reason: CryptoReason::Corrupt
            }),
            "the fault is recorded at startup, not discovered here"
        );
    }

    /// With encryption off, a too-new schema reaches the frontend from this
    /// command — it is the first in the sequence to ask for the store.
    #[test]
    fn a_too_new_schema_is_relayed_as_the_recorded_version_fault() {
        let parent = dir();
        let paths = paths_in(&parent);
        {
            let store = Store::open(paths.clone());
            let bumped = store.with_unlocked_store(|open| {
                open.pragma_update(None, "user_version", 99)
                    .map_err(|_| ClipError::Storage)
            });
            if let Err(e) = bumped {
                panic!("the fixture should be writable: {e}");
            }
            store.shutdown();
        }

        let store = Store::open(paths);
        assert_eq!(
            store.startup_fault(),
            Some(ClipError::UnsupportedVersion {
                component: VersionComponent::Schema,
                found: 99,
                supported: schema::SCHEMA_VERSION,
            })
        );
    }

    /// A `clips.db` that is present but unreadable is `storage`, and the message
    /// for it must not advise re-importing: nothing was deleted or created.
    #[test]
    fn an_unreadable_store_is_relayed_as_storage() {
        let parent = dir();
        let paths = paths_in(&parent);
        fixture_dir(&paths);
        if let Err(e) = std::fs::create_dir(paths.db()) {
            panic!("could not create the fixture: {e}");
        }

        let store = Store::open(paths);
        assert_eq!(store.startup_fault(), Some(ClipError::Storage));
    }

    /// **This was WP-07's marker, and it is now the real thing.** The two
    /// persisted values used to be hard-coded placeholders; they are read from
    /// `keyfile` now, and this asserts the counter is what the file says rather
    /// than what a constant says.
    #[test]
    fn a_locked_store_reports_the_attempt_state_persisted_in_the_key_material() {
        let parent = dir();
        let paths = paths_in(&parent);
        let (_dek, mut material) = encrypted_fixture(&paths);

        let store = Store::open(paths.clone());
        assert_eq!(
            store.startup_fault(),
            None,
            "being encrypted is not a fault"
        );
        assert_eq!(
            lock_state(&store),
            Ok(LockState {
                encryption_enabled: true,
                locked: true,
                attempts_remaining: Some(5),
                retry_after_ms: None,
            })
        );

        // Three failures on disk, and the report follows the file.
        for _ in 0..3 {
            material.record_failure(keyfile::now_unix_ms());
        }
        if let Err(e) = keyfile::write(&paths, &material) {
            panic!("the counter should persist: {e}");
        }
        let reopened = Store::open(paths.clone());
        assert_eq!(
            lock_state(&reopened),
            Ok(LockState {
                encryption_enabled: true,
                locked: true,
                attempts_remaining: Some(2),
                retry_after_ms: None,
            })
        );

        // Past five, the wait is reported as a snapshot of what remains.
        for _ in 0..2 {
            material.record_failure(keyfile::now_unix_ms());
        }
        if let Err(e) = keyfile::write(&paths, &material) {
            panic!("the counter should persist: {e}");
        }
        let backed_off = Store::open(paths);
        let state = match lock_state(&backed_off) {
            Ok(state) => state,
            Err(e) => panic!("the state should derive: {e}"),
        };
        assert_eq!(state.attempts_remaining, Some(0));
        match state.retry_after_ms {
            Some(wait) => assert!(wait > 0 && wait <= keyfile::BACKOFF_MS, "{wait}"),
            None => panic!("a running wait must be reported"),
        }
    }

    /// **An encrypted store with no key material is a different fault** from a
    /// locked one, and gets a different message: the copy deck renders `crypto`
    /// as "another Windows account", not "type your PIN".
    #[test]
    fn an_encrypted_store_with_no_key_material_is_crypto_rather_than_a_pin_prompt() {
        let parent = dir();
        let paths = paths_in(&parent);
        let (_dek, _material) = encrypted_fixture(&paths);
        if let Err(e) = std::fs::remove_file(paths.keyfile()) {
            panic!("the fixture keyfile should be removable: {e}");
        }

        let store = Store::open(paths);
        assert_eq!(
            lock_state(&store),
            Err(ClipError::Crypto {
                reason: CryptoReason::BadKeyMaterial
            })
        );
    }

    /// An unlocked encrypted store reports `null` for both counters, because
    /// contract §1 says the field is `null` whenever `locked` is false — and so
    /// `keyfile` is not read at all on that path.
    #[test]
    fn an_unlocked_encrypted_store_reports_no_counter_and_reads_no_key_material() {
        let parent = dir();
        let paths = paths_in(&parent);
        let (dek, _material) = encrypted_fixture(&paths);

        let store = Store::open(paths.clone());
        if let Err(e) = store.unlock_with(dek) {
            panic!("the fixture should unlock: {e}");
        }

        // Removing the key material must not change the answer, which is what
        // says it was never read.
        if let Err(e) = std::fs::remove_file(paths.keyfile()) {
            panic!("the fixture keyfile should be removable: {e}");
        }
        assert_eq!(
            lock_state(&store),
            Ok(LockState {
                encryption_enabled: true,
                locked: false,
                attempts_remaining: None,
                retry_after_ms: None,
            })
        );
        store.shutdown();
    }

    /// Contract §1: `encryption_enabled: false` implies `locked: false`, and the
    /// reverse combination is a backend defect.
    #[test]
    fn an_unencrypted_store_is_never_reported_as_locked() {
        let parent = dir();
        let store = Store::open(paths_in(&parent));
        let state = match lock_state(&store) {
            Ok(state) => state,
            Err(e) => panic!("the state should derive: {e}"),
        };
        assert!(!state.encryption_enabled);
        assert!(!state.locked);
        assert_eq!(state.attempts_remaining, None);
        assert_eq!(state.retry_after_ms, None);
    }
}
