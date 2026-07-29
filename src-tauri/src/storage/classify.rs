//! The four-state classification of `clips.db`.
//!
//! `docs/src/architecture/storage.md` § Classifying clips.db. This is the most
//! dangerous read in the application: every deletion in startup recovery is
//! gated on a *positive* classification, and misreading an unreadable encrypted
//! store as an absent one destroys the only copy of the wrapped DEK.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// The first 16 bytes of a plaintext SQLite database, including the terminating
/// NUL. A SQLCipher database begins with a random salt, so it never matches.
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// What `clips.db` is. Established once, from the error kind of a single open,
/// and held in memory (storage.md § Who owns lock state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// The open failed with `io::ErrorKind::NotFound` **specifically**.
    Absent,
    /// 16 bytes read and they are the SQLite magic.
    Plaintext,
    /// 16 bytes read and they are anything else.
    Encrypted,
    /// The open or the read failed for any other reason. Deletes nothing,
    /// creates nothing, opens nothing.
    Unreadable,
}

impl Classification {
    /// Whether the store is encrypted. Never "not plaintext" — `Unreadable` is
    /// not an answer to this question, and treating it as one is what deletes a
    /// user's key material.
    pub fn is_encrypted(self) -> bool {
        matches!(self, Self::Encrypted)
    }

    /// Whether a stray `keyfile` beside this store may be deleted. Startup
    /// recovery step 5.
    pub fn permits_keyfile_delete(self) -> bool {
        matches!(self, Self::Absent | Self::Plaintext)
    }
}

/// Classify `clips.db` by reading its first 16 bytes.
///
/// `Path::exists()` and `fs::read(..).is_err()` are both wrong here and the
/// design names them: each reports a permission failure as absence, which sends
/// startup recovery down the delete-the-keyfile, create-a-fresh-database path
/// over an intact encrypted store.
pub fn classify(path: &Path) -> Classification {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            log::debug!("clips.db is absent; a first run creates one");
            return Classification::Absent;
        }
        Err(error) => {
            log::error!("clips.db could not be opened to classify it: {error}");
            return Classification::Unreadable;
        }
    };

    let mut header = [0u8; SQLITE_MAGIC.len()];
    if let Err(error) = file.read_exact(&mut header) {
        // A short read lands here too, and is `unreadable` by design: a file
        // too short to classify is not a file that has been shown to be absent.
        log::error!("the clips.db header could not be read: {error}");
        return Classification::Unreadable;
    }

    if &header == SQLITE_MAGIC {
        Classification::Plaintext
    } else {
        Classification::Encrypted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => panic!("could not create a temporary directory: {e}"),
        }
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Err(e) = fs::write(path, bytes) {
            panic!("could not write the fixture: {e}");
        }
    }

    #[test]
    fn a_missing_file_is_absent() {
        let dir = dir();
        assert_eq!(
            classify(&dir.path().join("clips.db")),
            Classification::Absent
        );
    }

    #[test]
    fn the_sqlite_magic_is_plaintext() {
        let dir = dir();
        let path = dir.path().join("clips.db");
        write(&path, b"SQLite format 3\0the rest of a database");
        assert_eq!(classify(&path), Classification::Plaintext);
    }

    #[test]
    fn anything_else_in_the_header_is_encrypted() {
        let dir = dir();
        let path = dir.path().join("clips.db");
        // A SQLCipher database opens with a 16-byte random salt.
        write(
            &path,
            &[
                0x9e, 0x2b, 0x00, 0xff, 0x41, 0x41, 0x41, 0x41, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
                0x06, 0x07, 0x08,
            ],
        );
        assert_eq!(classify(&path), Classification::Encrypted);
    }

    #[test]
    fn a_short_file_is_unreadable_and_never_absent() {
        let dir = dir();
        let path = dir.path().join("clips.db");
        write(&path, b"SQLite");
        assert_eq!(classify(&path), Classification::Unreadable);
    }

    #[test]
    fn an_unopenable_file_is_unreadable_and_never_absent() {
        // A directory standing where the database should be fails to open for a
        // reason that is not NotFound, which is the whole point of the
        // distinction: `Path::exists()` would say `true` and `fs::read().is_err()`
        // would say "not there", and both would be wrong.
        let dir = dir();
        let path = dir.path().join("clips.db");
        if let Err(e) = fs::create_dir(&path) {
            panic!("could not create the fixture directory: {e}");
        }
        assert_eq!(classify(&path), Classification::Unreadable);
    }

    #[test]
    fn only_a_positive_encrypted_classification_blocks_the_keyfile_delete() {
        assert!(Classification::Absent.permits_keyfile_delete());
        assert!(Classification::Plaintext.permits_keyfile_delete());
        assert!(!Classification::Encrypted.permits_keyfile_delete());
        assert!(!Classification::Unreadable.permits_keyfile_delete());

        assert!(!Classification::Unreadable.is_encrypted());
        assert!(!Classification::Absent.is_encrypted());
        assert!(Classification::Encrypted.is_encrypted());
    }
}
