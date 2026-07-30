//! `~/.fast-clip/keyfile` as a file: written, read back, tampered with, and
//! rewritten (WP-07).
//!
//! **A separate binary, for the same reason `kill_mid_write.rs` is one**, and
//! then for a second reason this project had not met before.
//!
//! The first is ordinary: these exercise the file rather than the logic. They
//! write real DPAPI blobs to real paths, damage them, and read them back, which
//! is the same category as the other tests in `tests/` and a different one from
//! the backoff arithmetic that stays beside the module in `src/crypto/keyfile.rs`.
//!
//! The second is specific to this machine and worth writing down, because the
//! symptom is baffling and costs hours to diagnose twice. **Endpoint security
//! terminates a process that accumulates enough `CryptUnprotectData` calls
//! interleaved with file rewrites** — a credential-harvesting signature. On the
//! development machine that is CrowdStrike Falcon; it kills the process
//! asynchronously with exit code `0xE0000027` and, for one build, deleted the
//! test binary from disk outright. Symptoms, so the next person recognises it:
//!
//! - The failure is a process abort with **no panic message and no stack**.
//! - It is **layout-sensitive**: adding an unrelated test, or an `eprintln!`,
//!   makes it disappear, because the kill is asynchronous and the process
//!   sometimes exits first.
//! - It is **cumulative per process**, not per test: every subset passes and the
//!   whole binary does not.
//! - `cargo` may also report `Access is denied (os error 5)` when it tries to
//!   run a freshly linked test binary.
//!
//! Splitting these out puts the library's unit-test binary back under the
//! threshold. It is a mitigation, not a fix: the fix is an exclusion for this
//! repository's `target/` directory, which only the machine's owner can apply.
//! Nothing here is a defect in FastClip, and the shipped application is not
//! affected — a user makes one unlock attempt every few seconds, not forty in a
//! second.

use std::fs;

use fast_clip_lib::crypto::kdf::{self, KdfParams, MIN_M_COST_KIB, MIN_T_COST, P_COST, SALT_LEN};
use fast_clip_lib::crypto::keyfile::{self, KEYFILE_VERSION, MAX_ATTEMPTS};
use fast_clip_lib::crypto::{dpapi, wrap, Dek, KeyMaterial, Pin};
use fast_clip_lib::error::{ClipError, CryptoReason, VersionComponent};
use fast_clip_lib::storage::StorePaths;

use keyfile::{delete, now_unix_ms, read, write};

fn bad_key_material() -> ClipError {
    ClipError::Crypto {
        reason: CryptoReason::BadKeyMaterial,
    }
}

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

fn pin(value: &str) -> Pin {
    match Pin::parse("pin", Some(value.to_owned())) {
        Ok(pin) => pin,
        Err(e) => panic!("the fixture PIN should parse: {e}"),
    }
}

/// The KDF at its binding floor rather than the shipped tuple: these tests
/// exercise the file, not the cost.
fn cheap_material(pin_value: &str, dek: &Dek) -> KeyMaterial {
    let kdf_params = KdfParams {
        m_cost_kib: MIN_M_COST_KIB,
        t_cost: MIN_T_COST,
        p_cost: P_COST,
        salt: [0x11; SALT_LEN],
    };
    let key = match kdf::derive(&pin(pin_value), &kdf_params) {
        Ok(key) => key,
        Err(e) => panic!("the key should derive: {e}"),
    };
    let wrapped_dek = match wrap::wrap(&key, dek) {
        Ok(wrapped) => wrapped,
        Err(e) => panic!("the DEK should wrap: {e}"),
    };
    KeyMaterial {
        kdf: kdf_params,
        wrapped_dek,
        failed_attempts: 0,
        locked_until_unix_ms: None,
    }
}

#[test]
fn key_material_round_trips_through_the_file() {
    let parent = dir();
    let paths = paths_in(&parent);
    let material = cheap_material("123456", &Dek::from_bytes([0x2b; 32]));

    match write(&paths, &material) {
        Ok(()) => {}
        Err(e) => panic!("the key material should write: {e}"),
    }
    match read(&paths) {
        Ok(back) => assert_eq!(back, material),
        Err(e) => panic!("the key material should read back: {e}"),
    }
}

#[test]
fn the_file_begins_with_the_version_byte_and_the_rest_is_not_the_json() {
    let parent = dir();
    let paths = paths_in(&parent);
    let material = cheap_material("123456", &Dek::from_bytes([0x2b; 32]));
    if let Err(e) = write(&paths, &material) {
        panic!("the key material should write: {e}");
    }

    let bytes = match fs::read(paths.keyfile()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the key material should be readable as bytes: {e}"),
    };
    assert_eq!(bytes.first(), Some(&KEYFILE_VERSION));

    // The salt is inside the DPAPI blob, so it must not be visible in the
    // file. This is the outer factor doing its job.
    assert!(
        !bytes
            .windows(SALT_LEN)
            .any(|window| window == material.kdf.salt),
        "the salt appears in the file outside the DPAPI blob"
    );
    assert!(
        !bytes
            .windows(material.wrapped_dek.ciphertext.len())
            .any(|window| window == material.wrapped_dek.ciphertext),
        "the wrapped DEK appears in the file outside the DPAPI blob"
    );
}

/// **The write is atomic, and the rename is what makes it so.** The
/// intermediate is gone afterwards, and it is in the sweep list, so a crash
/// between the two leaves nothing a reader would consult.
#[test]
fn a_completed_write_leaves_no_intermediate_and_the_intermediate_is_swept() {
    let parent = dir();
    let paths = paths_in(&parent);
    if let Err(e) = write(
        &paths,
        &cheap_material("123456", &Dek::from_bytes([0x2b; 32])),
    ) {
        panic!("the key material should write: {e}");
    }

    assert!(paths.keyfile().exists());
    assert!(!paths.keyfile_new().exists());
    assert!(
        paths.intermediates().contains(&paths.keyfile_new()),
        "keyfile.new must be swept at the next launch"
    );
}

/// **The one that matters: a rewrite never destroys the previous file.** A
/// failed `unlock` rewrites this file to record the attempt, so if that
/// rewrite were in place a mistyped PIN could destroy the store.
#[test]
fn rewriting_repeatedly_always_leaves_a_readable_file() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = Dek::from_bytes([0x2b; 32]);
    let mut material = cheap_material("123456", &dek);

    for attempt in 1..=4u32 {
        material.record_failure(1_000);
        match write(&paths, &material) {
            Ok(()) => {}
            Err(e) => panic!("rewrite {attempt} should succeed: {e}"),
        }
        match read(&paths) {
            Ok(back) => {
                assert_eq!(back.failed_attempts, attempt);
                assert_eq!(back.wrapped_dek, material.wrapped_dek, "the DEK survived");
            }
            Err(e) => panic!("the file should still read after rewrite {attempt}: {e}"),
        }
    }
}

#[test]
fn an_absent_or_empty_file_is_bad_key_material() {
    let parent = dir();
    let paths = paths_in(&parent);
    assert_eq!(read(&paths), Err(bad_key_material()));

    if let Err(e) = fs::write(paths.keyfile(), []) {
        panic!("the fixture should write: {e}");
    }
    assert_eq!(read(&paths), Err(bad_key_material()));
}

/// `storage.md` § Version 0: a leading zero is a file no FastClip wrote, not
/// version zero of a format.
#[test]
fn a_zero_version_byte_is_bad_key_material_rather_than_a_version_error() {
    let parent = dir();
    let paths = paths_in(&parent);
    if let Err(e) = fs::write(paths.keyfile(), [0u8; 128]) {
        panic!("the fixture should write: {e}");
    }
    assert_eq!(read(&paths), Err(bad_key_material()));
}

/// Contract §2: every reader of `keyfile` declares this variant, because the
/// version byte is outside the blob precisely so it is parsed first.
#[test]
fn a_newer_version_byte_is_an_unsupported_version_not_a_dpapi_failure() {
    let parent = dir();
    let paths = paths_in(&parent);
    let material = cheap_material("123456", &Dek::from_bytes([0x2b; 32]));
    if let Err(e) = write(&paths, &material) {
        panic!("the key material should write: {e}");
    }

    let mut bytes = match fs::read(paths.keyfile()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the file should be readable: {e}"),
    };
    bytes[0] = KEYFILE_VERSION + 1;
    if let Err(e) = fs::write(paths.keyfile(), &bytes) {
        panic!("the fixture should write: {e}");
    }

    assert_eq!(
        read(&paths),
        Err(ClipError::UnsupportedVersion {
            component: VersionComponent::KeyMaterial,
            found: i64::from(KEYFILE_VERSION) + 1,
            supported: i64::from(KEYFILE_VERSION),
        })
    );
}

#[test]
fn a_blob_that_is_not_dpapi_is_bad_key_material() {
    let parent = dir();
    let paths = paths_in(&parent);
    let mut bytes = vec![KEYFILE_VERSION];
    bytes.extend_from_slice(b"this is not a DPAPI blob");
    if let Err(e) = fs::write(paths.keyfile(), &bytes) {
        panic!("the fixture should write: {e}");
    }
    assert_eq!(read(&paths), Err(bad_key_material()));
}

/// The parameters are validated on the way out of `read`, so no caller has
/// to remember to do it.
#[test]
fn a_file_carrying_parameters_below_the_floor_is_refused_on_read() {
    let parent = dir();
    let paths = paths_in(&parent);
    let mut material = cheap_material("123456", &Dek::from_bytes([0x2b; 32]));
    material.kdf.m_cost_kib = 8;
    if let Err(e) = write(&paths, &material) {
        panic!("the fixture should write: {e}");
    }
    assert_eq!(read(&paths), Err(bad_key_material()));
}

/// Migrations are forgiving on input: a field a later build added is
/// ignored, and fields it has not written yet default.
#[test]
fn a_blob_from_a_later_build_reads_rather_than_failing() {
    let parent = dir();
    let paths = paths_in(&parent);
    let material = cheap_material("123456", &Dek::from_bytes([0x2b; 32]));

    let json = match serde_json::to_string(&material) {
        Ok(json) => json,
        Err(e) => panic!("the material should serialise: {e}"),
    };
    let widened = json.replace(
        "\"failed_attempts\"",
        "\"unknown_from_v2\":[1,2,3],\"failed_attempts\"",
    );
    let protected = match dpapi::protect(widened.as_bytes()) {
        Ok(protected) => protected,
        Err(e) => panic!("DPAPI should protect: {e}"),
    };
    let mut bytes = vec![KEYFILE_VERSION];
    bytes.extend_from_slice(&protected);
    if let Err(e) = fs::write(paths.keyfile(), &bytes) {
        panic!("the fixture should write: {e}");
    }

    match read(&paths) {
        Ok(back) => assert_eq!(back, material),
        Err(e) => panic!("an unknown field must not be fatal: {e}"),
    }
}

#[test]
fn a_blob_missing_the_counters_defaults_them_rather_than_failing() {
    let parent = dir();
    let paths = paths_in(&parent);
    let material = cheap_material("123456", &Dek::from_bytes([0x2b; 32]));

    let kdf = match serde_json::to_string(&material.kdf) {
        Ok(json) => json,
        Err(e) => panic!("serialise: {e}"),
    };
    let wrapped = match serde_json::to_string(&material.wrapped_dek) {
        Ok(json) => json,
        Err(e) => panic!("serialise: {e}"),
    };
    let minimal = format!(r#"{{"kdf":{kdf},"wrapped_dek":{wrapped}}}"#);
    let protected = match dpapi::protect(minimal.as_bytes()) {
        Ok(protected) => protected,
        Err(e) => panic!("DPAPI should protect: {e}"),
    };
    let mut bytes = vec![KEYFILE_VERSION];
    bytes.extend_from_slice(&protected);
    if let Err(e) = fs::write(paths.keyfile(), &bytes) {
        panic!("the fixture should write: {e}");
    }

    match read(&paths) {
        Ok(back) => {
            assert_eq!(back.failed_attempts, 0);
            assert_eq!(back.locked_until_unix_ms, None);
            assert_eq!(back.attempts_remaining(), MAX_ATTEMPTS);
        }
        Err(e) => panic!("absent counters must default: {e}"),
    }
}

#[test]
fn deleting_an_absent_keyfile_is_not_an_error() {
    let parent = dir();
    let paths = paths_in(&parent);
    assert!(delete(&paths).is_ok());

    if let Err(e) = write(
        &paths,
        &cheap_material("123456", &Dek::from_bytes([0x2b; 32])),
    ) {
        panic!("the key material should write: {e}");
    }
    assert!(delete(&paths).is_ok());
    assert!(!paths.keyfile().exists());
}

#[test]
fn the_debug_of_the_material_redacts_the_salt_and_the_wrap() {
    let material = cheap_material("123456", &Dek::from_bytes([0xAB; 32]));
    let rendered = format!("{material:?}");
    assert!(
        !rendered.contains(&format!("{:?}", material.kdf.salt)),
        "{rendered}"
    );
    assert!(
        !rendered.contains(&format!("{:?}", material.wrapped_dek.ciphertext)),
        "{rendered}"
    );
    assert!(
        rendered.contains("failed_attempts"),
        "counters are not secret"
    );
}

/// **Nothing is ever wiped after failed attempts** (ADR-0004). Wrong PIN
/// after wrong PIN, the wrapped DEK is byte-for-byte what it was and the
/// right PIN still recovers it.
///
/// **Seven attempts rather than twenty, and the number is not arbitrary.**
/// Seven crosses the five-attempt threshold with margin, which is all the
/// property needs. Twenty was the original figure and it made this test a
/// tight loop of forty `CryptUnprotectData`/`CryptProtectData` calls with a
/// file rewrite between each — which is a credential-harvesting signature,
/// and endpoint security on the development machine terminated the test
/// process for it (CrowdStrike Falcon, asynchronously, with exit code
/// `0xE0000027`). A real user makes one attempt every few seconds, so the
/// shipped application does not look like this; only the test did.
#[test]
fn repeated_failures_destroy_nothing_and_the_right_pin_still_works() {
    let parent = dir();
    let paths = paths_in(&parent);
    let dek = Dek::from_bytes([0x2b; 32]);
    let original = cheap_material("123456", &dek);
    if let Err(e) = write(&paths, &original) {
        panic!("the key material should write: {e}");
    }

    for _ in 0..7 {
        let mut material = match read(&paths) {
            Ok(material) => material,
            Err(e) => panic!("the file should read: {e}"),
        };
        material.record_failure(now_unix_ms());
        if let Err(e) = write(&paths, &material) {
            panic!("the counter should persist: {e}");
        }
    }

    let after = match read(&paths) {
        Ok(material) => material,
        Err(e) => panic!("the file should still read: {e}"),
    };
    assert_eq!(after.failed_attempts, 7);
    assert_eq!(
        after.wrapped_dek, original.wrapped_dek,
        "the wrapped DEK must be untouched by failed attempts"
    );
    assert_eq!(after.kdf, original.kdf);

    let key = match kdf::derive(&pin("123456"), &after.kdf) {
        Ok(key) => key,
        Err(e) => panic!("the key should derive: {e}"),
    };
    assert_eq!(
        wrap::unwrap(&key, &after.wrapped_dek),
        Ok(Some(dek)),
        "the right PIN must still open the store after seven wrong ones"
    );
}
