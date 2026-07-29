//! A process killed mid-write leaves a readable database.
//!
//! WP-03's definition of done, and the durability ADR-0009 claims. SQLite gives
//! this for free, which is worth confirming rather than assuming — so this test
//! really does terminate a process, rather than reasoning about what one would
//! leave behind.
//!
//! The child opens a store in a temporary directory, commits some clips, opens a
//! transaction it never commits, and then aborts. `std::process::abort` on
//! Windows is `__fastfail`: no unwinding, no destructors, no clean close and no
//! opportunity for SQLite to tidy up.

use std::process::Command;

use fast_clip_lib::storage::{clips, StorePaths};
use fast_clip_lib::{ClipError, Colour, Store};

/// Set by the parent to tell the child which directory to write into. Its
/// absence is what makes the child test a no-op in an ordinary run.
const DIR_VAR: &str = "FASTCLIP_KILL_TEST_DIR";
const COMMITTED: usize = 50;
const UNCOMMITTED: usize = 10;

#[test]
fn a_killed_process_leaves_every_committed_clip_and_no_partial_one() {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => panic!("could not find the test binary: {e}"),
    };

    let status = Command::new(exe)
        .args([
            "the_child_commits_then_dies",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIR_VAR, dir.path())
        .status();
    let status = match status {
        Ok(status) => status,
        Err(e) => panic!("the child process could not be started: {e}"),
    };
    assert!(
        !status.success(),
        "the child was supposed to die, not to exit cleanly"
    );

    // The store is reopened exactly as a launch would: startup recovery,
    // classification, version check, WAL replay.
    let store = Store::open(StorePaths::at(dir.path().join(".fast-clip")));
    assert_eq!(
        store.startup_fault(),
        None,
        "a killed process must leave a readable database"
    );

    let rows = match store.with_unlocked_store(|open| clips::list(open)) {
        Ok(rows) => rows,
        Err(e) => panic!("the surviving store should list its clips: {e}"),
    };

    assert_eq!(
        rows.len(),
        COMMITTED,
        "every committed clip survives, and no uncommitted one does"
    );
    let positions: Vec<i64> = rows.iter().map(|row| row.position).collect();
    assert_eq!(
        positions,
        (0..COMMITTED as i64).collect::<Vec<i64>>(),
        "the order is dense and unbroken"
    );
    let labels: Vec<String> = rows.iter().map(|row| row.label.clone()).collect();
    let expected: Vec<String> = (0..COMMITTED).map(|n| format!("clip {n}")).collect();
    assert_eq!(labels, expected, "the order survives the kill unchanged");

    // A `use_count` written through before the kill is still there.
    assert_eq!(rows[0].use_count, 1);
}

/// The child. A no-op unless the parent asked for it by name and told it where
/// to write.
#[test]
fn the_child_commits_then_dies() {
    let dir = match std::env::var(DIR_VAR) {
        Ok(dir) => dir,
        Err(_) => return,
    };

    let store = Store::open(StorePaths::at(
        std::path::Path::new(&dir).join(".fast-clip"),
    ));
    let written = store.with_unlocked_store(|open| -> Result<(), ClipError> {
        for n in 0..COMMITTED {
            clips::insert(open, &format!("clip {n}"), "a value", Colour::DEFAULT)?;
        }
        match clips::ids_in_order(open)?.first() {
            Some(id) => clips::increment_use_count(open, *id),
            None => Err(ClipError::Internal),
        }
    });
    if let Err(e) = written {
        panic!("the child could not write: {e}");
    }

    // Work in flight when the process dies. It must leave no trace.
    let _ = store.with_unlocked_store(|open| -> Result<(), ClipError> {
        if open.execute_batch("BEGIN IMMEDIATE").is_err() {
            return Err(ClipError::Internal);
        }
        for _ in 0..UNCOMMITTED {
            let inserted = open.execute(
                "INSERT INTO clips (id, label, value, colour, use_count, position)
                 VALUES (?1, 'in flight', 'a value', 'slate', 0,
                         (SELECT COALESCE(MAX(position), -1) + 1 FROM clips))",
                [uuid::Uuid::new_v4().as_hyphenated().to_string()],
            );
            if inserted.is_err() {
                return Err(ClipError::Internal);
            }
        }
        // No commit, no rollback, no close: the process simply stops here.
        std::process::abort();
    });
}
