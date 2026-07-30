//! Wrapping the DEK under the PIN-derived key.
//!
//! `XChaCha20-Poly1305(key = Argon2id(PIN, salt), nonce, DEK)`
//! (`storage.md` § What the key material file holds). XChaCha20 rather than
//! AES-GCM: a 192-bit nonce removes the nonce-collision analysis entirely, and
//! the wrap happens rarely enough that AES hardware acceleration is worth
//! nothing here.
//!
//! **The AEAD tag is what makes `attempts_remaining` exact.** A wrong PIN derives
//! a wrong key, the tag fails to verify, and the unwrap returns an error — so
//! "was the PIN wrong?" is an authenticated question with a yes-or-no answer,
//! not a guess about whether the decrypted bytes look like a key
//! ([contract §2](../../../docs/src/architecture/contract.md#unlock)).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::crypto::kdf::WrapKey;
use crate::crypto::{fill_random, Dek};
use crate::error::{ClipError, CryptoReason};

/// XChaCha20's nonce length.
pub const NONCE_LEN: usize = 24;

/// Bound into the AEAD as associated data.
///
/// It authenticates *what this ciphertext is for*. A blob lifted out of a
/// `keyfile` and presented somewhere else with the same wrap key would fail to
/// verify, and if a second wrapped thing is ever added to this file it cannot be
/// substituted for the DEK.
const CONTEXT: &[u8] = b"fastclip.keyfile.v1.wrapped_dek";

/// The wrapped DEK and the nonce it was wrapped under.
///
/// **No `deny_unknown_fields`**, on the migration rule: a later build's key
/// material must still read here.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedDek {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

/// Hand-written: this is the wrapped key, and criterion 6 names it among the
/// four things that must never reach a log.
impl std::fmt::Debug for WrappedDek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WrappedDek(<redacted>)")
    }
}

/// Wrap a DEK under a PIN-derived key.
///
/// **A fresh random nonce every time.** Reusing one across two wraps under the
/// same key is the classic way to break a stream cipher's confidentiality; with
/// 192 bits the chance of collision is negligible, which is exactly why
/// XChaCha20 was chosen over a 96-bit nonce that would have needed a counter.
pub fn wrap(key: &WrapKey, dek: &Dek) -> Result<WrappedDek, ClipError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    if let Err(error) = fill_random(&mut nonce_bytes) {
        nonce_bytes.zeroize();
        return Err(error);
    }

    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let payload = Payload {
        msg: dek.expose(),
        aad: CONTEXT,
    };

    match cipher.encrypt(XNonce::from_slice(&nonce_bytes), payload) {
        Ok(ciphertext) => Ok(WrappedDek {
            nonce: nonce_bytes,
            ciphertext,
        }),
        Err(_) => {
            // `aead::Error` is a unit struct with no detail — there is nothing
            // to log and nothing that could carry the key. Encryption failing
            // here is unreachable for a fixed-length message.
            nonce_bytes.zeroize();
            log::error!("the data encryption key could not be wrapped");
            Err(ClipError::Crypto {
                reason: CryptoReason::BadKeyMaterial,
            })
        }
    }
}

/// Unwrap a DEK, or report that this key does not open it.
///
/// **`Ok(None)` is a wrong PIN; `Err` is a broken file.** They are different
/// facts with different remedies — `bad_pin` tells the user to try again and
/// counts an attempt, `crypto { bad_key_material }` tells them the store belongs
/// to another Windows account — and collapsing them would either count a
/// corrupt file as a guess or tell a user who mistyped that their store is
/// gone. The caller decides which error to raise; this function only reports
/// which happened.
pub fn unwrap(key: &WrapKey, wrapped: &WrappedDek) -> Result<Option<Dek>, ClipError> {
    if wrapped.nonce.len() != NONCE_LEN {
        log::error!("the wrapped key carries a nonce of the wrong length");
        return Err(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial,
        });
    }

    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let payload = Payload {
        msg: &wrapped.ciphertext,
        aad: CONTEXT,
    };

    let mut plaintext = match cipher.decrypt(XNonce::from_slice(&wrapped.nonce), payload) {
        Ok(plaintext) => plaintext,
        // **The tag did not verify.** Either the PIN was wrong or the blob was
        // altered, and the AEAD cannot tell those apart. Not logged even at
        // debug: a wrong PIN is the ordinary case and logging one attempt per
        // wrong guess writes a record of how often the user fumbles.
        Err(_) => return Ok(None),
    };

    let recovered: Result<[u8; crate::crypto::dek::DEK_LEN], _> = plaintext.as_slice().try_into();
    let dek = match recovered {
        Ok(bytes) => Dek::from_bytes(bytes),
        Err(_) => {
            // The tag verified, so this was written by something holding the
            // wrap key, and it is still the wrong length. A file this build did
            // not write.
            plaintext.zeroize();
            log::error!("the wrapped key unwrapped to something that is not a 256-bit key");
            return Err(ClipError::Crypto {
                reason: CryptoReason::BadKeyMaterial,
            });
        }
    };

    plaintext.zeroize();
    Ok(Some(dek))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::kdf::{self, KdfParams, MIN_M_COST_KIB, MIN_T_COST, P_COST, SALT_LEN};
    use crate::crypto::Pin;

    fn cheap(salt: [u8; SALT_LEN]) -> KdfParams {
        KdfParams {
            m_cost_kib: MIN_M_COST_KIB,
            t_cost: MIN_T_COST,
            p_cost: P_COST,
            salt,
        }
    }

    fn key_for(pin_value: &str, params: &KdfParams) -> WrapKey {
        let pin = match Pin::parse("pin", Some(pin_value.to_owned())) {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture PIN should parse: {e}"),
        };
        match kdf::derive(&pin, params) {
            Ok(key) => key,
            Err(e) => panic!("the key should derive: {e}"),
        }
    }

    fn wrapped_under(pin_value: &str, params: &KdfParams, dek: &Dek) -> WrappedDek {
        match wrap(&key_for(pin_value, params), dek) {
            Ok(wrapped) => wrapped,
            Err(e) => panic!("the DEK should wrap: {e}"),
        }
    }

    #[test]
    fn the_right_pin_recovers_exactly_the_key_that_was_wrapped() {
        let params = cheap([0x11; SALT_LEN]);
        let dek = Dek::from_bytes([0x2b; 32]);
        let wrapped = wrapped_under("123456", &params, &dek);

        match unwrap(&key_for("123456", &params), &wrapped) {
            Ok(Some(recovered)) => assert_eq!(recovered, dek),
            Ok(None) => panic!("the correct PIN was rejected"),
            Err(e) => panic!("the unwrap should not have errored: {e}"),
        }
    }

    /// **`Ok(None)`, not `Err`.** The distinction is what lets `unlock` return
    /// `bad_pin` for a guess and `crypto` for a broken file.
    #[test]
    fn a_wrong_pin_is_a_clean_no_rather_than_an_error() {
        let params = cheap([0x11; SALT_LEN]);
        let wrapped = wrapped_under("123456", &params, &Dek::from_bytes([0x2b; 32]));

        for wrong in ["123457", "000000", "999999", "654321"] {
            match unwrap(&key_for(wrong, &params), &wrapped) {
                Ok(None) => {}
                Ok(Some(_)) => panic!("{wrong} unwrapped a DEK it should not have"),
                Err(e) => panic!("a wrong PIN must not be an error: {e}"),
            }
        }
    }

    /// Every wrap draws a new nonce, so the same DEK under the same PIN produces
    /// different bytes each time — and both still unwrap.
    #[test]
    fn two_wraps_of_the_same_key_differ_and_both_unwrap() {
        let params = cheap([0x11; SALT_LEN]);
        let dek = Dek::from_bytes([0x2b; 32]);
        let first = wrapped_under("123456", &params, &dek);
        let second = wrapped_under("123456", &params, &dek);

        assert_ne!(first.nonce, second.nonce, "a nonce must never be reused");
        assert_ne!(first.ciphertext, second.ciphertext);

        let key = key_for("123456", &params);
        assert_eq!(unwrap(&key, &first), Ok(Some(dek.clone())));
        assert_eq!(unwrap(&key, &second), Ok(Some(dek)));
    }

    /// The AEAD is what makes tampering detectable rather than silently
    /// producing a wrong key that would then be blamed on the user's PIN.
    #[test]
    fn a_flipped_bit_anywhere_in_the_wrap_is_refused() {
        let params = cheap([0x11; SALT_LEN]);
        let dek = Dek::from_bytes([0x2b; 32]);
        let key = key_for("123456", &params);
        let original = wrapped_under("123456", &params, &dek);

        for index in 0..original.ciphertext.len() {
            let mut tampered = original.clone();
            tampered.ciphertext[index] ^= 0x01;
            assert_eq!(
                unwrap(&key, &tampered),
                Ok(None),
                "a flipped ciphertext bit at {index} was accepted"
            );
        }

        let mut wrong_nonce = original.clone();
        wrong_nonce.nonce[0] ^= 0x01;
        assert_eq!(unwrap(&key, &wrong_nonce), Ok(None));
    }

    /// The associated data is bound in, so a blob wrapped for a different
    /// purpose under the same key cannot be substituted for the DEK.
    #[test]
    fn a_ciphertext_wrapped_without_the_context_does_not_verify() {
        let params = cheap([0x11; SALT_LEN]);
        let key = key_for("123456", &params);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
        let nonce = [0x07u8; NONCE_LEN];

        let foreign = match cipher.encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &[0x2bu8; 32],
                aad: b"some other purpose",
            },
        ) {
            Ok(ciphertext) => ciphertext,
            Err(_) => panic!("the fixture should encrypt"),
        };

        assert_eq!(
            unwrap(
                &key,
                &WrappedDek {
                    nonce,
                    ciphertext: foreign,
                }
            ),
            Ok(None)
        );
    }

    /// A truncated ciphertext is refused rather than producing a short key.
    #[test]
    fn a_truncated_wrap_is_refused() {
        let params = cheap([0x11; SALT_LEN]);
        let key = key_for("123456", &params);
        let mut wrapped = wrapped_under("123456", &params, &Dek::from_bytes([0x2b; 32]));
        wrapped.ciphertext.truncate(16);
        assert_eq!(unwrap(&key, &wrapped), Ok(None));

        wrapped.ciphertext.clear();
        assert_eq!(unwrap(&key, &wrapped), Ok(None));
    }

    #[test]
    fn the_debug_of_a_wrap_carries_none_of_its_bytes() {
        let params = cheap([0x11; SALT_LEN]);
        let wrapped = wrapped_under("123456", &params, &Dek::from_bytes([0xAB; 32]));
        let rendered = format!("{wrapped:?}");
        assert!(rendered.contains("redacted"), "{rendered}");
        assert!(
            !rendered.contains(&format!("{:?}", wrapped.ciphertext)),
            "{rendered}"
        );
        assert!(
            !rendered.contains(&format!("{:?}", wrapped.nonce)),
            "{rendered}"
        );
    }

    #[test]
    fn a_wrap_round_trips_through_json_and_tolerates_an_unknown_field() {
        let params = cheap([0x11; SALT_LEN]);
        let wrapped = wrapped_under("123456", &params, &Dek::from_bytes([0x2b; 32]));
        let json = match serde_json::to_string(&wrapped) {
            Ok(json) => json,
            Err(e) => panic!("the wrap should serialise: {e}"),
        };
        match serde_json::from_str::<WrappedDek>(&json) {
            Ok(back) => assert_eq!(back, wrapped),
            Err(e) => panic!("the wrap should deserialise: {e}"),
        }

        let widened = json.replace('}', r#","algorithm":"xchacha20poly1305"}"#);
        match serde_json::from_str::<WrappedDek>(&widened) {
            Ok(back) => assert_eq!(back, wrapped),
            Err(e) => panic!("a later build's key material must still read: {e}"),
        }
    }
}
