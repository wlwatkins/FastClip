//! `ClipError` — the only error shape that crosses the IPC seam.
//!
//! The JSON in `docs/src/architecture/contract.md` §4 is the definition; the
//! serde attributes here are one way to produce it. A command that rejects with
//! anything else is a defect, because the frontend branches on `kind`.
//!
//! Nothing in this module may carry a clip `value`. Every variant is either a
//! discriminant, an identifier, a field name or a count, so the `Debug` derives
//! below cannot put a secret in a log line.

use serde::Serialize;

/// The error every command returns. Serialises to an object with a `kind`
/// discriminant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClipError {
    /// A `clip_id` argument names no stored clip.
    NotFound { clip_id: String },
    /// An argument failed validation. `field` is the wire field name.
    InvalidInput {
        field: String,
        reason: InvalidReason,
    },
    /// The store could not be read, written, created, or its directory reached.
    Storage,
    /// A user-chosen file — an export target or an import source — failed.
    /// Never the store itself.
    Io {
        operation: IoOperation,
        path: String,
        reason: IoReason,
    },
    /// Key material or the database itself could not be used.
    Crypto { reason: CryptoReason },
    /// An on-disk artefact carries a version this build does not know.
    UnsupportedVersion {
        component: VersionComponent,
        found: i64,
        supported: i64,
    },
    /// The import file's content was rejected.
    Import {
        reason: ImportReason,
        field: Option<String>,
        index: Option<u32>,
    },
    /// Encryption is on and the store is not open.
    Locked,
    /// The command needs the store in a state it is not in. `required` names the
    /// needed state, never the observed one.
    WrongState { required: RequiredState },
    /// The PIN was evaluated and did not unwrap the DEK.
    BadPin {
        attempts_remaining: Option<u32>,
        retry_after_ms: Option<u64>,
    },
    /// An unlock was attempted while a wait was already running. The PIN was not
    /// evaluated.
    Backoff { retry_after_ms: u64 },
    /// The clipboard could not be written.
    Clipboard,
    /// An unmodelled failure. No command may return this for a condition the
    /// contract models.
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidReason {
    Required,
    TooLong,
    ContainsControlCharacters,
    NotAPaletteToken,
    NotPermitted,
    UnknownField,
    MalformedUuid,
    NotAPermutation,
    NotSixDigits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IoOperation {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IoReason {
    NotFound,
    PermissionDenied,
    DiskFull,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CryptoReason {
    BadKeyMaterial,
    Corrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionComponent {
    Schema,
    KeyMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportReason {
    MalformedJson,
    UnsupportedVersion,
    MissingField,
    UnknownField,
    InvalidValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredState {
    Locked,
    Encrypted,
    Unencrypted,
}

impl std::fmt::Display for ClipError {
    /// Renders the discriminant only. The user-facing sentence comes from the
    /// copy deck on the frontend, and this text exists for logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { .. } => f.write_str("not_found"),
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid_input (field {field}, reason {reason:?})")
            }
            Self::Storage => f.write_str("storage"),
            Self::Io {
                operation, reason, ..
            } => write!(f, "io ({operation:?}, {reason:?})"),
            Self::Crypto { reason } => write!(f, "crypto ({reason:?})"),
            Self::UnsupportedVersion {
                component,
                found,
                supported,
            } => write!(
                f,
                "unsupported_version ({component:?}: found {found}, supported {supported})"
            ),
            Self::Import { reason, .. } => write!(f, "import ({reason:?})"),
            Self::Locked => f.write_str("locked"),
            Self::WrongState { required } => write!(f, "wrong_state (requires {required:?})"),
            Self::BadPin { .. } => f.write_str("bad_pin"),
            Self::Backoff { .. } => f.write_str("backoff"),
            Self::Clipboard => f.write_str("clipboard"),
            Self::Internal => f.write_str("internal"),
        }
    }
}

impl std::error::Error for ClipError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape is the contract. These assertions are character for
    /// character what `contract.md` §4 declares, so a serde attribute lost in a
    /// refactor fails here rather than on the frontend.
    #[test]
    fn variants_serialise_to_the_declared_wire_shape() {
        let cases: Vec<(ClipError, &str)> = vec![
            (
                ClipError::NotFound {
                    clip_id: "8a1f".into(),
                },
                r#"{"kind":"not_found","clip_id":"8a1f"}"#,
            ),
            (
                ClipError::InvalidInput {
                    field: "colour".into(),
                    reason: InvalidReason::NotAPaletteToken,
                },
                r#"{"kind":"invalid_input","field":"colour","reason":"not_a_palette_token"}"#,
            ),
            (ClipError::Storage, r#"{"kind":"storage"}"#),
            (
                ClipError::Io {
                    operation: IoOperation::Write,
                    path: "C:\\x.json".into(),
                    reason: IoReason::PermissionDenied,
                },
                r#"{"kind":"io","operation":"write","path":"C:\\x.json","reason":"permission_denied"}"#,
            ),
            (
                ClipError::Crypto {
                    reason: CryptoReason::Corrupt,
                },
                r#"{"kind":"crypto","reason":"corrupt"}"#,
            ),
            (
                ClipError::UnsupportedVersion {
                    component: VersionComponent::Schema,
                    found: 2,
                    supported: 1,
                },
                r#"{"kind":"unsupported_version","component":"schema","found":2,"supported":1}"#,
            ),
            (
                ClipError::UnsupportedVersion {
                    component: VersionComponent::KeyMaterial,
                    found: 2,
                    supported: 1,
                },
                r#"{"kind":"unsupported_version","component":"key_material","found":2,"supported":1}"#,
            ),
            (
                ClipError::Import {
                    reason: ImportReason::UnknownField,
                    field: Some("icon".into()),
                    index: Some(3),
                },
                r#"{"kind":"import","reason":"unknown_field","field":"icon","index":3}"#,
            ),
            (
                ClipError::Import {
                    reason: ImportReason::MalformedJson,
                    field: None,
                    index: None,
                },
                r#"{"kind":"import","reason":"malformed_json","field":null,"index":null}"#,
            ),
            (ClipError::Locked, r#"{"kind":"locked"}"#),
            (
                ClipError::WrongState {
                    required: RequiredState::Unencrypted,
                },
                r#"{"kind":"wrong_state","required":"unencrypted"}"#,
            ),
            (
                ClipError::BadPin {
                    attempts_remaining: Some(0),
                    retry_after_ms: Some(30000),
                },
                r#"{"kind":"bad_pin","attempts_remaining":0,"retry_after_ms":30000}"#,
            ),
            (
                ClipError::BadPin {
                    attempts_remaining: None,
                    retry_after_ms: None,
                },
                r#"{"kind":"bad_pin","attempts_remaining":null,"retry_after_ms":null}"#,
            ),
            (
                ClipError::Backoff {
                    retry_after_ms: 30000,
                },
                r#"{"kind":"backoff","retry_after_ms":30000}"#,
            ),
            (ClipError::Clipboard, r#"{"kind":"clipboard"}"#),
            (ClipError::Internal, r#"{"kind":"internal"}"#),
        ];

        for (error, expected) in cases {
            match serde_json::to_string(&error) {
                Ok(json) => assert_eq!(json, expected),
                Err(e) => panic!("{error} failed to serialise: {e}"),
            }
        }
    }

    #[test]
    fn every_invalid_reason_serialises_snake_case() {
        let cases = [
            (InvalidReason::Required, "\"required\""),
            (InvalidReason::TooLong, "\"too_long\""),
            (
                InvalidReason::ContainsControlCharacters,
                "\"contains_control_characters\"",
            ),
            (InvalidReason::NotAPaletteToken, "\"not_a_palette_token\""),
            (InvalidReason::NotPermitted, "\"not_permitted\""),
            (InvalidReason::UnknownField, "\"unknown_field\""),
            (InvalidReason::MalformedUuid, "\"malformed_uuid\""),
            (InvalidReason::NotAPermutation, "\"not_a_permutation\""),
            (InvalidReason::NotSixDigits, "\"not_six_digits\""),
        ];
        for (reason, expected) in cases {
            match serde_json::to_string(&reason) {
                Ok(json) => assert_eq!(json, expected),
                Err(e) => panic!("{reason:?} failed to serialise: {e}"),
            }
        }
    }
}
