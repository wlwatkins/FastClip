//! Where the store lives: `~/.fast-clip/`.
//!
//! `dirs::home_dir()` joined with `.fast-clip`, per
//! `docs/src/architecture/storage.md` § Location and ADR-0003. The pre-refactor
//! `%LOCALAPPDATA%\FastClip\db` is neither read nor written by any path in this
//! crate — it is a different directory, not a different filename, which is what
//! makes the non-collision structural.

use std::path::{Path, PathBuf};

use crate::error::ClipError;

/// The directory name under the user's home directory.
pub const DIR_NAME: &str = ".fast-clip";

/// The clip database. Its WAL and shared-memory sidecars are derived from it.
pub const DB_FILE: &str = "clips.db";
/// The intermediate a conversion builds beside the live store.
pub const DB_NEW_FILE: &str = "clips.db.new";
/// The DPAPI-protected wrapped DEK. Present only when encryption is on.
pub const KEYFILE: &str = "keyfile";
/// The intermediate `change_pin` and `enable_encryption` write before renaming.
pub const KEYFILE_NEW: &str = "keyfile.new";
/// Plaintext, readable while locked, holds no clip data.
pub const SETTINGS_FILE: &str = "settings.json";
/// The intermediate [`SETTINGS_FILE`] is written to before being renamed.
///
/// **Not** in [`StorePaths::intermediates`], and that is not an oversight: the
/// sweep list is fixed by `storage.md` § Startup recovery at four files, and this
/// one needs none of the protection that list exists to give. It holds a version
/// number and a boolean — no clip data, in plaintext or otherwise — so a copy
/// left by a killed process discloses nothing, and the next
/// `set_always_on_top` truncates it.
pub const SETTINGS_NEW_FILE: &str = "settings.json.new";

/// Every path the store uses, derived from one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePaths {
    dir: PathBuf,
}

impl StorePaths {
    /// `~/.fast-clip/`.
    ///
    /// A machine with no resolvable home directory is a typed `storage` error,
    /// not a panic. `DataBase::new()` used to `expect()` here.
    pub fn from_home() -> Result<Self, ClipError> {
        match dirs::home_dir() {
            Some(home) => Ok(Self::at(home.join(DIR_NAME))),
            None => {
                log::error!("no home directory could be resolved; the store has nowhere to live");
                Err(ClipError::Storage)
            }
        }
    }

    /// An arbitrary directory. Tests use this so they never touch the real store.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn db(&self) -> PathBuf {
        self.dir.join(DB_FILE)
    }

    /// `clips.db-wal`. Never swept: a WAL beside the database is committed data
    /// (storage.md § When a delete fails).
    pub fn db_wal(&self) -> PathBuf {
        self.dir.join(format!("{DB_FILE}-wal"))
    }

    /// `clips.db-shm`.
    pub fn db_shm(&self) -> PathBuf {
        self.dir.join(format!("{DB_FILE}-shm"))
    }

    pub fn db_new(&self) -> PathBuf {
        self.dir.join(DB_NEW_FILE)
    }

    pub fn keyfile(&self) -> PathBuf {
        self.dir.join(KEYFILE)
    }

    /// The temporary file **every** write to `keyfile` goes through
    /// (`storage.md` § Every write to `keyfile` is atomic. Every one.).
    ///
    /// A sibling of its target, because a rename is atomic only within one
    /// volume. It is in [`Self::intermediates`], so a crash between the write
    /// and the rename leaves a file the next launch sweeps.
    pub fn keyfile_new(&self) -> PathBuf {
        self.dir.join(KEYFILE_NEW)
    }

    pub fn settings(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }

    /// The temporary file a settings write is renamed from. In the same
    /// directory as its target, because a rename across volumes is a copy and a
    /// copy is not atomic.
    pub fn settings_new(&self) -> PathBuf {
        self.dir.join(SETTINGS_NEW_FILE)
    }

    /// The files that exist only inside a conversion. Any that survives a
    /// restart is debris from an interrupted one, and startup recovery step 2
    /// deletes all of them before it classifies anything.
    ///
    /// `clips.db-wal` and `clips.db-shm` are deliberately absent from this list.
    pub fn intermediates(&self) -> [PathBuf; 4] {
        [
            self.dir.join(DB_NEW_FILE),
            self.dir.join(format!("{DB_NEW_FILE}-wal")),
            self.dir.join(format!("{DB_NEW_FILE}-shm")),
            self.dir.join(KEYFILE_NEW),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_the_ones_the_design_names() {
        let paths = StorePaths::at("root");
        assert_eq!(paths.db(), Path::new("root").join("clips.db"));
        assert_eq!(paths.db_wal(), Path::new("root").join("clips.db-wal"));
        assert_eq!(paths.db_shm(), Path::new("root").join("clips.db-shm"));
        assert_eq!(paths.db_new(), Path::new("root").join("clips.db.new"));
        assert_eq!(paths.keyfile(), Path::new("root").join("keyfile"));
        assert_eq!(paths.settings(), Path::new("root").join("settings.json"));
        assert_eq!(
            paths.settings_new(),
            Path::new("root").join("settings.json.new")
        );
    }

    /// A rename is atomic only within one volume, so the temporary file has to
    /// be a sibling of its target rather than in the system temporary directory.
    #[test]
    fn the_settings_temporary_file_is_a_sibling_of_the_settings_file() {
        let paths = StorePaths::at("root");
        assert_eq!(paths.settings_new().parent(), paths.settings().parent());
    }

    #[test]
    fn the_sweep_list_holds_four_intermediates_and_no_live_sidecar() {
        let paths = StorePaths::at("root");
        let names: Vec<String> = paths
            .intermediates()
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            names,
            vec![
                "clips.db.new",
                "clips.db.new-wal",
                "clips.db.new-shm",
                "keyfile.new"
            ]
        );
        // Deleting a live sidecar discards committed writes (ADR-0009).
        assert!(!names.iter().any(|n| n == "clips.db-wal"));
        assert!(!names.iter().any(|n| n == "clips.db-shm"));
    }

    #[test]
    fn the_home_directory_is_joined_with_a_dot_fast_clip_directory() {
        // Not asserted against the real home directory, because the test must
        // not depend on the machine it runs on.
        let paths = StorePaths::at(Path::new("C:\\Users\\someone").join(DIR_NAME));
        assert!(paths.dir().ends_with(".fast-clip"));
    }
}
