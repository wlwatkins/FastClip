//! The data encryption key.
//!
//! 256 bits of OS randomness. It is the entropy the whole construction rests on
//! ([ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md));
//! the PIN is a gate in front of it, not a source of key material.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::crypto::fill_random;
use crate::error::ClipError;

/// The DEK's length in bytes. SQLCipher takes exactly this as a raw key.
pub const DEK_LEN: usize = 32;

/// The key the store is encrypted with.
///
/// **Zeroised on drop**, which is what makes [`crate::commands`]' `lock` mean
/// something: ADR-0010 chose to close the database and give up the key rather
/// than flip a boolean, and a `Drop` that merely released the memory would
/// leave the key sitting in the freed page.
///
/// **No `Debug` derive, and the hand-written one prints nothing.** A `{:?}`
/// anywhere — a log line, a panic message, a `#[derive(Debug)]` on some future
/// struct that happens to hold one — must not be able to print this.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Dek([u8; DEK_LEN]);

impl Dek {
    /// A fresh key from the operating system's CSPRNG.
    ///
    /// Called once, by `enable_encryption`. There is no other source of DEKs
    /// and no path that derives one from a PIN.
    pub fn generate() -> Result<Self, ClipError> {
        let mut bytes = [0u8; DEK_LEN];
        match fill_random(&mut bytes) {
            Ok(()) => Ok(Self(bytes)),
            Err(error) => {
                bytes.zeroize();
                Err(error)
            }
        }
    }

    /// Rebuild a key from bytes that were just unwrapped.
    pub fn from_bytes(bytes: [u8; DEK_LEN]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for the wrap and for SQLCipher and nothing else.
    pub fn expose(&self) -> &[u8; DEK_LEN] {
        &self.0
    }
}

/// Constant-time equality.
///
/// **Used by `enable_encryption`'s step 1 read-back**, which unwraps the key
/// material it just wrote and asserts it recovers the DEK it generated
/// (storage.md § Switching encryption on and off). A `==` on the byte arrays
/// would short-circuit on the first differing byte; there is no attacker on
/// that path, but a key type that compares in variable time is a footgun left
/// lying about for the next call site.
impl PartialEq for Dek {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for Dek {}

/// Prints a fixed string. See the type's own documentation.
impl std::fmt::Debug for Dek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dek(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dek_of(byte: u8) -> Dek {
        Dek::from_bytes([byte; DEK_LEN])
    }

    #[test]
    fn a_generated_key_is_not_all_zeroes_and_two_are_different() {
        let first = match Dek::generate() {
            Ok(dek) => dek,
            Err(e) => panic!("a DEK should be generatable: {e}"),
        };
        let second = match Dek::generate() {
            Ok(dek) => dek,
            Err(e) => panic!("a DEK should be generatable: {e}"),
        };

        assert_ne!(first.expose(), &[0u8; DEK_LEN]);
        assert_ne!(first, second);
    }

    #[test]
    fn the_key_is_two_hundred_and_fifty_six_bits() {
        assert_eq!(DEK_LEN * 8, 256, "ADR-0004 specifies a 256-bit DEK");
    }

    #[test]
    fn equality_holds_for_the_same_bytes_and_fails_for_one_flipped_bit() {
        assert_eq!(dek_of(0x2b), dek_of(0x2b));

        let mut bytes = [0x2bu8; DEK_LEN];
        bytes[DEK_LEN - 1] ^= 0x01;
        assert_ne!(dek_of(0x2b), Dek::from_bytes(bytes));
    }

    /// ADR-0002 and criterion 6. A `{:?}` must not be a way to print the key,
    /// because a future struct deriving `Debug` around one would do exactly
    /// that without anybody deciding to.
    #[test]
    fn the_debug_of_a_key_carries_none_of_its_bytes() {
        let dek = Dek::from_bytes([0xAB; DEK_LEN]);
        let rendered = format!("{dek:?}");

        assert!(rendered.contains("redacted"), "{rendered}");
        assert!(!rendered.contains("ab"), "{rendered}");
        assert!(!rendered.contains("171"), "{rendered}");
        assert!(
            !rendered.contains(&format!("{:?}", [0xABu8; DEK_LEN])),
            "{rendered}"
        );
    }
}
