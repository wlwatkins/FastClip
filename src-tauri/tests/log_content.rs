//! Spec §8 criterion 6, proved rather than inspected: "No clip `value` appears
//! in any log, in stdout, or in any file other than the encrypted store and a
//! user-initiated export." ADR-0012 and WP-02's addendum name this test in so
//! many words — create, copy, edit and delete a clip carrying a distinctive
//! value, then grep the log file and stdout for it and find nothing.
//!
//! **A subprocess, on the same reasoning `kill_mid_write.rs` gives.** The log
//! sink writes to the process's real stdout handle (`fern`'s `Stdout` target is
//! `std::io::stdout()` directly), which bypasses `cargo test`'s own output
//! capture — that capture only intercepts the `print!`/`println!` macros, not a
//! raw write to the OS handle. Running the exercised code in a child and
//! reading back its captured `Command::output()` is what lets this test see
//! exactly what a released, windowless FastClip would have written, rather than
//! reasoning about what `fern` does with a handle this process does not fully
//! own.
//!
//! **Built from the production wiring, not a copy of it.** The child calls
//! `fast_clip_lib::log_plugin`, the exact function `lib.rs::run` registers on
//! the builder, against a temporary directory standing in for `~/.fast-clip`.
//! What is under test is that function and the call sites beneath it, not a
//! second logger this file invents.

use std::process::Command;

use fast_clip_lib::commands::clipboard::ClipboardWriter;
use fast_clip_lib::storage::{clips, StorePaths};
use fast_clip_lib::{ClipError, Colour, Store};

/// Render bytes as lowercase hex, which is the shape a DEK reaches a log in.
///
/// `PRAGMA key = "x'<64 hex>'"` is the statement that carries it, so hex is what
/// a leak looks like — not the raw bytes.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut rendered = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

/// Set by the parent to tell the child which directory to use. Its absence is
/// what makes the child test a no-op in an ordinary run, exactly as
/// `kill_mid_write.rs`'s `DIR_VAR` does.
const DIR_VAR: &str = "FASTCLIP_LOG_TEST_DIR";

/// Never sent to the clipboard for real. Recording it here, not writing it
/// anywhere, is what lets the child avoid touching whatever the test machine's
/// clipboard currently holds.
struct DiscardingClipboard;

impl ClipboardWriter for DiscardingClipboard {
    fn write_text(&self, _text: &str) -> Result<(), ClipError> {
        Ok(())
    }
}

/// A value distinctive enough that it cannot appear in this file's own prose,
/// in a timestamp, in a UUID, or in any diagnostic line the application writes
/// about itself — so a match is unambiguously the content rule breaking.
const DISTINCTIVE_VALUE: &str = "xyzzy-criterion-6-fde8b4c1-do-not-log-this";

#[test]
fn a_clip_s_value_never_reaches_the_log_file_or_stdout() {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => panic!("could not find the test binary: {e}"),
    };

    let output = Command::new(exe)
        .args([
            "the_child_creates_copies_edits_and_deletes_a_clip",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIR_VAR, dir.path())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(e) => panic!("the child process could not be started: {e}"),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "the child was not supposed to fail; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("CHILD_DONE"),
        "the child did not reach its sentinel; it may have panicked before finishing.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(
        !stdout.contains(DISTINCTIVE_VALUE),
        "the clip value reached stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains(DISTINCTIVE_VALUE),
        "the clip value reached stderr:\n{stderr}"
    );

    let log_dir = dir.path().join(".fast-clip");
    let entries = match std::fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(e) => panic!(
            "the log directory {} could not be read: {e}",
            log_dir.display()
        ),
    };

    let mut log_files_found = 0;
    let mut combined_log_content = String::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => panic!("a directory entry could not be read: {e}"),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("fast-clip") {
            continue;
        }
        log_files_found += 1;
        match std::fs::read_to_string(entry.path()) {
            Ok(content) => combined_log_content.push_str(&content),
            Err(e) => panic!("{} could not be read: {e}", entry.path().display()),
        }
    }

    assert!(
        log_files_found > 0,
        "no log file was found under {}; the sink was not live",
        log_dir.display()
    );
    // A positive control: the pre-WP-10 colour remedy is the one line ADR-0012
    // names as reachable only through this sink. Its presence is what proves
    // the file actually received real application log lines rather than being
    // empty and vacuously clean.
    assert!(
        combined_log_content.contains("delete the whole"),
        "the log file exists but does not contain the expected remedy line, \
         so this test cannot tell a live sink from an empty one:\n{combined_log_content}"
    );
    assert!(
        !combined_log_content.contains(DISTINCTIVE_VALUE),
        "the clip value reached the log file:\n{combined_log_content}"
    );
}

// ---- WP-07: the PIN, the DEK, the salt and the wrapped blob ----

/// Set by the parent for the encrypted-session child.
const CRYPTO_DIR_VAR: &str = "FASTCLIP_CRYPTO_LOG_TEST_DIR";

/// Fixed rather than generated, so the parent can grep for the exact bytes the
/// child used. A real DEK is random; nothing about the code paths under test
/// depends on which 32 bytes these are.
const FIXED_DEK: [u8; 32] = [0x5c; 32];
/// Likewise fixed. Six ASCII digits, and distinctive enough that a match in a
/// log is not a timestamp.
const FIXED_PIN: &str = "428913";
const FIXED_SALT: [u8; 16] = [0x7e; 16];
/// A wrong PIN, as a user mistyping would send it. Distinctive, because a
/// rejected PIN is one keystroke from the real one and is still a secret.
const WRONG_PIN: &str = "570241";
/// The PIN a `change_pin` would move to.
const NEW_PIN: &str = "836492";
/// A clip value stored in the *encrypted* database, so criterion 6 is checked
/// on the encrypted path as well as the plaintext one.
const ENCRYPTED_CLIP_VALUE: &str = "xyzzy-wp07-3ab91f7c-encrypted-value-do-not-log";

/// **The four secrets `storage.md` names, none of which may reach a log.**
///
/// The DEK is the one that matters most: `PRAGMA key = "x'…'"` embeds it in SQL
/// text, and `rusqlite::Error::SqlInputError`'s `Display` renders the SQL that
/// produced it — so `log::error!("…: {error}")`, the idiom used correctly
/// everywhere else in this crate, would write the whole store's key into the log
/// file the application creates beside the database it decrypts.
///
/// **Nothing about the calling code looks wrong.** The statement is correct, the
/// error handling is idiomatic, and the disclosure comes from a `Display` in a
/// dependency. That is why this test exists rather than a review comment
/// (`storage.md` § A keying statement's error is never formatted).
///
/// This has been shown to fail: temporarily logging the formatted error from
/// `connection::apply_key` makes the DEK assertion below catch it. The first
/// attempt at that control did **not** trip it, which is how the hazard's exact
/// shape was established — `rusqlite` prepares statements one at a time, so the
/// leak needs the key-carrying statement *itself* to fail rather than a later
/// one in the same batch.
#[test]
fn no_pin_dek_salt_or_wrapped_blob_reaches_the_log_file_or_stdout() {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => panic!("could not find the test binary: {e}"),
    };

    let output = Command::new(exe)
        .args([
            "the_child_runs_an_encrypted_session",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CRYPTO_DIR_VAR, dir.path())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(e) => panic!("the child process could not be started: {e}"),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child was not supposed to fail; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("CHILD_DONE"),
        "the child did not reach its sentinel.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let store_dir = dir.path().join("store");
    let mut logs = String::new();
    let entries = match std::fs::read_dir(&store_dir) {
        Ok(entries) => entries,
        Err(e) => panic!(
            "the log directory {} could not be read: {e}",
            store_dir.display()
        ),
    };
    let mut log_files_found = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => panic!("a directory entry could not be read: {e}"),
        };
        if !entry.file_name().to_string_lossy().starts_with("fast-clip") {
            continue;
        }
        log_files_found += 1;
        match std::fs::read_to_string(entry.path()) {
            Ok(content) => logs.push_str(&content),
            Err(e) => panic!("{} could not be read: {e}", entry.path().display()),
        }
    }
    assert!(
        log_files_found > 0,
        "no log file was found under {}; the sink was not live",
        store_dir.display()
    );

    // A positive control. The child drives several failure paths that log by
    // design; if none of their lines is here, this test is grepping an empty
    // file and would pass over any leak at all.
    assert!(
        logs.contains("DPAPI") || logs.contains("key material"),
        "the log holds none of the child's expected diagnostics, so this test \
         cannot tell a live sink from an empty one:\n{logs}"
    );

    // The wrapped blob and the salt are read back from the file the child wrote,
    // because a fresh nonce makes the ciphertext unpredictable. This process is
    // the same Windows user, so DPAPI unprotects it.
    let paths = fast_clip_lib::storage::StorePaths::at(&store_dir);
    let material = match fast_clip_lib::crypto::keyfile::read(&paths) {
        Ok(material) => material,
        Err(e) => panic!("the parent should read the key material it owns: {e}"),
    };
    let protected_blob = match std::fs::read(paths.keyfile()) {
        Ok(bytes) => bytes,
        Err(e) => panic!("the keyfile should be readable: {e}"),
    };

    let mut secrets: Vec<(&str, String)> = vec![
        ("the PIN", FIXED_PIN.to_string()),
        ("a rejected PIN", WRONG_PIN.to_string()),
        ("a replacement PIN", NEW_PIN.to_string()),
        ("the DEK in hex", hex(&FIXED_DEK)),
        ("the DEK in upper hex", hex(&FIXED_DEK).to_uppercase()),
        ("the salt in hex", hex(&material.kdf.salt)),
        (
            "the wrapped DEK in hex",
            hex(&material.wrapped_dek.ciphertext),
        ),
        ("the wrap nonce in hex", hex(&material.wrapped_dek.nonce)),
        ("the DPAPI blob in hex", hex(&protected_blob)),
        ("the encrypted clip value", ENCRYPTED_CLIP_VALUE.to_string()),
    ];
    // The byte-array rendering a `{:?}` on a slice produces, which is how a
    // derived `Debug` would leak these rather than as hex.
    secrets.push(("the DEK as a debug byte array", format!("{FIXED_DEK:?}")));
    secrets.push((
        "the salt as a debug byte array",
        format!("{:?}", material.kdf.salt),
    ));

    for (name, secret) in &secrets {
        assert!(
            !secret.is_empty(),
            "{name} is empty, so its assertion proves nothing"
        );
        assert!(
            !logs.contains(secret.as_str()),
            "{name} reached the log file"
        );
        assert!(!stdout.contains(secret.as_str()), "{name} reached stdout");
        assert!(!stderr.contains(secret.as_str()), "{name} reached stderr");
    }
}

/// The encrypted-session child. A no-op unless the parent asked for it by name.
///
/// It walks the paths that would leak if the rules were not followed: a keyed
/// open, a keyed open with the **wrong** key, an unkeyed open of an encrypted
/// store, a DPAPI failure, a parameter-floor rejection, a rejected PIN, and a
/// wrong-PIN unwrap. Each one either logs a fixed string or logs an error that
/// has been checked not to carry key material.
#[test]
fn the_child_runs_an_encrypted_session() {
    use fast_clip_lib::crypto::kdf::{self, KdfParams, MIN_M_COST_KIB, MIN_T_COST, P_COST};
    use fast_clip_lib::crypto::{keyfile, wrap, Dek, KeyMaterial, Pin};
    use fast_clip_lib::storage::connection;

    let dir = match std::env::var(CRYPTO_DIR_VAR) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let root = std::path::Path::new(&dir);
    let store_dir = root.join("store");
    if let Err(e) = std::fs::create_dir_all(&store_dir) {
        panic!("the child could not create the store directory: {e}");
    }

    let plugin = fast_clip_lib::log_plugin::<tauri::test::MockRuntime>(Some(store_dir.clone()));
    let built = tauri::test::mock_builder()
        .plugin(plugin)
        .build(tauri::test::mock_context(tauri::test::noop_assets()));
    let _app = match built {
        Ok(app) => app,
        Err(e) => panic!("the mock app with the log plugin could not be built: {e}"),
    };

    let paths = StorePaths::at(&store_dir);
    let dek = Dek::from_bytes(FIXED_DEK);
    let pin = match Pin::parse("pin", Some(FIXED_PIN.to_owned())) {
        Ok(pin) => pin,
        Err(e) => panic!("the fixture PIN should parse: {e}"),
    };

    // The floor rather than the shipped tuple: this child runs inside a debug
    // test binary and the cost is not what is under test.
    let params = KdfParams {
        m_cost_kib: MIN_M_COST_KIB,
        t_cost: MIN_T_COST,
        p_cost: P_COST,
        salt: FIXED_SALT,
    };
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

    // Write, read back, and record a failed attempt — the four `keyfile`
    // writers, in miniature.
    if let Err(e) = keyfile::write(&paths, &material) {
        panic!("the key material should write: {e}");
    }
    let mut reread = match keyfile::read(&paths) {
        Ok(material) => material,
        Err(e) => panic!("the key material should read: {e}"),
    };
    reread.record_failure(keyfile::now_unix_ms());
    if let Err(e) = keyfile::write(&paths, &reread) {
        panic!("the counter should persist: {e}");
    }
    reread.record_success();
    if let Err(e) = keyfile::write(&paths, &reread) {
        panic!("the reset should persist: {e}");
    }

    // A keyed session over a real encrypted store.
    let opened = match connection::open_encrypted(&paths.db(), &dek) {
        Ok(connection) => connection,
        Err(e) => panic!("the encrypted store should open: {e}"),
    };
    if let Err(e) = opened.execute_batch("CREATE TABLE clips (id INTEGER PRIMARY KEY, v TEXT);") {
        panic!("the fixture schema should create: {e}");
    }
    if let Err(e) = opened.execute("INSERT INTO clips (v) VALUES (?1)", [ENCRYPTED_CLIP_VALUE]) {
        panic!("the fixture clip should insert: {e}");
    }
    connection::checkpoint_and_close(opened);

    // **The failure paths.** Each of these logs, and none may carry a secret.
    let mut wrong_key = FIXED_DEK;
    wrong_key[0] ^= 0xff;
    let _ = connection::open_encrypted(&paths.db(), &Dek::from_bytes(wrong_key));
    let _ = connection::open(&paths.db());

    // A wrong PIN: the AEAD says no, and nothing is logged for it.
    if let Ok(other) = Pin::parse("pin", Some("000000".to_owned())) {
        if let Ok(key) = kdf::derive(&other, &material.kdf) {
            let _ = wrap::unwrap(&key, &material.wrapped_dek);
        }
    }

    // A rejected PIN argument, which must not echo the value.
    let _ = Pin::parse("pin", Some("42891".to_owned()));

    // **The PIN as a command actually receives it.** Everything above works on
    // an already-parsed `Pin`; these drive the whole surface that takes one from
    // the wire, including the failure paths, so the value travels the route a
    // user's PIN really takes. `enable_encryption` and the rest are plain
    // functions behind their `#[tauri::command]` attribute, but they need an
    // `AppHandle`, so the store-facing halves are exercised directly here and
    // `tests/ipc.rs` drives the commands themselves.
    let session = StorePaths::at(root.join("session"));
    if let Err(e) = std::fs::create_dir_all(session.dir()) {
        panic!("the child could not create the session directory: {e}");
    }
    let session_dek = match Dek::generate() {
        Ok(dek) => dek,
        Err(e) => panic!("a DEK should generate: {e}"),
    };
    let session_material = match KeyMaterial::create(&pin, &session_dek) {
        Ok(material) => material,
        Err(e) => panic!("the session key material should build: {e}"),
    };
    if let Err(e) = keyfile::write(&session, &session_material) {
        panic!("the session key material should write: {e}");
    }
    // A wrong PIN, recorded — the most-exercised failure path in the feature,
    // and the one that rewrites `keyfile` on every attempt.
    if let Ok(wrong) = Pin::parse("pin", Some(WRONG_PIN.to_owned())) {
        let mut recorded = session_material.clone();
        recorded.record_failure(keyfile::now_unix_ms());
        if let Err(e) = keyfile::write(&session, &recorded) {
            panic!("the attempt counter should persist: {e}");
        }
        if let Ok(key) = kdf::derive(&wrong, &session_material.kdf) {
            let _ = wrap::unwrap(&key, &session_material.wrapped_dek);
        }
    }
    // A re-wrap under a new PIN, as `change_pin` performs it.
    if let Ok(next) = Pin::parse("new_pin", Some(NEW_PIN.to_owned())) {
        if let Ok(rewrapped) = KeyMaterial::create(&next, &session_dek) {
            if let Err(e) = keyfile::write(&session, &rewrapped) {
                panic!("the re-wrapped key material should write: {e}");
            }
        }
    }

    // Parameters below the floor: the costs are logged, the salt is not.
    let below_floor = KdfParams {
        m_cost_kib: 8,
        t_cost: 1,
        p_cost: P_COST,
        salt: FIXED_SALT,
    };
    let _ = kdf::derive(&pin, &below_floor);

    // DPAPI refusing, in a scratch directory so the store's own keyfile is left
    // intact for the parent to read.
    let scratch = StorePaths::at(root.join("scratch"));
    if let Err(e) = std::fs::create_dir_all(scratch.dir()) {
        panic!("the child could not create the scratch directory: {e}");
    }
    let mut damaged = vec![1u8];
    damaged.extend_from_slice(b"not a DPAPI blob");
    if let Err(e) = std::fs::write(scratch.keyfile(), &damaged) {
        panic!("the scratch fixture should write: {e}");
    }
    let _ = keyfile::read(&scratch);
    let _ = keyfile::read(&StorePaths::at(root.join("nowhere")));

    log::logger().flush();
    println!("CHILD_DONE");
}

/// The child. A no-op unless the parent asked for it by name and told it where
/// to write, exactly as `kill_mid_write.rs`'s child is.
#[test]
fn the_child_creates_copies_edits_and_deletes_a_clip() {
    let dir = match std::env::var(DIR_VAR) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let root = std::path::Path::new(&dir);
    let log_dir = root.join(".fast-clip");

    // The exact function `lib.rs::run` registers, against a temporary
    // directory standing in for `~/.fast-clip`. Building the mock app runs
    // plugin initialisation the same way `Builder::build` does for the real
    // one (`app.rs`'s `initialize_plugins` runs inside `build`, not `run`),
    // which is what installs the global logger before anything below logs a
    // line.
    let plugin = fast_clip_lib::log_plugin::<tauri::test::MockRuntime>(Some(log_dir.clone()));
    let built = tauri::test::mock_builder()
        .plugin(plugin)
        .build(tauri::test::mock_context(tauri::test::noop_assets()));
    let _app = match built {
        Ok(app) => app,
        Err(e) => panic!("the mock app with the log plugin could not be built: {e}"),
    };

    let store = Store::open(StorePaths::at(log_dir.clone()));

    // Triggers the one line ADR-0012 calls the only statement of the pre-WP-10
    // remedy anywhere in the codebase (`colour.rs::from_stored`). This is the
    // positive control the parent checks for, proving the sink is live rather
    // than merely present.
    let _ = Colour::from_stored("unset");

    let id = match store.with_unlocked_store(|open| {
        clips::insert(
            open,
            "a distinctive label",
            DISTINCTIVE_VALUE,
            Colour::DEFAULT,
        )
    }) {
        Ok(id) => id,
        Err(e) => panic!("the child could not create the clip: {e}"),
    };

    if let Err(e) = fast_clip_lib::commands::copy(&store, &DiscardingClipboard, id) {
        panic!("the child could not copy the clip: {e}");
    }

    let edited_value = format!("{DISTINCTIVE_VALUE}-edited");
    if let Err(e) = store.with_unlocked_store(|open| {
        clips::update(
            open,
            id,
            "a distinctive label, edited",
            &edited_value,
            Colour::Red,
        )
    }) {
        panic!("the child could not edit the clip: {e}");
    }

    if let Err(e) = store.with_unlocked_store(|open| clips::delete(open, id)) {
        panic!("the child could not delete the clip: {e}");
    }

    store.shutdown();

    // Flush whatever the logger has buffered, so the parent reads a complete
    // file rather than racing a rotator's internal buffer.
    log::logger().flush();

    println!("CHILD_DONE");
}
