//! `enable_encryption` and `disable_encryption` (WP-07).
//!
//! `docs/src/architecture/contract.md` §2 is authoritative for both, and
//! `storage.md` § Switching encryption on and off for the mechanism. Where this
//! file and those pages disagree, this file is the defect.
//!
//! **These two commands are the only code in the application that replaces the
//! user's whole store.** The conversion itself — steps 2 to 7, the verification
//! and the abort — is [`crate::storage::convert`], reached through
//! [`Store::convert`], and is tested without a Tauri application. What lives
//! here is the part the contract owns: the PIN, the key material, the state
//! checks, and the emission.
//!
//! **The emission is not conditional on the command succeeding.** Both commands
//! emit `lock_state` whenever the conversion *committed*, which is not the same
//! question as whether they return `Ok`. The rename is the instant the store's
//! state changes; a reopen that fails afterwards leaves a converted store and a
//! rejected command, and the frontend must be told the first thing
//! (contract, `lock_state`; `storage.md` § The emission is not conditional on
//! the command succeeding).
//!
//! **Nothing here logs the PIN, the DEK, the salt or the wrapped blob**
//! (ADR-0002, criterion 6). `tests/log_content.rs` greps a real log file for all
//! four.

use tauri::{AppHandle, Runtime, State};

use crate::commands::events;
use crate::commands::lock::lock_state;
use crate::crypto::{kdf, keyfile, wrap, Dek, KeyMaterial, Pin};
use crate::error::{ClipError, RequiredState};
use crate::storage::Becoming;
use crate::storage::Store;
use crate::tray;

/// Turn encryption on, converting the store under a freshly minted key.
///
/// The sequence is `storage.md`'s, with step 1 here and steps 2 to 7 inside
/// [`Store::convert`]:
///
/// 1. Mint a DEK, wrap it under `Argon2id(PIN, salt)`, DPAPI-protect it, write
///    `keyfile` atomically — **then read it back and assert the recovered DEK
///    equals the one generated.**
/// 2. to 7. Convert the database.
/// 8. Emit `lock_state`, whether or not the reopen succeeded.
/// 9. Sweep what the conversion left. Absorbed.
///
/// **Step 1's read-back is not belt-and-braces.** Key material that cannot be
/// unwrapped is discovered here, where the store is still plaintext and nothing
/// has been lost — rather than after the commit point, where it would leave an
/// encrypted store whose only key does not open it. That is unrecoverable by
/// design: there is no reset and no backdoor
/// ([ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md)).
///
/// The store stays **unlocked** afterwards. The user has just set the PIN; there
/// is nothing to prove (contract, `enable_encryption`).
#[tauri::command(rename_all = "snake_case")]
pub fn enable_encryption<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    pin: Option<String>,
) -> Result<(), ClipError> {
    let pin = Pin::parse("pin", pin)?;

    // **Before the state checks, and therefore before step 1's write.** A
    // second conversion must refuse and return here, without reaching an abort
    // path that would delete the first one's key material
    // ([`Store::begin_conversion`]).
    let _conversion = store.begin_conversion();

    // The store must be unencrypted, and it must be one this process can read.
    // A recorded startup fault is relayed rather than converted over.
    if let Some(fault) = store.startup_fault() {
        return Err(fault);
    }
    if store.encryption_enabled() {
        return Err(ClipError::WrongState {
            required: RequiredState::Unencrypted,
        });
    }

    let paths = store.directory()?.clone();

    // ---- Step 1: the key material, written and then proved readable ----
    let dek = Dek::generate()?;
    if let Err(error) = write_and_prove_key_material(&paths, &pin, &dek) {
        // Abort: the store is untouched and still plaintext, so the key material
        // written a moment ago must not be left beside it — startup recovery
        // would delete it anyway, and leaving it is a file that decrypts
        // nothing.
        discard_key_material(&paths);
        return Err(error);
    }

    // ---- Steps 2 to 7 ----
    let converted = store.convert(Becoming::Encrypted(dek));

    if !converted.committed {
        // Nothing committed, so `keyfile` describes a store that does not exist.
        // It was written before the commit point, which is why deleting it is
        // part of the abort (`storage.md` § Abort).
        discard_key_material(&paths);
        return converted.result;
    }

    // ---- Step 8: emit, whether or not step 7 succeeded ----
    finish(&app, &store);

    // ---- Step 9: absorbed ----
    keyfile::discard_intermediate(&paths);

    converted.result
}

/// Turn encryption off, rewriting the store as plaintext.
///
/// Requires the store unlocked **and** the PIN (spec §4.8): the plaintext export
/// needs the DEK, and the PIN is what proves the user meant it.
///
/// **Not subject to backoff, and it does not touch the unlock attempt counter**
/// (contract, `disable_encryption`). The caller is already unlocked, so a limit
/// on guesses would protect nothing — which is why `bad_pin` from here carries
/// `attempts_remaining: null` and `retry_after_ms: null`.
///
/// `keyfile` is deleted at step 9, **after** the commit point, and a failure to
/// delete it cannot fail this command: the rename already made the store
/// plaintext, and returning `storage` for a refused delete would suppress the
/// emission and leave the settings view claiming encryption is on over a
/// plaintext store.
#[tauri::command(rename_all = "snake_case")]
pub fn disable_encryption<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    pin: Option<String>,
) -> Result<(), ClipError> {
    let pin = Pin::parse("pin", pin)?;

    // As `enable_encryption`: taken before the state checks, so the two
    // conversions serialise with each other as well as with themselves.
    let _conversion = store.begin_conversion();

    if !store.encryption_enabled() {
        return Err(ClipError::WrongState {
            required: RequiredState::Encrypted,
        });
    }
    if store.is_locked() {
        return Err(ClipError::Locked);
    }
    if let Some(fault) = store.startup_fault() {
        return Err(fault);
    }

    let paths = store.directory()?.clone();

    // The PIN is checked against the stored key material rather than against
    // anything held in memory, so it is the same authenticated check `unlock`
    // makes. A wrong PIN here changes nothing at all.
    check_pin(&paths, &pin)?;

    let converted = store.convert(Becoming::Plaintext);

    if !converted.committed {
        return converted.result;
    }

    // ---- Step 8 ----
    finish(&app, &store);

    // ---- Step 9: delete `keyfile`. Absorbed. ----
    if let Err(error) = keyfile::delete(&paths) {
        // The store is plaintext and the event has already said so. The stray
        // file wraps a DEK for a database that no longer exists, so it decrypts
        // nothing, and startup recovery step 5 removes it at the next launch.
        log::warn!(
            "the key material could not be deleted after disabling encryption: {}",
            error.kind()
        );
    }
    keyfile::discard_intermediate(&paths);

    converted.result
}

/// Re-wrap the DEK under a new PIN.
///
/// **The database is not re-encrypted**, so this is instant regardless of clip
/// count ([ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md)).
/// The DEK is the entropy and it does not change; what changes is the Argon2id
/// salt and the wrap around it, which is the whole of the PIN's job.
///
/// **Emits nothing.** No field of `LockState` changes: the store is encrypted
/// before and after, unlocked before and after, and the counter is already clear
/// because the caller must be unlocked to be here.
///
/// Not subject to backoff, on the same reasoning as `disable_encryption`; the
/// caller is already unlocked, so a limit on guesses would protect nothing, and
/// `bad_pin` carries `null` for both fields.
///
/// **The new wrap is proved in memory before the file is touched.** This
/// replaces the only key to the store, so verifying after the write would be
/// verifying after the old key was already gone — the atomic rename makes the
/// swap all-or-nothing, but all-of-a-bad-key is still all. The unwrap below
/// happens against the new material while the old file is still on disk, so a
/// failure costs nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn change_pin(
    store: State<'_, Store>,
    current_pin: Option<String>,
    new_pin: Option<String>,
) -> Result<(), ClipError> {
    let current_pin = Pin::parse("current_pin", current_pin)?;
    let new_pin = Pin::parse("new_pin", new_pin)?;

    if !store.encryption_enabled() {
        return Err(ClipError::WrongState {
            required: RequiredState::Encrypted,
        });
    }
    if store.is_locked() {
        return Err(ClipError::Locked);
    }

    let paths = store.directory()?.clone();
    let material = keyfile::read(&paths)?;

    // The current PIN is checked against the stored key material, which is the
    // same authenticated check `unlock` makes.
    let wrapping = kdf::derive(&current_pin, &material.kdf)?;
    let dek = match wrap::unwrap(&wrapping, &material.wrapped_dek)? {
        Some(dek) => dek,
        None => {
            return Err(ClipError::BadPin {
                attempts_remaining: None,
                retry_after_ms: None,
            })
        }
    };

    // A fresh salt, not only a fresh wrap: two PINs for one store must not share
    // a derivation, or changing the PIN would leave the old Argon2id work
    // reusable against the new one.
    let rewrapped = KeyMaterial::create(&new_pin, &dek)?;
    prove_in_memory(&rewrapped, &new_pin, &dek)?;

    keyfile::write(&paths, &rewrapped)?;

    // Read back, for the reason `enable_encryption` does: key material that
    // cannot be read is better discovered now, while the user is still at the
    // keyboard, than at the next launch.
    let read_back = keyfile::read(&paths)?;
    prove_in_memory(&read_back, &new_pin, &dek)?;

    // The old PIN stops working the moment this returns, which is what the
    // contract promises. Asserted rather than assumed.
    debug_assert!(store.holds_dek(&dek), "change_pin must not change the DEK");
    Ok(())
}

/// Assert that this key material unwraps to this DEK under this PIN.
fn prove_in_memory(material: &KeyMaterial, pin: &Pin, dek: &Dek) -> Result<(), ClipError> {
    let wrapping = kdf::derive(pin, &material.kdf)?;
    match wrap::unwrap(&wrapping, &material.wrapped_dek)? {
        Some(recovered) if &recovered == dek => Ok(()),
        _ => {
            log::error!("the re-wrapped key material does not unwrap to the store's key");
            Err(bad_key_material())
        }
    }
}

/// Step 8 and the tray, for both conversions.
///
/// The tray is rebuilt because lock state is its third rebuild trigger
/// (spec §4.5, spec §4.8) — enabling leaves an unlocked store whose menu still
/// lists clips, and disabling the same, but neither can be assumed and the tray
/// is not an event.
fn finish<R: Runtime>(app: &AppHandle<R>, store: &Store) {
    match lock_state(store) {
        Ok(state) => events::emit_lock_state(app, &state),
        Err(error) => {
            // Unreachable on this path: both conversions leave the store
            // unlocked, and `lock_state` reads `keyfile` only when it is
            // locked. Absorbed rather than propagated — the conversion has
            // committed, and returning an error for a state report would tell
            // the user a conversion that worked had failed.
            log::error!("the lock state could not be derived after a conversion: {error}");
        }
    }
    tray::refresh(app);
}

/// Write `keyfile` and then **prove it can be read back**.
///
/// The assertion is that the DEK recovered from the file equals the one
/// generated — not merely that the file parses. A `keyfile` that parses and
/// unwraps to the wrong key would pass a weaker check and lose the store at the
/// next launch.
fn write_and_prove_key_material(
    paths: &crate::storage::StorePaths,
    pin: &Pin,
    dek: &Dek,
) -> Result<(), ClipError> {
    let material = KeyMaterial::create(pin, dek)?;
    keyfile::write(paths, &material)?;

    let read_back = keyfile::read(paths)?;
    let wrapping = kdf::derive(pin, &read_back.kdf)?;
    match wrap::unwrap(&wrapping, &read_back.wrapped_dek)? {
        Some(recovered) if &recovered == dek => Ok(()),
        Some(_) => {
            // Unreachable: the AEAD verified, so this was wrapped under the same
            // key, and yet the bytes differ. Reported rather than ignored.
            log::error!("the key material read back a different key from the one generated");
            Err(bad_key_material())
        }
        None => {
            log::error!("the key material could not be unwrapped with the PIN that wrote it");
            Err(bad_key_material())
        }
    }
}

/// Check a PIN against the stored key material.
///
/// `bad_pin` here carries `None` for both fields: this command is not subject to
/// backoff and does not touch the counter (contract, `disable_encryption`).
fn check_pin(paths: &crate::storage::StorePaths, pin: &Pin) -> Result<(), ClipError> {
    let material = keyfile::read(paths)?;
    let wrapping = kdf::derive(pin, &material.kdf)?;
    match wrap::unwrap(&wrapping, &material.wrapped_dek)? {
        Some(_) => Ok(()),
        None => Err(ClipError::BadPin {
            attempts_remaining: None,
            retry_after_ms: None,
        }),
    }
}

/// Delete `keyfile` and `keyfile.new` on an abort. Absorbed.
fn discard_key_material(paths: &crate::storage::StorePaths) {
    if let Err(error) = keyfile::delete(paths) {
        log::warn!(
            "the key material could not be deleted after an aborted enable: {}",
            error.kind()
        );
    }
    keyfile::discard_intermediate(paths);
}

fn bad_key_material() -> ClipError {
    ClipError::Crypto {
        reason: crate::error::CryptoReason::BadKeyMaterial,
    }
}
