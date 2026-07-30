//! A process killed mid-write leaves a readable database.
//!
//! WP-03's definition of done, and the durability ADR-0009 claims. SQLite gives
//! this for free, which is worth confirming rather than assuming — so this test
//! really does terminate a process, rather than reasoning about what one would
//! leave behind.
//!
//! The child opens a store in a temporary directory, commits some clips, commits
//! a reorder, opens a transaction it never commits, and then aborts.
//! `std::process::abort` on Windows is `__fastfail`: no unwinding, no
//! destructors, no clean close and no opportunity for SQLite to tidy up.
//!
//! **The abandoned transaction is a half-written reorder**, not only a run of
//! inserts (WP-06: "a reorder interrupted mid-write leaves a valid store"). It
//! holds the offset step of the offset-then-write pair and some — not all — of
//! the final positions, which is the exact state a kill between the two writes
//! leaves in the journal. The parent asserts the store reads back as the
//! *committed* order, dense, with no row left holding an offset position.

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
    // The child inserted `clip 0`..`clip 49` and then committed a full
    // reversal. The committed reorder is what survives; the abandoned one left
    // nothing, not even the offset positions it had already written.
    let expected: Vec<String> = (0..COMMITTED).rev().map(|n| format!("clip {n}")).collect();
    assert_eq!(
        labels, expected,
        "the committed order survives the kill unchanged"
    );
    assert!(
        !labels.iter().any(|label| label == "in flight"),
        "no uncommitted row may survive"
    );

    // A `use_count` written through before the kill is still there. It was
    // incremented on `clip 0`, which the reversal moved to the end.
    match rows.iter().find(|row| row.label == "clip 0") {
        Some(row) => assert_eq!(row.use_count, 1),
        None => panic!("clip 0 should have survived"),
    }
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

    // A committed reorder: every one of the fifty rows moves. This is the order
    // the parent must find.
    let reordered = store.with_unlocked_store(|open| {
        let mut ids = clips::ids_in_order(open)?;
        ids.reverse();
        clips::reorder(open, &ids)
    });
    if let Err(e) = reordered {
        panic!("the child could not reorder: {e}");
    }

    // Work in flight when the process dies. It must leave no trace.
    let _ = store.with_unlocked_store(|open| -> Result<(), ClipError> {
        let mut ids = clips::ids_in_order(open)?;
        ids.rotate_left(7);

        if open.execute_batch("BEGIN IMMEDIATE").is_err() {
            return Err(ClipError::Internal);
        }

        // Step one of a renumber: every position offset clear of the occupied
        // range. On its own this is a store whose order is `50..99`.
        if open
            .execute(
                "UPDATE clips SET position = position + ?1",
                [COMMITTED as i64],
            )
            .is_err()
        {
            return Err(ClipError::Internal);
        }
        // Step two, stopped part way through. Ten rows hold their final
        // position and forty still hold an offset one — a half-applied order,
        // which is the state that must not be able to reach the disk.
        for (position, id) in ids.iter().enumerate().take(UNCOMMITTED) {
            let written = open.execute(
                "UPDATE clips SET position = ?2 WHERE id = ?1",
                (id.as_hyphenated().to_string(), position as i64),
            );
            if written.is_err() {
                return Err(ClipError::Internal);
            }
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
