//! The 6-digit PIN, validated once at the boundary.
//!
//! Contract §2 declares `invalid_input { reason: "not_six_digits" }` on all four
//! commands that take one. Parsing it into a type here means the commands take a
//! [`Pin`] rather than a `String`, so "did anyone check this?" is answered by
//! the signature.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{ClipError, InvalidReason};

/// Exactly six, exactly ASCII digits (contract §2).
pub const PIN_LEN: usize = 6;

/// A validated PIN.
///
/// **Zeroised on drop, and no `Debug` derive**, on the same reasoning as
/// [`crate::crypto::Dek`]: the PIN is one of the four secrets criterion 6 says
/// must never reach a log, and a `{:?}` is how it would.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Pin(String);

impl Pin {
    /// Parse an argument from the wire.
    ///
    /// `None` is an absent argument — `JSON.stringify` drops a key whose value
    /// is `undefined` — and is `required` rather than `not_six_digits`, so the
    /// frontend can tell a bug in its own code from a user typing too few
    /// digits (contract §1, *Argument deserialisation*).
    ///
    /// **Counted in `char`s and checked with `is_ascii_digit`.** Contract §0
    /// counts every length in Unicode scalar values, and `is_numeric` would
    /// accept Arabic-Indic digits and Roman numerals, which are not what a
    /// 6-digit keypad produces and would widen the space the KDF is applied to
    /// in a way nothing else in the system expects.
    pub fn parse(field: &str, value: Option<String>) -> Result<Self, ClipError> {
        let value = match value {
            Some(value) => value,
            None => {
                return Err(ClipError::InvalidInput {
                    field: field.to_owned(),
                    reason: InvalidReason::Required,
                })
            }
        };

        let is_six_ascii_digits =
            value.chars().count() == PIN_LEN && value.chars().all(|c| c.is_ascii_digit());

        if !is_six_ascii_digits {
            // The rejected value is **not** logged and **not** echoed into the
            // error. It is a PIN, or a typo one keystroke away from one.
            let mut rejected = value;
            rejected.zeroize();
            return Err(ClipError::InvalidInput {
                field: field.to_owned(),
                reason: InvalidReason::NotSixDigits,
            });
        }

        Ok(Self(value))
    }

    /// The bytes the KDF consumes, and nothing else.
    pub fn expose(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl PartialEq for Pin {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        // Length is public — every PIN is six digits — so comparing the bytes
        // directly leaks nothing a constant-time compare would hide.
        self.0.as_bytes().ct_eq(other.0.as_bytes()).into()
    }
}

impl Eq for Pin {}

impl std::fmt::Debug for Pin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pin(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> Result<Pin, ClipError> {
        Pin::parse("pin", Some(value.to_owned()))
    }

    #[test]
    fn six_ascii_digits_are_accepted() {
        for value in ["123456", "000000", "999999"] {
            match parse(value) {
                Ok(pin) => assert_eq!(pin.expose(), value.as_bytes()),
                Err(e) => panic!("{value} should be a valid PIN: {e}"),
            }
        }
    }

    #[test]
    fn an_absent_argument_is_required_rather_than_not_six_digits() {
        assert_eq!(
            Pin::parse("pin", None).map(drop),
            Err(ClipError::InvalidInput {
                field: "pin".into(),
                reason: InvalidReason::Required,
            })
        );
    }

    #[test]
    fn anything_that_is_not_six_ascii_digits_is_rejected() {
        let rejected = [
            "",             // empty
            "12345",        // five
            "1234567",      // seven
            "12345a",       // a letter
            "12 456",       // a space
            "12345\n",      // a control character
            "-12345",       // a sign
            "١٢٣٤٥٦",       // Arabic-Indic digits: `is_numeric`, not `is_ascii_digit`
            "１２３４５６", // fullwidth digits, same trap
        ];
        for value in rejected {
            assert_eq!(
                parse(value).map(drop),
                Err(ClipError::InvalidInput {
                    field: "pin".into(),
                    reason: InvalidReason::NotSixDigits,
                }),
                "for {value:?}"
            );
        }
    }

    /// The field name travels, because `change_pin` takes two PINs and the
    /// frontend has to know which one it got wrong.
    #[test]
    fn the_rejection_names_the_argument_it_came_from() {
        for field in ["pin", "current_pin", "new_pin"] {
            assert_eq!(
                Pin::parse(field, Some("nope".into())).map(drop),
                Err(ClipError::InvalidInput {
                    field: field.into(),
                    reason: InvalidReason::NotSixDigits,
                })
            );
        }
    }

    #[test]
    fn the_debug_of_a_pin_carries_none_of_its_digits() {
        let pin = match parse("135790") {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture should parse: {e}"),
        };
        let rendered = format!("{pin:?}");
        assert!(rendered.contains("redacted"), "{rendered}");
        assert!(!rendered.contains("135790"), "{rendered}");
    }

    /// **The rejected value must not travel either.** A PIN one keystroke from
    /// the real one is still a secret, and an error that echoed it would put it
    /// into whatever the frontend logs and into any `{error}` on the way out.
    #[test]
    fn a_rejected_pin_is_not_echoed_in_the_error() {
        let secret = "1234567";
        let error = match parse(secret) {
            Ok(_) => panic!("seven digits should be rejected"),
            Err(error) => error,
        };
        assert!(!format!("{error}").contains(secret));
        assert!(!format!("{error:?}").contains(secret));
        match serde_json::to_string(&error) {
            Ok(json) => assert!(!json.contains(secret), "{json}"),
            Err(e) => panic!("the error should serialise: {e}"),
        }
    }

    #[test]
    fn equality_is_by_value() {
        let a = match parse("123456") {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture should parse: {e}"),
        };
        let b = match parse("123456") {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture should parse: {e}"),
        };
        let c = match parse("123457") {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture should parse: {e}"),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
