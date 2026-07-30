//! The settings pair of WP-14: `get_settings` and `set_always_on_top`.
//!
//! `docs/src/architecture/contract.md` §2 is authoritative for both. Where this
//! file and that page disagree, this file is the defect.
//!
//! **Both work while the store is locked**, and neither touches the database.
//! They read and write `~/.fast-clip/settings.json`, which is plaintext and
//! outside the store precisely so that the window can be positioned before a PIN
//! has been entered — and while it never is. Neither command goes through
//! [`crate::Store::with_unlocked_store`]; they ask the store only where its
//! directory is.
//!
//! **Neither can fail because of the settings file's contents.** Missing,
//! unreadable, truncated, unparseable or carrying an unrecognised `version`, the
//! answer is the documented default. `storage` is reachable for exactly one
//! reason: `~/.fast-clip/` could not be created or reached at all.

use tauri::{AppHandle, Runtime, State};

use crate::commands::wire;
use crate::error::ClipError;
use crate::settings::{self, Settings};
use crate::storage::Store;
use crate::window;

/// The persisted settings, or the default when there are none.
///
/// Called at step 2 of the startup sequence. A `storage` rejection here is not
/// treated by the frontend: it continues to `get_lock_state`, which reports the
/// same fault through the failure path the sequence already defines. One failure
/// surface, not two.
#[tauri::command(rename_all = "snake_case")]
pub fn get_settings(store: State<'_, Store>) -> Result<Settings, ClipError> {
    let paths = store.directory()?;
    Ok(settings::read(paths))
}

/// Persist the always-on-top setting, and apply it.
///
/// **The order is persist, then apply**, and it is a decision rather than an
/// accident. The command's declared mutation is the settings file; it is written
/// atomically, so a failure there changes nothing and `storage` is the whole
/// truth. The window call is then attempted, and it is the step that cannot be
/// undone by returning an error — a window that has already been raised stays
/// raised.
///
/// A failed window call is `internal`
/// ([`window::apply_always_on_top`]). It is not `storage`: the disk is fine and
/// the setting is saved, and telling the user their store could not be written
/// would describe a fault that did not happen. The persisted value is applied
/// again at the next launch, so the two converge.
#[tauri::command(rename_all = "snake_case")]
pub fn set_always_on_top<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    enabled: Option<bool>,
) -> Result<(), ClipError> {
    // Contract §1: every argument is `Option<T>` and an absent key is
    // `required`. `invoke("set_always_on_top", {})` is the shape of any frontend
    // bug that lets a variable go undefined, and it must not fail inside Tauri
    // with a plain string.
    let enabled = wire::require("enabled", enabled)?;

    let paths = store.directory()?;
    settings::write(
        paths,
        Settings {
            always_on_top: enabled,
        },
    )?;

    window::apply_always_on_top(&app, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::InvalidReason;
    use crate::storage::StorePaths;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn store_in(parent: &tempfile::TempDir) -> Store {
        Store::open(StorePaths::at(parent.path().join(".fast-clip")))
    }

    /// The command's own validation, without a window. The application half is
    /// exercised through a real `invoke` in `tests/ipc.rs`.
    #[test]
    fn an_absent_enabled_argument_is_required_and_writes_nothing() {
        let parent = dir();
        let store = store_in(&parent);
        let paths = match store.directory() {
            Ok(paths) => paths.clone(),
            Err(e) => panic!("the directory should exist after recovery: {e}"),
        };

        assert_eq!(
            wire::require::<bool>("enabled", None),
            Err(ClipError::InvalidInput {
                field: "enabled".into(),
                reason: InvalidReason::Required,
            })
        );
        assert!(
            !paths.settings().exists(),
            "a rejected argument writes no file"
        );
    }

    /// `get_settings` reports the default for every content fault, and the store
    /// being unopenable is not its business either.
    #[test]
    fn the_settings_are_readable_while_the_database_is_not() {
        let parent = dir();
        let paths = StorePaths::at(parent.path().join(".fast-clip"));
        if let Err(e) = std::fs::create_dir_all(paths.dir()) {
            panic!("could not create the fixture directory: {e}");
        }
        // An encrypted store: recovery leaves it shut, and every clip command
        // returns `locked`.
        if let Err(e) = std::fs::write(paths.db(), [0x1f; 64]) {
            panic!("could not write the fixture: {e}");
        }
        if let Err(e) = settings::write(
            &paths,
            Settings {
                always_on_top: true,
            },
        ) {
            panic!("the settings write should succeed: {e}");
        }

        let store = Store::open(paths);
        assert!(store.is_locked(), "the fixture should be locked");
        let read = match store.directory() {
            Ok(paths) => settings::read(paths),
            Err(e) => panic!("the directory should be reachable: {e}"),
        };
        assert_eq!(
            read,
            Settings {
                always_on_top: true
            }
        );
    }
}
