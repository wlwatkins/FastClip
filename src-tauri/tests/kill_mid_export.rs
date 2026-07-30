//! A process killed mid-export leaves a complete export file or the previous
//! one — never a truncated backup, and never a stray plaintext copy under the
//! name the user chose.
//!
//! Export is the only user-facing recovery path this design has
//! ([ADR-0005](../../docs/src/architecture/adr/0005-sqlite-store.md)) and
//! [criterion 7](../../docs/src/product/spec.md#8-acceptance-criteria) has the
//! user wipe their store between the export and the import. A half-written
//! export is therefore not a cosmetic fault: it is the file the user reaches for
//! after they have already lost everything else.
//!
//! The rename in `export()` is the commit point and `sync_all` is what makes it
//! one. That is reasoning, and this file exists because reasoning is not the
//! standard: a second process exports in a loop and is killed with
//! `TerminateProcess`, which runs no destructor, unwinds nothing and flushes
//! nothing. The parent then asserts what is on the disk.
//!
//! **The `<path>.part` residue is expected and is not a failure of this test.**
//! Contract §2 `export_clips` names it as the one thing the design cannot
//! prevent: it sits in a directory FastClip does not own and cannot sweep. What
//! is asserted is the *target* — the file the user chose and will open.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use fast_clip_lib::commands::export_import::export;
use fast_clip_lib::storage::{clips, StorePaths};
use fast_clip_lib::{Colour, Store};

/// Set by the parent. Its absence is what makes the child a no-op in an ordinary
/// run, so `cargo test` pays nothing for it.
const DIR_VAR: &str = "FASTCLIP_EXPORT_KILL_DIR";
/// Written by the child after its first completed export, so the parent can tell
/// "interrupted an exporter" from "spawned something that never exported".
const SENTINEL: &str = "the-child-exported";
/// Enough clips that one export is not instantaneous, so the kill lands inside
/// one rather than between two.
const CLIPS: usize = 400;

fn target_in(dir: &Path) -> PathBuf {
    dir.join("clips.json")
}

fn part_of(target: &Path) -> PathBuf {
    let mut part = target.as_os_str().to_owned();
    part.push(".part");
    PathBuf::from(part)
}

#[test]
fn a_process_killed_mid_export_never_leaves_a_truncated_export_file() {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => panic!("could not create a temporary directory: {e}"),
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => panic!("could not find the test binary: {e}"),
    };

    let spawned = Command::new(exe)
        .args([
            "the_child_exports_until_it_dies",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIR_VAR, dir.path())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => panic!("the exporting child could not be started: {e}"),
    };

    std::thread::sleep(Duration::from_millis(700));
    if let Err(e) = child.kill() {
        panic!("the child should be killable: {e}");
    }
    if let Err(e) = child.wait() {
        panic!("the child should be reapable: {e}");
    }

    let target = target_in(dir.path());

    // Without this the test could pass by the child never having run, which is
    // the way a kill test quietly stops testing anything.
    assert!(
        dir.path().join(SENTINEL).is_file(),
        "the child never completed an export, so nothing was interrupted"
    );

    let bytes = match std::fs::read(&target) {
        Ok(bytes) => bytes,
        Err(e) => panic!("a completed export should still be on disk: {e}"),
    };

    // Not "it parses" — it parses *and holds every clip*. A reader that accepted
    // a short file would pass a weaker assertion than the one that matters.
    match fast_clip_lib::export_file::parse(&bytes) {
        Ok(clips) => assert_eq!(
            clips.len(),
            CLIPS,
            "a killed exporter left a file holding {} of {CLIPS} clips",
            clips.len()
        ),
        Err(e) => panic!("a killed exporter left a file that does not parse: {e}"),
    }

    // The residue the contract accepts. Recorded rather than asserted either
    // way: its presence depends on where the kill landed, and both outcomes are
    // within the design.
    let part = part_of(&target);
    println!(
        "the temporary file {} the kill: {}",
        if part.exists() {
            "survived"
        } else {
            "did not survive"
        },
        part.display()
    );
}

/// The child. A no-op unless the parent asked for it by name and told it where
/// to write.
#[test]
fn the_child_exports_until_it_dies() {
    let dir = match std::env::var(DIR_VAR) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let dir = PathBuf::from(dir);

    let store = Store::open(StorePaths::at(dir.join(".fast-clip")));
    let seeded = store.with_unlocked_store(|open| {
        for n in 0..CLIPS {
            clips::insert(
                open,
                &format!("clip {n}"),
                &"a value that is long enough to make the file worth writing".repeat(4),
                Colour::DEFAULT,
            )?;
        }
        Ok(())
    });
    if let Err(e) = seeded {
        panic!("the child could not seed the store: {e}");
    }

    let target = target_in(&dir);
    let mut announced = false;
    loop {
        match export(&store, &target) {
            Ok(_) => {
                if !announced {
                    let _ = std::fs::write(dir.join(SENTINEL), b"exporting");
                    announced = true;
                }
            }
            Err(e) => panic!("the child's export should succeed: {e}"),
        }
    }
}
