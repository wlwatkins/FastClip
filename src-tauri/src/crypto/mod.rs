//! Key material: the DEK, the KDF, the wrap, DPAPI, and the file that holds
//! them (WP-07, [ADR-0004](../../docs/src/architecture/adr/0004-optional-pin-encryption.md)).
//!
//! **Both factors are required, and this module is where that is true or
//! false.** Opening the store needs the file, the Windows account, and the PIN:
//!
//! ```text
//! keyfile = [version byte] ++ DPAPI( json{ kdf params, wrapped_dek, attempts } )
//!                                     └─ wrapped_dek =
//!                                        XChaCha20-Poly1305(Argon2id(PIN, salt), DEK)
//! ```
//!
//! Peel the outer layer and you need the Windows account. Peel the inner one and
//! you need the PIN. A design where either alone yields the DEK is the failure
//! ADR-0004 exists to prevent, and it is asserted directly in
//! [`tests/two_factor.rs`](../../src-tauri/tests/two_factor.rs) rather than
//! argued here.
//!
//! **The PIN is a gate, not the entropy.** The store's key is a random 256-bit
//! DEK. Argon2id is applied to the PIN only to make the *wrap* expensive to
//! attack offline; SQLCipher never sees a PIN-derived key
//! ([`kdf`], [`wrap`]).
//!
//! ## What must never reach a log
//!
//! The PIN, the DEK, the salt, the wrapped blob, and the DPAPI ciphertext —
//! at any level, under any threat model (ADR-0002, ADR-0012,
//! [criterion 6](../../docs/src/product/spec.md#8-acceptance-criteria)).
//!
//! Three habits enforce it here, because a rule that is only written down is a
//! rule that is one careless `{error}` away from being broken:
//!
//! 1. **No type in this module derives `Debug`.** [`Dek`] and the key-material
//!    struct write their own, and they redact.
//! 2. **No error from this module carries a value.** Every failure becomes a
//!    `ClipError` variant with no payload beyond a discriminant, and every
//!    `log::` call takes a fixed string.
//! 3. **A keying statement's error is never formatted.** `rusqlite`'s
//!    `SqlInputError` renders the SQL that produced it, and
//!    `PRAGMA key = '…'` *is* the DEK in hex
//!    ([`crate::storage::connection`]).
//!
//! `tests/log_content.rs` greps a real log file for all four secrets.

pub mod dek;
pub mod dpapi;
pub mod kdf;
pub mod keyfile;
pub mod pin;
pub mod wrap;

pub use dek::Dek;
pub use kdf::KdfParams;
pub use keyfile::KeyMaterial;
pub use pin::Pin;

/// Fill a buffer from the operating system's CSPRNG.
///
/// One place, so that "where does the randomness come from" has one answer. It
/// is `getrandom` by way of the AEAD crate's re-export rather than a
/// user-space PRNG seeded once: the DEK is the entropy the whole construction
/// rests on ([ADR-0004](../../docs/src/architecture/adr/0004-optional-pin-encryption.md)),
/// and a seeded generator would make it as good as its seeding.
///
/// **A failure here is fatal to the operation and never silently degrades.**
/// There is no fallback path that produces a weaker key: an OS RNG that cannot
/// answer means no key is generated at all.
pub(crate) fn fill_random(buffer: &mut [u8]) -> Result<(), crate::error::ClipError> {
    use chacha20poly1305::aead::rand_core::RngCore;

    let mut rng = chacha20poly1305::aead::OsRng;
    match rng.try_fill_bytes(buffer) {
        Ok(()) => Ok(()),
        Err(error) => {
            // The error describes the generator, never the buffer.
            log::error!("the operating system random number generator failed: {error}");
            Err(crate::error::ClipError::Storage)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a statistical test — that is the OS's job. This catches the one
    /// failure that would matter and would otherwise be invisible: a
    /// `fill_random` that returns `Ok` having written nothing, leaving a DEK of
    /// zeroes.
    #[test]
    fn fill_random_does_not_leave_the_buffer_untouched() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        assert!(fill_random(&mut first).is_ok());
        assert!(fill_random(&mut second).is_ok());

        assert_ne!(first, [0u8; 32], "a DEK of zeroes is not a key");
        assert_ne!(second, [0u8; 32]);
        assert_ne!(first, second, "two draws must not be identical");
    }

    #[test]
    fn fill_random_handles_an_empty_buffer() {
        assert!(fill_random(&mut []).is_ok());
    }
}
