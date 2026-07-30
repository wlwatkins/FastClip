//! **The acceptance test for [ADR-0004](../../docs/src/architecture/adr/0004-optional-pin-encryption.md).**
//!
//! WP-07 names this as the one test that catches a design which only looks
//! encrypted, and it carries a `BLOCK` in the critic's instruction: *"Can the
//! store be opened with only one of the two factors?"*
//!
//! Opening an encrypted store requires **three** things — the file, the Windows
//! account, and the PIN — and this file constructs the two attackers who each
//! hold all but one:
//!
//! | Attacker | Holds | Lacks | Must get |
//! | -------- | ----- | ----- | -------- |
//! | Took the file to another machine | `clips.db`, `keyfile`, and the PIN | the Windows account, so DPAPI will not unprotect the blob | nothing |
//! | Running as the user, guessing | the Windows account, so DPAPI *will* unprotect the blob | the PIN | nothing |
//!
//! The failure this guards against is a construction where DPAPI alone unwraps
//! the DEK — which makes the PIN decoration — or where the PIN alone derives it,
//! which turns 10⁶ guesses into an offline attack. Both pass a casual reading of
//! the code. Neither passes this file.
//!
//! **What this cannot do, stated rather than implied.** A test process runs as
//! one Windows user, so "another Windows account" cannot be created here. The
//! first attacker is modelled two ways instead — the blob **absent**, which is
//! exactly the case the work package names, and the blob **present but not
//! unprotectable**, which is what DPAPI refusing looks like from this side of the
//! call. Neither is the real thing; both exercise the code path the real thing
//! would take.
//!
//! **[`attempt_unlock`] is a hand-reassembly of `commands::lock::unlock`'s key
//! recovery, not a call to it.** `unlock` exists now (`src/commands/lock.rs`),
//! but its signature is `unlock<R: Runtime>(AppHandle<R>, State<'_, Store>,
//! Option<String>)`, so reaching it means building a Tauri application and
//! invoking it through the mock runtime — the machinery `tests/ipc.rs` owns,
//! because it is the only test binary linking Tauri's application machinery
//! (see that file's header). Duplicating that machinery here to reach one
//! command would pull a two-factor property test into an IPC harness for no
//! reason the property needs: this file is about whether the file, the account
//! and the PIN are all three required to recover a DEK, not about the seam
//! `unlock` is served through — that is `tests/ipc.rs`'s job, and it already
//! drives the real command (`five_wrong_pins_arm_a_flat_thirty_second_wait_and_destroy_nothing`,
//! `an_unlock_reopens_the_store_and_rebuilds_the_tray_menu`, and others there).
//!
//! So [`attempt_unlock`] below calls the same functions `unlock` calls, in the
//! same order, checked against `unlock`'s current body each time this file is
//! touched: `keyfile::read`, `kdf::derive`, `wrap::unwrap`,
//! `connection::open_encrypted`. It omits `unlock`'s state checks
//! (`wrong_state`), its backoff gate, its event emissions and its tray
//! rebuild — none of which bears on whether two factors are both required, and
//! all of which are exercised at the seam in `tests/ipc.rs` instead. If
//! `unlock`'s key-recovery steps ever change, this file and that command can
//! drift silently; nothing but re-reading `unlock` catches it, which is the
//! cost of the reassembly and the reason this paragraph names it rather than
//! leaving the fact implicit.

use std::path::Path;

use fast_clip_lib::crypto::kdf::{self, KdfParams, MIN_M_COST_KIB, MIN_T_COST, P_COST};
use fast_clip_lib::crypto::{keyfile, wrap, Dek, KeyMaterial, Pin};
use fast_clip_lib::error::{ClipError, CryptoReason};
use fast_clip_lib::storage::{connection, StorePaths};

/// Why an unlock attempt did not produce a usable store.
///
/// The three are kept apart because contract §2 keeps them apart: a wrong PIN
/// counts an attempt and says "try again", bad key material says the store
/// belongs to another Windows account, and a database that will not open says
/// re-import. Collapsing any two would tell a user the wrong thing.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// The key material could not be read or unprotected.
    KeyMaterial(ClipError),
    /// The key material was read, and this PIN does not unwrap it.
    WrongPin,
    /// The DEK was recovered and the database still would not open.
    Database(ClipError),
}

/// The `unlock` sequence, exactly as contract §2 specifies it.
///
/// Read `keyfile` and its version byte, unwrap the DEK with the PIN, open the
/// database with the DEK. **Every step is required and there is no shortcut**;
/// that is the property under test.
fn attempt_unlock(paths: &StorePaths, pin: &Pin) -> Result<rusqlite::Connection, Failure> {
    // Factor one: the Windows account. `read` unprotects the DPAPI blob.
    let material = keyfile::read(paths).map_err(Failure::KeyMaterial)?;

    // Factor two: the PIN. Argon2id over it produces the wrapping key.
    let wrapping = kdf::derive(pin, &material.kdf).map_err(Failure::KeyMaterial)?;

    // The AEAD tag decides. `Ok(None)` is a wrong PIN, `Err` is a broken file.
    let dek = match wrap::unwrap(&wrapping, &material.wrapped_dek) {
        Ok(Some(dek)) => dek,
        Ok(None) => return Err(Failure::WrongPin),
        Err(error) => return Err(Failure::KeyMaterial(error)),
    };

    connection::open_encrypted(&paths.db(), &dek).map_err(Failure::Database)
}

/// [`attempt_unlock`] with the connection dropped, so an outcome can be
/// compared. `rusqlite::Connection` is not `PartialEq`, and a test that only
/// cares *whether* the store opened should not have to say so twice.
fn unlock_outcome(paths: &StorePaths, pin: &Pin) -> Result<(), Failure> {
    attempt_unlock(paths, pin).map(drop)
}

fn dir() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    }
}

fn pin(value: &str) -> Pin {
    match Pin::parse("pin", Some(value.to_owned())) {
        Ok(pin) => pin,
        Err(e) => panic!("the fixture PIN should parse: {e}"),
    }
}

/// The KDF at its binding floor rather than the shipped tuple.
///
/// The shipped tuple costs 250–500 ms per derive by design, and these tests
/// derive many times — one of them five times in a loop. The floor exercises the
/// identical code path at a cost the suite can afford, and
/// [`the_whole_construction_holds_at_the_tuple_this_build_actually_ships`] runs
/// the real one once so the shipped parameters are not left untested.
fn cheap_params() -> KdfParams {
    let mut params = match KdfParams::generate() {
        Ok(params) => params,
        Err(e) => panic!("parameters should generate: {e}"),
    };
    params.m_cost_kib = MIN_M_COST_KIB;
    params.t_cost = MIN_T_COST;
    params.p_cost = P_COST;
    params
}

fn material_with(params: KdfParams, pin_value: &str, dek: &Dek) -> KeyMaterial {
    let wrapping = match kdf::derive(&pin(pin_value), &params) {
        Ok(key) => key,
        Err(e) => panic!("the wrapping key should derive: {e}"),
    };
    let wrapped_dek = match wrap::wrap(&wrapping, dek) {
        Ok(wrapped) => wrapped,
        Err(e) => panic!("the DEK should wrap: {e}"),
    };
    KeyMaterial {
        kdf: params,
        wrapped_dek,
        failed_attempts: 0,
        locked_until_unix_ms: None,
    }
}

/// A distinctive clip value, so a test can assert it is *not* in a file.
const SECRET: &str = "two-factor-acceptance-9c1f4a-plaintext-must-not-appear";

/// Build an encrypted store holding one clip, the way `enable_encryption` will.
///
/// Returns the paths and the DEK. The DEK is returned only so a test can prove
/// the store *is* openable with it — no production path ever has it without
/// both factors.
fn encrypted_store(
    parent: &tempfile::TempDir,
    pin_value: &str,
    params: KdfParams,
) -> (StorePaths, Dek) {
    let paths = StorePaths::at(parent.path().join(".fast-clip"));
    if let Err(e) = std::fs::create_dir_all(paths.dir()) {
        panic!("could not create the fixture directory: {e}");
    }

    let dek = match Dek::generate() {
        Ok(dek) => dek,
        Err(e) => panic!("a DEK should generate: {e}"),
    };

    let material = material_with(params, pin_value, &dek);
    if let Err(e) = keyfile::write(&paths, &material) {
        panic!("the key material should write: {e}");
    }

    let connection = match connection::open_encrypted(&paths.db(), &dek) {
        Ok(connection) => connection,
        Err(e) => panic!("the encrypted store should open: {e}"),
    };
    if let Err(e) =
        connection.execute_batch("CREATE TABLE clips (id INTEGER PRIMARY KEY, value TEXT);")
    {
        panic!("the fixture schema should create: {e}");
    }
    if let Err(e) = connection.execute("INSERT INTO clips (value) VALUES (?1)", [SECRET]) {
        panic!("the fixture clip should insert: {e}");
    }
    connection::checkpoint_and_close(connection);

    (paths, dek)
}

fn stored_value(connection: &rusqlite::Connection) -> Result<String, rusqlite::Error> {
    connection.query_row("SELECT value FROM clips", [], |row| row.get(0))
}

/// The control. Both factors present, and the store opens and reads.
///
/// Without this the rest of the file could pass because the store never worked.
#[test]
fn both_factors_together_open_the_store() {
    let parent = dir();
    let (paths, _dek) = encrypted_store(&parent, "123456", cheap_params());

    match attempt_unlock(&paths, &pin("123456")) {
        Ok(connection) => assert_eq!(stored_value(&connection), Ok(SECRET.to_string())),
        Err(failure) => panic!("both factors should open the store: {failure:?}"),
    }
}

/// **Attacker one: the PIN is known and the DPAPI blob is absent.**
///
/// The work package names this case in so many words. Knowing the PIN is worth
/// nothing without the blob, because the blob is where the salt and the wrapped
/// DEK live — there is no path from a PIN to a DEK.
#[test]
fn the_right_pin_cannot_open_the_store_when_the_key_material_is_absent() {
    let parent = dir();
    let (paths, _dek) = encrypted_store(&parent, "123456", cheap_params());

    if let Err(e) = std::fs::remove_file(paths.keyfile()) {
        panic!("the fixture keyfile should be removable: {e}");
    }

    assert_eq!(
        unlock_outcome(&paths, &pin("123456")),
        Err(Failure::KeyMaterial(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial
        })),
        "the correct PIN opened a store whose key material is gone"
    );
}

/// The same attacker, with the file in hand rather than deleted — which is what
/// carrying `~/.fast-clip/` to another machine actually looks like. DPAPI
/// refuses, and the PIN never gets a chance to be applied.
#[test]
fn the_right_pin_cannot_open_the_store_when_dpapi_will_not_unprotect_the_blob() {
    let parent = dir();
    let (paths, _dek) = encrypted_store(&parent, "123456", cheap_params());

    let mut bytes = match std::fs::read(paths.keyfile()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the fixture keyfile should be readable: {e}"),
    };
    // Damage the protected blob, leaving the version byte intact. DPAPI
    // authenticates its own ciphertext, so this is refused exactly as a blob
    // from another account is.
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    if let Err(e) = std::fs::write(paths.keyfile(), &bytes) {
        panic!("the fixture should write: {e}");
    }

    assert_eq!(
        unlock_outcome(&paths, &pin("123456")),
        Err(Failure::KeyMaterial(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial
        })),
        "the correct PIN opened a store whose blob DPAPI refused"
    );
}

/// **Attacker two: DPAPI is available and the PIN is wrong.**
///
/// This runs as the Windows user, so the blob unprotects and the salt and
/// wrapped DEK are in hand. Every wrong PIN still fails, and it fails at the
/// AEAD tag — an authenticated no, which is what makes `attempts_remaining`
/// exact rather than a guess about whether the output looks like a key.
#[test]
fn dpapi_alone_does_not_open_the_store_without_the_pin() {
    let parent = dir();
    let (paths, _dek) = encrypted_store(&parent, "123456", cheap_params());

    for wrong in ["000000", "123457", "654321", "111111", "999999"] {
        assert_eq!(
            unlock_outcome(&paths, &pin(wrong)),
            Err(Failure::WrongPin),
            "the PIN {wrong} opened a store it should not have"
        );
    }
}

/// The same attacker, one level deeper: **unprotecting the blob yields the
/// wrapped DEK, not the DEK.**
///
/// The previous test could pass if `attempt_unlock` merely refused early. This
/// asserts the underlying fact — that everything DPAPI hands back is still
/// sealed under the PIN, and that the DEK's bytes appear nowhere in it.
#[test]
fn everything_inside_the_dpapi_blob_is_still_sealed_under_the_pin() {
    let parent = dir();
    let (paths, dek) = encrypted_store(&parent, "123456", cheap_params());

    // What an attacker running as the user gets: the whole key material.
    let material = match keyfile::read(&paths) {
        Ok(material) => material,
        Err(e) => panic!("this account should be able to read its own key material: {e}"),
    };

    // The DEK is not in it in the clear — not in the wrap, not in the salt.
    assert!(
        !material
            .wrapped_dek
            .ciphertext
            .windows(32)
            .any(|window| window == dek.expose()),
        "the DEK appears in the wrapped blob in the clear"
    );
    assert_ne!(&material.kdf.salt[..], &dek.expose()[..16]);

    // And the wrap does not open without the right PIN.
    for wrong in ["000000", "123457"] {
        let wrapping = match kdf::derive(&pin(wrong), &material.kdf) {
            Ok(key) => key,
            Err(e) => panic!("the wrapping key should derive: {e}"),
        };
        assert_eq!(wrap::unwrap(&wrapping, &material.wrapped_dek), Ok(None));
    }

    let right = match kdf::derive(&pin("123456"), &material.kdf) {
        Ok(key) => key,
        Err(e) => panic!("the wrapping key should derive: {e}"),
    };
    assert_eq!(wrap::unwrap(&right, &material.wrapped_dek), Ok(Some(dek)));
}

/// **The PIN is a gate, not the entropy** — the sentence ADR-0004 turns on.
///
/// If the PIN derived the store's key, then the PIN plus public parameters would
/// be enough to open the database, and copying the file would reduce the whole
/// scheme to 10⁶ offline guesses. It does not: the key Argon2id produces from
/// the PIN opens nothing, because it is not the store's key.
#[test]
fn a_key_derived_from_the_pin_is_not_the_key_the_store_is_encrypted_with() {
    let parent = dir();
    let (paths, dek) = encrypted_store(&parent, "123456", cheap_params());

    let material = match keyfile::read(&paths) {
        Ok(material) => material,
        Err(e) => panic!("the key material should read: {e}"),
    };
    let wrapping = match kdf::derive(&pin("123456"), &material.kdf) {
        Ok(key) => key,
        Err(e) => panic!("the wrapping key should derive: {e}"),
    };

    assert_ne!(
        wrapping.expose(),
        dek.expose(),
        "the PIN-derived key IS the store key: the PIN is the entropy, which ADR-0004 forbids"
    );

    // And it does not open the database either.
    let as_if_it_were_the_dek = Dek::from_bytes(*wrapping.expose());
    assert!(
        connection::open_encrypted(&paths.db(), &as_if_it_were_the_dek).is_err(),
        "the PIN-derived key opened the store directly"
    );
}

/// **Attacker three, who holds only the file.** No account, no PIN, no tooling.
/// Acceptance criterion 5: the store is unreadable in a text editor.
#[test]
fn the_database_file_discloses_nothing_to_someone_holding_only_it() {
    let parent = dir();
    let (paths, _dek) = encrypted_store(&parent, "123456", cheap_params());

    let bytes = match std::fs::read(paths.db()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the store should be readable as bytes: {e}"),
    };
    assert!(
        !bytes
            .windows(SECRET.len())
            .any(|window| window == SECRET.as_bytes()),
        "the clip value is in the encrypted store in the clear"
    );
    assert!(
        !bytes.starts_with(b"SQLite format 3\0"),
        "the store carries a plaintext SQLite header"
    );

    // Opening with no key at all fails.
    assert!(connection::open(&paths.db()).is_err());

    // And the sidecars, which SQLCipher encrypts too. This is what makes `lock`
    // safe to absorb a failed checkpoint (ADR-0010).
    for sidecar in [paths.db_wal(), paths.db_shm()] {
        if let Ok(bytes) = std::fs::read(&sidecar) {
            assert!(
                !bytes
                    .windows(SECRET.len())
                    .any(|window| window == SECRET.as_bytes()),
                "{} holds the clip value in the clear",
                sidecar.display()
            );
        }
    }
}

/// Neither the clip value nor the DEK is anywhere in `~/.fast-clip/`, in any
/// file, once the store is encrypted. A sweep rather than a named-file check, so
/// a future file added to that directory is covered without anyone remembering.
#[test]
fn no_file_in_the_store_directory_holds_the_clip_value_or_the_key() {
    let parent = dir();
    let (paths, dek) = encrypted_store(&parent, "123456", cheap_params());

    let entries = match std::fs::read_dir(paths.dir()) {
        Ok(entries) => entries,
        Err(e) => panic!("the store directory should be readable: {e}"),
    };

    let mut checked = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => panic!("a directory entry should be readable: {e}"),
        };
        let bytes = match std::fs::read(entry.path()) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        checked += 1;

        assert!(
            !bytes
                .windows(SECRET.len())
                .any(|window| window == SECRET.as_bytes()),
            "{} holds the clip value in the clear",
            entry.path().display()
        );
        assert!(
            !bytes.windows(32).any(|window| window == dek.expose()),
            "{} holds the data encryption key in the clear",
            entry.path().display()
        );
    }
    assert!(
        checked >= 2,
        "the sweep should have seen clips.db and keyfile"
    );
}

/// The floor is used everywhere above for speed. This runs the tuple the build
/// actually ships, once, end to end — so the shipped parameters are covered by
/// something rather than only by the constant that declares them.
#[test]
fn the_whole_construction_holds_at_the_tuple_this_build_actually_ships() {
    let parent = dir();
    let params = match KdfParams::generate() {
        Ok(params) => params,
        Err(e) => panic!("parameters should generate: {e}"),
    };
    assert_eq!(params.m_cost_kib, kdf::DEFAULT_M_COST_KIB);
    assert_eq!(params.t_cost, kdf::DEFAULT_T_COST);

    let (paths, _dek) = encrypted_store(&parent, "246810", params);

    match attempt_unlock(&paths, &pin("246810")) {
        Ok(connection) => assert_eq!(stored_value(&connection), Ok(SECRET.to_string())),
        Err(failure) => panic!("the shipped tuple should open the store: {failure:?}"),
    }
    assert_eq!(
        unlock_outcome(&paths, &pin("246811")),
        Err(Failure::WrongPin)
    );
}

/// The salt is per-store, so two stores with the **same PIN** have different
/// wrapped DEKs and neither one's key material opens the other. Without this a
/// precomputed table over 10⁶ PINs would work against every FastClip install.
#[test]
fn two_stores_with_the_same_pin_do_not_share_key_material() {
    let first_parent = dir();
    let second_parent = dir();
    let (first, _) = encrypted_store(&first_parent, "123456", cheap_params());
    let (second, _) = encrypted_store(&second_parent, "123456", cheap_params());

    let read_material = |paths: &StorePaths| match keyfile::read(paths) {
        Ok(material) => material,
        Err(e) => panic!("the key material should read: {e}"),
    };
    let a = read_material(&first);
    let b = read_material(&second);

    assert_ne!(a.kdf.salt, b.kdf.salt, "the salt must be per-store");
    assert_ne!(a.wrapped_dek, b.wrapped_dek);

    // Swapping one store's key material onto the other does not open it: the
    // DEK it unwraps is the wrong store's.
    if let Err(e) = std::fs::copy(first.keyfile(), second.keyfile()) {
        panic!("the fixture should copy: {e}");
    }
    match attempt_unlock(&second, &pin("123456")) {
        Err(Failure::Database(ClipError::Crypto {
            reason: CryptoReason::Corrupt,
        })) => {}
        other => panic!("another store's key material opened this one: {other:?}"),
    }
}

/// A file's path is not a factor, but this asserts the obvious complement: the
/// encrypted database is bound to its key and not to its location.
#[test]
fn a_moved_store_still_needs_both_factors() {
    let parent = dir();
    let (paths, dek) = encrypted_store(&parent, "123456", cheap_params());

    let elsewhere = dir();
    let moved: &Path = elsewhere.path();
    let target = moved.join("clips.db");
    if let Err(e) = std::fs::copy(paths.db(), &target) {
        panic!("the fixture should copy: {e}");
    }

    assert!(connection::open(&target).is_err(), "no key, no store");
    match connection::open_encrypted(&target, &dek) {
        Ok(connection) => assert_eq!(stored_value(&connection), Ok(SECRET.to_string())),
        Err(e) => panic!("the DEK should still open its own database: {e}"),
    }
}
