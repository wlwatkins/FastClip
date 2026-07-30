//! `~/.fast-clip/settings.json` — the one setting FastClip persists.
//!
//! `docs/src/architecture/storage.md` § Files puts this file outside the
//! database and in plaintext, because
//! [spec §4.4](../../../docs/src/product/spec.md) requires the always-on-top
//! toggle to apply at launch, which is before any PIN has been entered. It holds
//! no clip data, so nothing here can disclose one.
//!
//! **This file is the one versioned artefact whose unrecognised version is not
//! an error** (contract § Versions on disk, closed question 19). Missing,
//! unreadable, truncated, unparseable, or carrying a `version` this build does
//! not know — all five produce the same result: `always_on_top: false`, the
//! application starts, and the next write replaces the file at version 1.
//! Refusing to start over one boolean would be a worse failure than losing it.
//!
//! That leniency is deliberate and local. The store's schema version and the key
//! material's version both fail loudly, because both guard data that cannot be
//! reconstructed. This one guards a window property that the user can set again
//! in one click.
//!
//! **The write is atomic**: a temporary file in the same directory, `sync_all`,
//! then a rename onto the target. A process killed at any instant leaves either
//! the old settings or the new ones, never a truncated file — asserted by
//! killing a real process in the tests below rather than by reasoning about it.

use std::fs::{self, File};
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::error::ClipError;
use crate::storage::StorePaths;

/// The version this build writes and recognises.
pub const SETTINGS_VERSION: i64 = 1;

/// The settings as the frontend sees them (contract §1, `Settings`).
///
/// The `version` field is not part of this shape: it describes the file, not the
/// setting, and the contract's `Settings` is exactly one boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Settings {
    pub always_on_top: bool,
}

/// The shape written to disk. Field order is the order the design documents it
/// in: `{ "version": 1, "always_on_top": false }`.
#[derive(Debug, Serialize)]
struct OnDisk {
    version: i64,
    always_on_top: bool,
}

/// The shape read from disk.
///
/// **Lenient by construction, and `deny_unknown_fields` must never appear here.**
/// A field written by a later build is ignored rather than fatal, and a field
/// this build expects and does not find falls back to its default. Both rules
/// are the general one for anything that reads a file an older or newer build
/// wrote.
#[derive(Debug, Default, Deserialize)]
struct FromDisk {
    version: Option<i64>,
    always_on_top: Option<bool>,
}

/// Read the settings, falling back to the default rather than failing.
///
/// This function returns no error, and that is the contract's requirement rather
/// than a convenience: [`crate::commands::settings::get_settings`] can fail for
/// exactly one reason, and it is not anything about this file.
pub fn read(paths: &StorePaths) -> Settings {
    let path = paths.settings();

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A first run, and the commonest case by far. Not worth a warning.
            return Settings::default();
        }
        Err(error) => {
            log::warn!("settings.json could not be read; using the defaults: {error}");
            return Settings::default();
        }
    };

    let parsed: FromDisk = match serde_json::from_str(&text) {
        Ok(parsed) => parsed,
        Err(error) => {
            // The error's `Display` would quote the offending token. Nothing in
            // this file is a secret, but the habit of never formatting file
            // content into a log line is the one that keeps a clip out of one.
            log::warn!(
                "settings.json is not readable as settings ({:?} at line {}); using the defaults",
                error.classify(),
                error.line()
            );
            return Settings::default();
        }
    };

    match parsed.version {
        Some(SETTINGS_VERSION) => Settings {
            always_on_top: parsed.always_on_top.unwrap_or_default(),
        },
        found => {
            log::warn!(
                "settings.json carries version {found:?}, which this build does not know; \
                 using the defaults. The next write replaces it at version {SETTINGS_VERSION}"
            );
            Settings::default()
        }
    }
}

/// Write the settings, atomically.
///
/// Temporary file, `sync_all`, rename. The temporary file is removed on every
/// failure path, so a refused rename does not leave debris that the next read
/// has to reason about.
pub fn write(paths: &StorePaths, settings: Settings) -> Result<(), ClipError> {
    let temporary = paths.settings_new();

    let body = match serde_json::to_string(&OnDisk {
        version: SETTINGS_VERSION,
        always_on_top: settings.always_on_top,
    }) {
        Ok(body) => body,
        Err(error) => {
            // A struct of one integer and one boolean. Unreachable, and reported
            // rather than unwrapped: this crate does not decide to crash.
            log::error!("the settings could not be serialised: {error}");
            return Err(ClipError::Storage);
        }
    };

    if let Err(error) = write_and_sync(&temporary, body.as_bytes()) {
        log::error!("the settings could not be written: {error}");
        discard(&temporary);
        return Err(ClipError::Storage);
    }

    // The commit point. On Windows this replaces the target in one operation, so
    // a reader sees the old file or the new one and never a partial write.
    if let Err(error) = fs::rename(&temporary, paths.settings()) {
        log::error!("the settings could not be renamed into place: {error}");
        discard(&temporary);
        return Err(ClipError::Storage);
    }

    Ok(())
}

/// Write the whole body and flush it to the device before returning.
///
/// `sync_all` is what makes the rename a commit point rather than a promise: a
/// renamed file whose contents are still in the operating system's cache is a
/// file that a power cut can truncate after the rename has already happened.
///
/// **This must stay a separate function.** The handle is closed by `file` being
/// dropped at the end of *this* scope, which is what lets the caller rename over
/// the target and discard the temporary file — Windows refuses both while a
/// handle is open. Inlining it holds the handle until the end of the caller and
/// the rename fails. The same shape is load-bearing in
/// [`crate::crypto::keyfile`], where the renamed file is the store's only key.
fn write_and_sync(path: &std::path::Path, body: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(body)?;
    file.sync_all()
}

/// Remove the temporary file, absorbing a failure. It is overwritten by the next
/// write in any case.
fn discard(path: &std::path::Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!("a settings temporary file could not be removed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Set by the parent of the kill test, and by nothing else.
    const WRITE_LOOP_DIR: &str = "FASTCLIP_TEST_SETTINGS_WRITE_LOOP_DIR";
    /// Written by the child after its first completed write, so the parent can
    /// tell "interrupted a writer" from "spawned something that never wrote".
    const WRITE_LOOP_SENTINEL: &str = "the-child-wrote";

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    /// A store directory that exists, so that a write has somewhere to go.
    fn paths_in(parent: &tempfile::TempDir) -> StorePaths {
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        paths
    }

    fn put(paths: &StorePaths, contents: &str) {
        if let Err(e) = fs::write(paths.settings(), contents) {
            panic!("could not write the fixture: {e}");
        }
    }

    fn contents(paths: &StorePaths) -> String {
        match fs::read_to_string(paths.settings()) {
            Ok(text) => text,
            Err(e) => panic!("the settings file should be readable: {e}"),
        }
    }

    #[test]
    fn a_written_setting_reads_back_in_both_directions() {
        let parent = dir();
        let paths = paths_in(&parent);

        assert_eq!(
            write(
                &paths,
                Settings {
                    always_on_top: true
                }
            ),
            Ok(())
        );
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: true
            }
        );

        assert_eq!(
            write(
                &paths,
                Settings {
                    always_on_top: false
                }
            ),
            Ok(())
        );
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: false
            }
        );
    }

    /// The file's shape is documented in `storage.md` § Files, and a build that
    /// wrote a different one would be unreadable by the build that fixed it.
    #[test]
    fn the_file_on_disk_is_exactly_the_documented_shape() {
        let parent = dir();
        let paths = paths_in(&parent);

        if let Err(e) = write(
            &paths,
            Settings {
                always_on_top: false,
            },
        ) {
            panic!("the write should succeed: {e}");
        }
        assert_eq!(contents(&paths), r#"{"version":1,"always_on_top":false}"#);
    }

    /// Contract §1: `Settings` is one boolean. The file's `version` describes the
    /// file and must not cross the seam.
    #[test]
    fn the_wire_shape_is_one_boolean_and_carries_no_version() {
        let json = match serde_json::to_string(&Settings {
            always_on_top: true,
        }) {
            Ok(json) => json,
            Err(e) => panic!("the settings should serialise: {e}"),
        };
        assert_eq!(json, r#"{"always_on_top":true}"#);
    }

    /// The first of the three failure paths WP-14 names. A first run has no file
    /// and must start at the default rather than be blocked by its absence.
    #[test]
    fn a_missing_file_reads_as_the_default() {
        let parent = dir();
        let paths = paths_in(&parent);
        assert!(!paths.settings().exists());
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: false
            }
        );
    }

    /// A directory that does not exist at all is the same answer. `get_settings`
    /// reports that condition, and it reports it from the directory rather than
    /// from this file (contract, `get_settings`).
    #[test]
    fn a_missing_directory_reads_as_the_default_rather_than_panicking() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join("nowhere"));
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: false
            }
        );
    }

    /// The second. Every prefix of a valid file is a fixture here, so the test
    /// does not depend on guessing where a real truncation would land.
    #[test]
    fn every_truncation_of_a_valid_file_reads_as_the_default() {
        let parent = dir();
        let paths = paths_in(&parent);
        let whole = r#"{"version":1,"always_on_top":true}"#;

        for cut in 1..whole.len() {
            put(&paths, &whole[..cut]);
            assert_eq!(
                read(&paths),
                Settings {
                    always_on_top: false
                },
                "a file truncated to {cut} bytes must not block the launch"
            );
        }

        // The whole file is still the whole file.
        put(&paths, whole);
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: true
            }
        );
    }

    /// The third, and the one that is an exception to how every other on-disk
    /// version behaves (closed question 19).
    #[test]
    fn an_unrecognised_version_reads_as_the_default_and_is_never_an_error() {
        let parent = dir();
        let paths = paths_in(&parent);

        for version in ["2", "99", "0", "-1", "null"] {
            put(
                &paths,
                &format!(r#"{{"version":{version},"always_on_top":true}}"#),
            );
            assert_eq!(
                read(&paths),
                Settings {
                    always_on_top: false
                },
                "version {version} must fall back rather than fail"
            );
        }

        // Absent entirely, which is what a build older than the version field
        // would have written.
        put(&paths, r#"{"always_on_top":true}"#);
        assert_eq!(read(&paths), Settings::default());
    }

    /// And the next write repairs it, at version 1.
    #[test]
    fn the_next_write_replaces_an_unrecognised_version_with_version_one() {
        let parent = dir();
        let paths = paths_in(&parent);
        put(
            &paths,
            r#"{"version":99,"always_on_top":true,"theme":"dark"}"#,
        );

        if let Err(e) = write(
            &paths,
            Settings {
                always_on_top: true,
            },
        ) {
            panic!("the write should succeed: {e}");
        }
        assert_eq!(contents(&paths), r#"{"version":1,"always_on_top":true}"#);
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: true
            }
        );
    }

    /// Forgiving on input. A field a later build added is ignored, not fatal —
    /// the opposite of the rule for a command argument, and the difference is
    /// that this file was written by FastClip rather than sent by a caller.
    #[test]
    fn an_unknown_field_is_ignored_rather_than_rejected() {
        let parent = dir();
        let paths = paths_in(&parent);
        put(
            &paths,
            r#"{"version":1,"always_on_top":true,"theme":"dark","window":{"x":1}}"#,
        );
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: true
            }
        );
    }

    /// An unmappable value falls back to the default rather than failing the
    /// whole read.
    #[test]
    fn a_wrongly_typed_or_absent_setting_falls_back_to_the_default() {
        let parent = dir();
        let paths = paths_in(&parent);

        for body in [
            json!({ "version": 1, "always_on_top": "yes" }),
            json!({ "version": 1, "always_on_top": 1 }),
            json!({ "version": 1, "always_on_top": null }),
            json!({ "version": 1 }),
            json!([]),
            json!("nonsense"),
        ] {
            put(&paths, &body.to_string());
            assert_eq!(
                read(&paths),
                Settings {
                    always_on_top: false
                },
                "{body} must fall back"
            );
        }
    }

    #[test]
    fn an_unreadable_file_reads_as_the_default() {
        let parent = dir();
        let paths = paths_in(&parent);
        // A directory where the file should be: the read fails for a reason
        // other than absence, which is the case WP-14 calls "unreadable".
        if let Err(e) = fs::create_dir(paths.settings()) {
            panic!("could not create the fixture: {e}");
        }
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: false
            }
        );
    }

    /// A write into a directory that does not exist is `storage`, and it leaves
    /// nothing behind.
    #[test]
    fn a_write_that_cannot_reach_the_directory_is_storage_and_leaves_no_debris() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join("nowhere"));

        assert_eq!(
            write(
                &paths,
                Settings {
                    always_on_top: true
                }
            ),
            Err(ClipError::Storage)
        );
        assert!(!paths.settings_new().exists());
        assert!(!paths.settings().exists());
    }

    /// A failed write must not damage the settings that were already there.
    #[test]
    fn a_failed_write_leaves_the_previous_settings_intact() {
        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = write(
            &paths,
            Settings {
                always_on_top: true,
            },
        ) {
            panic!("the first write should succeed: {e}");
        }

        // A directory at the temporary path: `File::create` cannot replace it,
        // so the write fails before the rename.
        if let Err(e) = fs::create_dir(paths.settings_new()) {
            panic!("could not create the fixture: {e}");
        }
        assert_eq!(
            write(
                &paths,
                Settings {
                    always_on_top: false
                }
            ),
            Err(ClipError::Storage)
        );
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: true
            }
        );
        assert_eq!(contents(&paths), r#"{"version":1,"always_on_top":true}"#);
    }

    /// Debris from an interrupted write is not read and does not survive the
    /// next one.
    #[test]
    fn a_leftover_temporary_file_is_neither_read_nor_kept() {
        let parent = dir();
        let paths = paths_in(&parent);
        put(&paths, r#"{"version":1,"always_on_top":false}"#);
        if let Err(e) = fs::write(paths.settings_new(), r#"{"version":1,"always_"#) {
            panic!("could not write the fixture: {e}");
        }

        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: false
            }
        );
        if let Err(e) = write(
            &paths,
            Settings {
                always_on_top: true,
            },
        ) {
            panic!("the write should succeed: {e}");
        }
        assert_eq!(
            read(&paths),
            Settings {
                always_on_top: true
            }
        );
        assert!(
            !paths.settings_new().exists(),
            "the rename consumes the temporary file"
        );
    }

    /// **The interrupted-write criterion, proved by interrupting one.**
    ///
    /// A second process writes settings in a loop and is killed with
    /// `TerminateProcess`, which runs no destructor and flushes nothing. The file
    /// it leaves must be one of the two complete forms — byte for byte, not
    /// merely "parses" — because a reader that accepted a truncated file would
    /// pass a weaker assertion.
    #[test]
    fn a_process_killed_mid_write_leaves_either_the_old_settings_or_the_new() {
        use std::process::Command;
        use std::time::Duration;

        let parent = dir();
        let paths = paths_in(&parent);
        if let Err(e) = write(
            &paths,
            Settings {
                always_on_top: false,
            },
        ) {
            panic!("the seed write should succeed: {e}");
        }

        let executable = match std::env::current_exe() {
            Ok(executable) => executable,
            Err(e) => panic!("the test binary should know its own path: {e}"),
        };
        let spawned = Command::new(executable)
            .args([
                "--exact",
                "--nocapture",
                "settings::tests::write_loop_child",
            ])
            .env(WRITE_LOOP_DIR, paths.dir())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => panic!("the writing child should spawn: {e}"),
        };

        std::thread::sleep(Duration::from_millis(400));
        if let Err(e) = child.kill() {
            panic!("the child should be killable: {e}");
        }
        if let Err(e) = child.wait() {
            panic!("the child should be reapable: {e}");
        }

        // Without this the test could pass by the child never having run, which
        // is the way a kill test quietly stops testing anything.
        assert!(
            paths.dir().join(WRITE_LOOP_SENTINEL).is_file(),
            "the child never completed a write, so nothing was interrupted"
        );

        let after = contents(&paths);
        assert!(
            after == r#"{"version":1,"always_on_top":true}"#
                || after == r#"{"version":1,"always_on_top":false}"#,
            "a killed writer left {after:?}"
        );
        // And the value is usable, not merely present.
        let _: Settings = read(&paths);
    }

    /// The child half of the test above. It does nothing at all unless that test
    /// spawned it, so an ordinary run pays nothing for it.
    #[test]
    fn write_loop_child() {
        let directory = match std::env::var(WRITE_LOOP_DIR) {
            Ok(directory) => directory,
            Err(_) => return,
        };
        let paths = StorePaths::at(directory);
        let mut enabled = true;
        let mut announced = false;
        loop {
            let written = write(
                &paths,
                Settings {
                    always_on_top: enabled,
                },
            );
            if written.is_ok() && !announced {
                let _ = fs::write(paths.dir().join(WRITE_LOOP_SENTINEL), b"writing");
                announced = true;
            }
            enabled = !enabled;
        }
    }
}
