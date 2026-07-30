//! `~/.fast-clip/keyfile` — the only copy of the wrapped DEK.
//!
//! ```text
//! byte 0     : format version
//! bytes 1..  : DPAPI( serde_json( KeyMaterial ) )
//! ```
//!
//! **The version byte is outside the blob** so that an unrecognised format is
//! `unsupported_version { component: "key_material" }` rather than a DPAPI
//! failure — it is parsed before anything is decrypted, so every reader meets it
//! (`storage.md` § What the key material file holds, contract §2).
//!
//! **There is no second copy and no reset path.** A rewrite interrupted in place
//! destroys every clip in an encrypted store, so [`write`] is the only writer and
//! it goes through `keyfile.new`: write, flush, **close**, rename. The close is
//! part of the rule — Windows refuses to rename an open file, so flush-and-rename
//! would not write the key material at all
//! (`storage.md` § Every write to `keyfile` is atomic. Every one.).

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::crypto::kdf::KdfParams;
use crate::crypto::wrap::WrappedDek;
use crate::crypto::{dpapi, Dek};
use crate::error::{ClipError, CryptoReason, VersionComponent};
use crate::storage::StorePaths;

/// The format this build writes.
///
/// **1, and never 0.** `storage.md` § Version 0 makes a leading zero byte "a file
/// no FastClip wrote", which is what an empty or zero-filled file looks like.
pub const KEYFILE_VERSION: u8 = 1;

/// Attempts before a wait engages (ADR-0011).
pub const MAX_ATTEMPTS: u32 = 5;

/// The wait, flat, from the fifth failure onwards. No doubling, no ceiling
/// (ADR-0011).
pub const BACKOFF_MS: u64 = 30_000;

/// What the DPAPI blob holds, once unprotected.
///
/// **No `deny_unknown_fields`.** A `keyfile` written by a later build may carry
/// fields this one does not know; ignoring them is what lets the two coexist,
/// and rejecting them would make the store unopenable rather than merely
/// un-upgraded.
///
/// `failed_attempts` and `locked_until_unix_ms` default when absent, so a blob
/// from a build that never wrote them reads as a clean counter rather than
/// failing.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyMaterial {
    pub kdf: KdfParams,
    pub wrapped_dek: WrappedDek,
    #[serde(default)]
    pub failed_attempts: u32,
    #[serde(default)]
    pub locked_until_unix_ms: Option<u64>,
}

/// Hand-written. The salt and the wrapped DEK are two of the four secrets
/// criterion 6 names, and a `{:?}` on this struct is how they would reach a log.
impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyMaterial")
            .field("kdf", &self.kdf)
            .field("wrapped_dek", &self.wrapped_dek)
            .field("failed_attempts", &self.failed_attempts)
            .field("locked_until_unix_ms", &self.locked_until_unix_ms)
            .finish()
    }
}

impl KeyMaterial {
    /// A fresh file's contents: a new salt, the DEK wrapped under this PIN, and
    /// a clean counter.
    pub fn create(pin: &crate::crypto::Pin, dek: &Dek) -> Result<Self, ClipError> {
        let kdf = KdfParams::generate()?;
        let key = crate::crypto::kdf::derive(pin, &kdf)?;
        let wrapped_dek = crate::crypto::wrap::wrap(&key, dek)?;

        Ok(Self {
            kdf,
            wrapped_dek,
            failed_attempts: 0,
            locked_until_unix_ms: None,
        })
    }

    /// Attempts left before a wait engages. Never negative.
    pub fn attempts_remaining(&self) -> u32 {
        MAX_ATTEMPTS.saturating_sub(self.failed_attempts)
    }

    /// The wait still to run at `now_ms`, or `None`.
    ///
    /// `now_ms` is a parameter rather than a call to the clock, so the backoff
    /// arithmetic is testable without sleeping for thirty seconds.
    pub fn backoff_remaining_ms(&self, now_ms: u64) -> Option<u64> {
        match self.locked_until_unix_ms {
            Some(deadline) if deadline > now_ms => Some(deadline - now_ms),
            _ => None,
        }
    }

    /// Record a wrong PIN.
    ///
    /// The fifth failure and **every** one after it arms the same 30-second
    /// wait; the wait does not vary with the count (ADR-0011), so this re-arms
    /// rather than escalating.
    pub fn record_failure(&mut self, now_ms: u64) {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        if self.failed_attempts >= MAX_ATTEMPTS {
            self.locked_until_unix_ms = Some(now_ms.saturating_add(BACKOFF_MS));
        }
    }

    /// Record a successful unlock: the counter and any wait are cleared.
    pub fn record_success(&mut self) {
        self.failed_attempts = 0;
        self.locked_until_unix_ms = None;
    }
}

/// Wall-clock milliseconds since the Unix epoch.
///
/// A clock before the epoch reads as 0, which expires any pending wait rather
/// than panicking. Moving the system clock is a capability the user already has
/// and [ADR-0002](../../../docs/src/architecture/adr/0002-threat-model.md)
/// concedes; the backoff defends against someone guessing by hand at a machine,
/// not against its owner.
pub fn now_unix_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

/// Read and unprotect `keyfile`.
///
/// | Cause | Result |
/// | ----- | ------ |
/// | Absent, unreadable, empty | `crypto { bad_key_material }` |
/// | Leading byte 0 | `crypto { bad_key_material }` — a file no FastClip wrote |
/// | Leading byte above this build's | `unsupported_version { key_material }` |
/// | DPAPI refuses | `crypto { bad_key_material }` — another account or machine |
/// | The blob is not the expected JSON | `crypto { bad_key_material }` |
pub fn read(paths: &StorePaths) -> Result<KeyMaterial, ClipError> {
    let path = paths.keyfile();

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            // The error kind, never the path's contents.
            log::error!("the key material could not be read: {}", error.kind());
            return Err(bad_key_material());
        }
    };

    let (version, blob) = match bytes.split_first() {
        Some((version, blob)) => (*version, blob),
        None => {
            log::error!("the key material file is empty");
            return Err(bad_key_material());
        }
    };

    if version == 0 {
        // Not "version zero of a format" — no build ever wrote one. It is a
        // zero-filled or truncated file (storage.md § Version 0).
        log::error!("the key material carries a zero version byte; no build wrote this file");
        return Err(bad_key_material());
    }
    if version > KEYFILE_VERSION {
        log::error!(
            "the key material is at version {version}; this build understands {KEYFILE_VERSION}"
        );
        return Err(ClipError::UnsupportedVersion {
            component: VersionComponent::KeyMaterial,
            found: i64::from(version),
            supported: i64::from(KEYFILE_VERSION),
        });
    }

    let mut plaintext = dpapi::unprotect(blob)?;
    let parsed = serde_json::from_slice::<KeyMaterial>(&plaintext);
    plaintext.zeroize();

    match parsed {
        Ok(material) => {
            // The parameters are checked here, so every reader gets the check
            // and no caller has to remember it.
            material.kdf.validate()?;
            Ok(material)
        }
        Err(error) => {
            // **`error` is a serde message naming a field and an offset.** It
            // is emitted without the input, because the input is the
            // unprotected key material.
            log::error!(
                "the key material did not parse (line {}, column {})",
                error.line(),
                error.column()
            );
            Err(bad_key_material())
        }
    }
}

/// Write `keyfile` atomically: write, flush, close, rename.
///
/// **The only writer.** All four callers — `enable_encryption`, `change_pin`, a
/// failed `unlock` and a successful one — go through here, so the rule lives on
/// the file rather than in four places that each have to remember it.
///
/// A crash between the write and the rename leaves `keyfile.new`, which startup
/// recovery step 2 sweeps and no reader ever consults.
pub fn write(paths: &StorePaths, material: &KeyMaterial) -> Result<(), ClipError> {
    let mut body = match serde_json::to_vec(material) {
        Ok(body) => body,
        Err(error) => {
            log::error!("the key material could not be serialised: {error}");
            return Err(ClipError::Storage);
        }
    };

    let protected = dpapi::protect(&body);
    body.zeroize();
    let protected = protected?;

    let mut file_bytes = Vec::with_capacity(1 + protected.len());
    file_bytes.push(KEYFILE_VERSION);
    file_bytes.extend_from_slice(&protected);

    let temporary = paths.keyfile_new();
    if let Err(error) = write_flush_close(&temporary, &file_bytes) {
        log::error!("the key material could not be written: {}", error.kind());
        discard(&temporary);
        return Err(ClipError::Storage);
    }

    // The commit point. On Windows this replaces the target in one operation, so
    // a reader sees the old key material or the new one and never a partial
    // write — which for this file is the difference between a working store and
    // one nobody can open again.
    if let Err(error) = fs::rename(&temporary, paths.keyfile()) {
        log::error!(
            "the key material could not be renamed into place: {}",
            error.kind()
        );
        discard(&temporary);
        return Err(ClipError::Storage);
    }

    Ok(())
}

/// Write the whole body, flush it to the device, and **close it**, all before
/// returning.
///
/// The close is what the rename needs: Windows refuses to rename a file that is
/// still open. It happens because `file` is dropped at the end of this function,
/// which is a property of this being a function — **do not inline this into
/// [`write`]**, or the handle lives until the end of the caller and the rename
/// above fails on the one path where the store's only key was supposed to
/// change.
///
/// `sync_all` is what makes the rename a commit rather than a promise: a renamed
/// file whose contents are still in the operating system's cache is one a power
/// cut can truncate after the rename has already happened.
fn write_flush_close(path: &Path, body: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(body)?;
    file.sync_all()
}

/// Remove the temporary file, absorbing a failure.
///
/// It holds a DPAPI blob rather than anything readable, so leaving one discloses
/// nothing — but it is swept at the next launch regardless, and a failure to
/// delete must not replace the real error with one about a file the user never
/// asked about.
fn discard(temporary: &Path) {
    match fs::remove_file(temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!(
            "the temporary key material file could not be removed: {}",
            error.kind()
        ),
    }
}

/// Delete `keyfile.new` if a failed write left one. Absorbed.
///
/// Public because a conversion's abort has to sweep it too: `keyfile.new` is in
/// [`StorePaths::intermediates`], so the next launch would collect it, but
/// leaving one behind for a whole session is untidy where it costs one call to
/// avoid.
pub fn discard_intermediate(paths: &StorePaths) {
    discard(&paths.keyfile_new());
}

/// Delete `keyfile`. Used by `disable_encryption` step 9, where a failure is
/// absorbed and the stray file is removed at the next launch.
pub fn delete(paths: &StorePaths) -> std::io::Result<()> {
    match fs::remove_file(paths.keyfile()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn bad_key_material() -> ClipError {
    ClipError::Crypto {
        reason: CryptoReason::BadKeyMaterial,
    }
}

/// **The arithmetic only.** Everything in this module that touches a real
/// `keyfile` — writing it, reading it back, damaging it, rewriting it — lives in
/// `tests/keyfile.rs`, for the two reasons that file's header gives. What is
/// left here is the backoff, which is pure computation over a `KeyMaterial` and
/// reads no file.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::kdf::{MIN_M_COST_KIB, MIN_T_COST, P_COST, SALT_LEN};
    use crate::crypto::Pin;

    fn pin(value: &str) -> Pin {
        match Pin::parse("pin", Some(value.to_owned())) {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture PIN should parse: {e}"),
        }
    }

    /// The floor rather than the shipped tuple: these tests exercise the file,
    /// not the KDF's cost, and the shipped tuple would add seconds to each one.
    fn cheap_material(pin_value: &str, dek: &Dek) -> KeyMaterial {
        let kdf = KdfParams {
            m_cost_kib: MIN_M_COST_KIB,
            t_cost: MIN_T_COST,
            p_cost: P_COST,
            salt: [0x11; SALT_LEN],
        };
        let key = match crate::crypto::kdf::derive(&pin(pin_value), &kdf) {
            Ok(key) => key,
            Err(e) => panic!("the key should derive: {e}"),
        };
        let wrapped_dek = match crate::crypto::wrap::wrap(&key, dek) {
            Ok(wrapped) => wrapped,
            Err(e) => panic!("the DEK should wrap: {e}"),
        };
        KeyMaterial {
            kdf,
            wrapped_dek,
            failed_attempts: 0,
            locked_until_unix_ms: None,
        }
    }

    // ---- the backoff arithmetic (ADR-0011) ----

    fn fresh() -> KeyMaterial {
        cheap_material("123456", &Dek::from_bytes([0x2b; 32]))
    }

    #[test]
    fn a_clean_counter_has_five_attempts_and_no_wait() {
        let material = fresh();
        assert_eq!(material.attempts_remaining(), 5);
        assert_eq!(material.backoff_remaining_ms(now_unix_ms()), None);
    }

    /// The first four failures count down and arm nothing.
    #[test]
    fn the_first_four_failures_count_down_without_a_wait() {
        let mut material = fresh();
        for expected in [4u32, 3, 2, 1] {
            material.record_failure(1_000);
            assert_eq!(material.attempts_remaining(), expected);
            assert_eq!(material.backoff_remaining_ms(1_000), None);
        }
    }

    /// **The fifth arms exactly 30 seconds, and so does every one after it.**
    /// No doubling and no ceiling (ADR-0011).
    #[test]
    fn the_fifth_and_every_later_failure_arms_the_same_flat_wait() {
        let mut material = fresh();
        for _ in 0..4 {
            material.record_failure(1_000);
        }

        let mut now = 1_000u64;
        for attempt in 5..=20u32 {
            material.record_failure(now);
            assert_eq!(material.failed_attempts, attempt);
            assert_eq!(material.attempts_remaining(), 0);
            assert_eq!(
                material.backoff_remaining_ms(now),
                Some(BACKOFF_MS),
                "attempt {attempt} must wait exactly 30 seconds"
            );
            // The wait expires, and the next attempt arms the same wait again.
            now += BACKOFF_MS;
            assert_eq!(material.backoff_remaining_ms(now), None);
        }
    }

    #[test]
    fn a_wait_counts_down_and_then_expires() {
        let mut material = fresh();
        for _ in 0..5 {
            material.record_failure(10_000);
        }

        assert_eq!(material.backoff_remaining_ms(10_000), Some(30_000));
        assert_eq!(material.backoff_remaining_ms(25_000), Some(15_000));
        assert_eq!(material.backoff_remaining_ms(39_999), Some(1));
        assert_eq!(material.backoff_remaining_ms(40_000), None);
        assert_eq!(material.backoff_remaining_ms(u64::MAX), None);
    }

    #[test]
    fn a_success_clears_both_the_counter_and_the_wait() {
        let mut material = fresh();
        for _ in 0..7 {
            material.record_failure(1_000);
        }
        assert_eq!(material.attempts_remaining(), 0);
        assert!(material.backoff_remaining_ms(1_000).is_some());

        material.record_success();
        assert_eq!(material.failed_attempts, 0);
        assert_eq!(material.attempts_remaining(), MAX_ATTEMPTS);
        assert_eq!(material.backoff_remaining_ms(1_000), None);
    }

    /// Saturating rather than wrapping: a tampered counter at `u32::MAX` must
    /// not roll over to zero and hand the attacker five fresh attempts.
    #[test]
    fn an_absurd_counter_saturates_rather_than_wrapping() {
        let mut material = fresh();
        material.failed_attempts = u32::MAX;
        material.record_failure(1_000);
        assert_eq!(material.failed_attempts, u32::MAX);
        assert_eq!(material.attempts_remaining(), 0);

        material.locked_until_unix_ms = Some(u64::MAX);
        material.record_failure(u64::MAX);
        assert_eq!(material.backoff_remaining_ms(0), Some(u64::MAX));
    }
}
