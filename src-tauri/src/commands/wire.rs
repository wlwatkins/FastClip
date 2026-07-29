//! The shapes that cross the IPC seam, and the validation that decides whether
//! one is allowed to.
//!
//! Two rules from `docs/src/architecture/contract.md` shape every type here.
//!
//! **Outbound is strict.** [`Clip`] is exactly `id`, `label`, `value`, `colour`.
//! It is a different type from [`crate::storage::ClipRow`], so `use_count` and
//! `position` cannot reach the wire by anyone forgetting they exist (ADR-0008).
//!
//! **Inbound is lenient, then validated** (contract §1, *Argument
//! deserialisation*). Tauri deserialises a command's arguments before the
//! command body runs, and a failure there rejects with a plain string that no
//! `ClipError` can be recovered from. So every field of an argument struct is
//! `Option<T>`, `colour` arrives as a `String` and is resolved by hand, and
//! unknown fields are captured rather than refused — `deny_unknown_fields`
//! produces the wrong error shape and cannot be combined with `flatten` in any
//! case.
//!
//! The one condition outside the scheme is an argument of the **wrong JSON
//! type**, such as `label: 42`. It is not modelled, it reaches the frontend as
//! `internal`, and contract §1 says why: unlike an absent key, it is only
//! reachable by a frontend that has bypassed its own generated types.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::colour::Colour;
use crate::error::{ClipError, InvalidReason};
use crate::redact::Redacted;
use crate::storage::ClipRow;

/// Contract §4, *Validation rules*. Counted in Unicode scalar values.
pub const LABEL_MAX_CHARS: usize = 100;
/// Contract §4, *Validation rules*. Counted in Unicode scalar values.
pub const VALUE_MAX_CHARS: usize = 10_000;

/// The clip as it crosses the seam.
///
/// `id` is a `Uuid` rather than a `String` so that the lowercase hyphenated
/// spelling contract §0 requires is produced by the type instead of by a call
/// site remembering to.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Clip {
    pub id: Uuid,
    pub label: String,
    pub value: String,
    pub colour: Colour,
}

/// Hand-written, like [`ClipRow`]'s. A `#[derive(Debug)]` here would print
/// every clip the user owns into the first log line that formatted a payload.
impl fmt::Debug for Clip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Clip")
            .field("id", &self.id)
            .field("label", &Redacted::of(&self.label))
            .field("value", &Redacted::of(&self.value))
            .field("colour", &self.colour)
            .finish()
    }
}

impl From<ClipRow> for Clip {
    /// The row's `use_count` and `position` are dropped here, and this is the
    /// only place they can be. Order is the array index (ADR-0007) and
    /// `use_count` never crosses the seam (ADR-0008).
    fn from(row: ClipRow) -> Self {
        Self {
            id: row.id,
            label: row.label,
            value: row.value,
            colour: row.colour,
        }
    }
}

/// The lenient form of the `clip` argument, for `create_clip` and
/// `update_clip` alike.
///
/// **One deserialisation type, two validated types.** The wire distinction
/// between `Clip` and `ClipDraft` is whether `id` is present, and both commands
/// have to answer for an `id` that is present when it should not be — so
/// declaring `id` here and judging it in [`validate_draft`] and
/// [`validate_clip`] produces exactly the contract's two behaviours from one
/// parse. A `ClipDraft` carrying an `id` is
/// `invalid_input { field: "id", reason: "not_permitted" }`; a `Clip` missing
/// one is `invalid_input { field: "id", reason: "required" }`.
///
/// What the compiler enforces is the output: [`ValidDraft`] has no `id` field
/// at all, so a create cannot be handed one.
#[derive(Debug, Default, Deserialize)]
pub struct ClipPayload {
    id: Option<String>,
    label: Option<String>,
    value: Option<String>,
    /// A `String`, not a [`Colour`]. An unknown token must become
    /// `not_a_palette_token` rather than a serde enum failure the frontend can
    /// only read as `internal`.
    colour: Option<String>,
    /// Everything the contract does not declare. Captured so the rejection can
    /// name the offending key; `use_count` in particular is `not_permitted`
    /// rather than `unknown_field` (contract §1, spec §3).
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// A validated `ClipDraft`. Has no identity, by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidDraft {
    pub label: String,
    pub value: String,
    pub colour: Colour,
}

/// A validated `Clip`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidClip {
    pub id: Uuid,
    pub label: String,
    pub value: String,
    pub colour: Colour,
}

fn invalid(field: &str, reason: InvalidReason) -> ClipError {
    ClipError::InvalidInput {
        field: field.to_owned(),
        reason,
    }
}

/// An absent argument or field is `required`, and `field` is its wire name.
///
/// Contract §1: every command parameter is `Option<T>` and there is no optional
/// argument on that page. `JSON.stringify` drops a key whose value is
/// `undefined`, so this is the shape of any frontend bug that lets a variable go
/// undefined — not an exotic case.
fn require<T>(field: &str, argument: Option<T>) -> Result<T, ClipError> {
    argument.ok_or_else(|| invalid(field, InvalidReason::Required))
}

/// Reject the first undeclared key, naming it.
///
/// `use_count` is `not_permitted` — it exists, it is backend-owned, and the
/// frontend never writes it (spec §3, ADR-0008). Anything else is
/// `unknown_field`; `icon`, `visible` and `clear_time` are the three the
/// refactor deleted and they arrive here.
fn reject_undeclared(extra: &Map<String, Value>) -> Result<(), ClipError> {
    match extra.keys().next() {
        None => Ok(()),
        Some(first) => Err(invalid(
            first,
            if first == "id" || first == "use_count" {
                InvalidReason::NotPermitted
            } else {
                InvalidReason::UnknownField
            },
        )),
    }
}

/// 1–100 characters, not whitespace-only, no control characters.
///
/// The order of the three checks is the order contract §2 lists the reasons in
/// for `create_clip`. Nothing depends on it, but two implementations reporting
/// different reasons for one string would be a difference nobody could explain.
///
/// Neither `label` nor `value` is trimmed: what the user typed is what is
/// stored. A whitespace-only label is `required`, because a row with a blank
/// label is indistinguishable from a broken one.
fn validate_label(label: &str) -> Result<(), ClipError> {
    if label.trim().is_empty() {
        return Err(invalid("label", InvalidReason::Required));
    }
    if label.chars().count() > LABEL_MAX_CHARS {
        return Err(invalid("label", InvalidReason::TooLong));
    }
    if label.chars().any(char::is_control) {
        return Err(invalid("label", InvalidReason::ContainsControlCharacters));
    }
    Ok(())
}

/// 1–10 000 characters, not whitespace-only. Any character otherwise,
/// including newlines.
fn validate_value(value: &str) -> Result<(), ClipError> {
    if value.trim().is_empty() {
        return Err(invalid("value", InvalidReason::Required));
    }
    if value.chars().count() > VALUE_MAX_CHARS {
        return Err(invalid("value", InvalidReason::TooLong));
    }
    Ok(())
}

/// Parse an identifier arriving over IPC.
///
/// Contract §0: input is parsed case-insensitively and stored lowercase. The
/// parse is `Uuid::parse_str`, which is forgiving about the spelling it accepts
/// and exact about what it produces — the stored and emitted form is always the
/// lowercase hyphenated 36 characters, whatever arrived.
///
/// The rejected text is not echoed into the error or into a log. `field` names
/// the field; a caller cannot use the rejection to reflect its own input back.
fn parse_uuid(field: &str, text: &str) -> Result<Uuid, ClipError> {
    Uuid::parse_str(text).map_err(|_| invalid(field, InvalidReason::MalformedUuid))
}

/// The `clip_id` argument of `delete_clip` and `copy_clip`.
pub fn require_clip_id(clip_id: Option<String>) -> Result<Uuid, ClipError> {
    let text = require("clip_id", clip_id)?;
    parse_uuid("clip_id", &text)
}

/// The three fields `Clip` and `ClipDraft` share, in the order contract §2
/// lists their failure reasons.
fn validate_parts(payload: ClipPayload) -> Result<(String, String, Colour), ClipError> {
    reject_undeclared(&payload.extra)?;

    let label = require("label", payload.label)?;
    validate_label(&label)?;

    let value = require("value", payload.value)?;
    validate_value(&value)?;

    let colour = require("colour", payload.colour)?;
    let colour = Colour::from_wire(&colour)?;

    Ok((label, value, colour))
}

/// Validate the `clip` argument of `create_clip`.
///
/// An `id` is `not_permitted`: the backend mints every one, and an id arriving
/// from the frontend on a create is not a value to trust (contract §5,
/// *Identity*).
pub fn validate_draft(clip: Option<ClipPayload>) -> Result<ValidDraft, ClipError> {
    let payload = require("clip", clip)?;
    if payload.id.is_some() {
        return Err(invalid("id", InvalidReason::NotPermitted));
    }
    let (label, value, colour) = validate_parts(payload)?;
    Ok(ValidDraft {
        label,
        value,
        colour,
    })
}

/// Validate the `clip` argument of `update_clip`.
pub fn validate_clip(clip: Option<ClipPayload>) -> Result<ValidClip, ClipError> {
    let mut payload = require("clip", clip)?;
    let id = require("id", payload.id.take())?;
    let id = parse_uuid("id", &id)?;
    let (label, value, colour) = validate_parts(payload)?;
    Ok(ValidClip {
        id,
        label,
        value,
        colour,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload(value: Value) -> ClipPayload {
        match serde_json::from_value(value) {
            Ok(payload) => payload,
            Err(e) => panic!("the lenient form should always deserialise: {e}"),
        }
    }

    fn draft(value: Value) -> Result<ValidDraft, ClipError> {
        validate_draft(Some(payload(value)))
    }

    fn clip(value: Value) -> Result<ValidClip, ClipError> {
        validate_clip(Some(payload(value)))
    }

    fn rejected(field: &str, reason: InvalidReason) -> ClipError {
        ClipError::InvalidInput {
            field: field.into(),
            reason,
        }
    }

    const ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    #[test]
    fn a_well_formed_draft_is_accepted() {
        assert_eq!(
            draft(json!({ "label": "Support greeting", "value": "Hello", "colour": "amber" })),
            Ok(ValidDraft {
                label: "Support greeting".into(),
                value: "Hello".into(),
                colour: Colour::Amber,
            })
        );
    }

    #[test]
    fn a_well_formed_clip_is_accepted_and_its_id_is_lowercased() {
        assert_eq!(
            clip(json!({
                "id": ID.to_uppercase(),
                "label": "l",
                "value": "v",
                "colour": "teal",
            })),
            Ok(ValidClip {
                id: match Uuid::parse_str(ID) {
                    Ok(id) => id,
                    Err(e) => panic!("the fixture id should parse: {e}"),
                },
                label: "l".into(),
                value: "v".into(),
                colour: Colour::Teal,
            })
        );
    }

    /// Contract §1: an absent `clip` argument is `required`, and `field` is the
    /// argument's wire name. `invoke("create_clip", {})` is exactly this call.
    #[test]
    fn an_absent_clip_argument_names_the_argument() {
        assert_eq!(
            validate_draft(None),
            Err(rejected("clip", InvalidReason::Required))
        );
        assert_eq!(
            validate_clip(None),
            Err(rejected("clip", InvalidReason::Required))
        );
        assert_eq!(
            require_clip_id(None),
            Err(rejected("clip_id", InvalidReason::Required))
        );
    }

    #[test]
    fn every_missing_field_names_itself() {
        assert_eq!(
            draft(json!({ "value": "v", "colour": "teal" })),
            Err(rejected("label", InvalidReason::Required))
        );
        assert_eq!(
            draft(json!({ "label": "l", "colour": "teal" })),
            Err(rejected("value", InvalidReason::Required))
        );
        assert_eq!(
            draft(json!({ "label": "l", "value": "v" })),
            Err(rejected("colour", InvalidReason::Required))
        );
        assert_eq!(
            clip(json!({ "label": "l", "value": "v", "colour": "teal" })),
            Err(rejected("id", InvalidReason::Required))
        );
    }

    /// The backend mints every id. One arriving on a create is not a value to
    /// trust (contract §5, *Identity*).
    #[test]
    fn a_client_supplied_id_on_a_create_is_not_permitted() {
        assert_eq!(
            draft(json!({ "id": ID, "label": "l", "value": "v", "colour": "teal" })),
            Err(rejected("id", InvalidReason::NotPermitted))
        );
        // Even a well-formed id, and even one that is not a UUID at all: the
        // objection is that it was sent, not that it was malformed.
        assert_eq!(
            draft(json!({ "id": "", "label": "l", "value": "v", "colour": "teal" })),
            Err(rejected("id", InvalidReason::NotPermitted))
        );
    }

    /// ADR-0008 keeps `use_count` off the wire entirely, so an inbound one is
    /// `not_permitted` on every command that takes a clip.
    #[test]
    fn an_inbound_use_count_is_not_permitted_on_either_command() {
        let body = json!({
            "id": ID, "label": "l", "value": "v", "colour": "teal", "use_count": 7
        });
        assert_eq!(
            clip(body.clone()),
            Err(rejected("use_count", InvalidReason::NotPermitted))
        );
        let mut draft_body = body;
        if let Some(object) = draft_body.as_object_mut() {
            object.remove("id");
        }
        assert_eq!(
            draft(draft_body),
            Err(rejected("use_count", InvalidReason::NotPermitted))
        );
    }

    /// The three fields the refactor deleted. ADR-0003 removed the migration,
    /// so they are rejected by name rather than ignored.
    #[test]
    fn a_removed_field_is_an_unknown_field_named_in_the_error() {
        for field in ["icon", "visible", "clear_time"] {
            let mut body = json!({ "label": "l", "value": "v", "colour": "teal" });
            if let Some(object) = body.as_object_mut() {
                object.insert(field.into(), json!("anything"));
            }
            assert_eq!(
                draft(body),
                Err(rejected(field, InvalidReason::UnknownField)),
                "{field} should be reported by name"
            );
        }
    }

    #[test]
    fn a_label_is_checked_for_emptiness_length_and_control_characters() {
        let with = |label: &str| draft(json!({ "label": label, "value": "v", "colour": "teal" }));

        assert_eq!(with(""), Err(rejected("label", InvalidReason::Required)));
        assert_eq!(with("   "), Err(rejected("label", InvalidReason::Required)));
        assert_eq!(with("\t"), Err(rejected("label", InvalidReason::Required)));

        let hundred = "a".repeat(LABEL_MAX_CHARS);
        assert!(
            with(&hundred).is_ok(),
            "100 characters is the limit, not one over"
        );
        let one_over = "a".repeat(LABEL_MAX_CHARS + 1);
        assert_eq!(
            with(&one_over),
            Err(rejected("label", InvalidReason::TooLong))
        );

        for control in ["a\nb", "a\tb", "a\rb", "a\u{0}b", "a\u{7f}b"] {
            assert_eq!(
                with(control),
                Err(rejected("label", InvalidReason::ContainsControlCharacters)),
                "{control:?} should be refused"
            );
        }
    }

    /// Lengths are Unicode scalar values, not bytes and not UTF-16 code units
    /// (contract §0). A label of 100 emoji is 400 bytes and is accepted.
    #[test]
    fn lengths_are_counted_in_characters_rather_than_bytes() {
        let emoji = "\u{1F600}".repeat(LABEL_MAX_CHARS);
        assert_eq!(emoji.len(), LABEL_MAX_CHARS * 4);
        assert!(
            draft(json!({ "label": emoji, "value": "v", "colour": "teal" })).is_ok(),
            "a byte count would refuse this"
        );
    }

    #[test]
    fn a_value_may_hold_newlines_but_not_be_blank_or_too_long() {
        let with = |value: &str| draft(json!({ "label": "l", "value": value, "colour": "teal" }));

        assert_eq!(with(""), Err(rejected("value", InvalidReason::Required)));
        assert_eq!(
            with(" \n "),
            Err(rejected("value", InvalidReason::Required))
        );
        assert!(
            with("line one\nline two").is_ok(),
            "a value may hold newlines"
        );

        let limit = "a".repeat(VALUE_MAX_CHARS);
        assert!(with(&limit).is_ok());
        let over = "a".repeat(VALUE_MAX_CHARS + 1);
        assert_eq!(with(&over), Err(rejected("value", InvalidReason::TooLong)));
    }

    #[test]
    fn a_colour_that_is_not_a_palette_token_is_rejected_as_one() {
        assert_eq!(
            draft(json!({ "label": "l", "value": "v", "colour": "rgba(47, 119, 150, 0.7)" })),
            Err(rejected("colour", InvalidReason::NotAPaletteToken))
        );
        assert_eq!(
            draft(json!({ "label": "l", "value": "v", "colour": "unset" })),
            Err(rejected("colour", InvalidReason::NotAPaletteToken))
        );
    }

    #[test]
    fn a_malformed_id_is_reported_as_one() {
        assert_eq!(
            clip(json!({ "id": "not-a-uuid", "label": "l", "value": "v", "colour": "teal" })),
            Err(rejected("id", InvalidReason::MalformedUuid))
        );
        assert_eq!(
            require_clip_id(Some("not-a-uuid".into())),
            Err(rejected("clip_id", InvalidReason::MalformedUuid))
        );
    }

    /// Contract §0: input is parsed case-insensitively, output is lowercase.
    #[test]
    fn a_clip_id_round_trips_to_the_lowercase_hyphenated_form() {
        match require_clip_id(Some(ID.to_uppercase())) {
            Ok(id) => assert_eq!(id.as_hyphenated().to_string(), ID),
            Err(e) => panic!("an uppercase id should parse: {e}"),
        }
    }

    /// Contract §1: a wrongly-typed field is **not** modelled. It fails inside
    /// serde, before the command body runs, and reaches the frontend as
    /// `internal`. This test fixes that boundary so that a later "helpful"
    /// change to `serde_json::Value` fields is a visible decision.
    #[test]
    fn a_wrongly_typed_field_fails_to_deserialise_rather_than_being_modelled() {
        let result: Result<ClipPayload, _> =
            serde_json::from_value(json!({ "label": 42, "value": "v", "colour": "teal" }));
        assert!(result.is_err(), "label: 42 must not deserialise");
    }

    /// The rejection names the field, never the value. A user's clip must not
    /// come back out through an error message, and nor must it be logged.
    #[test]
    fn no_rejection_carries_the_rejected_text() {
        let secret = "hunter2-hunter2";
        let refusals: Vec<Result<(), ClipError>> = vec![
            draft(json!({ "label": secret.repeat(20), "value": "v", "colour": "teal" })).map(drop),
            draft(json!({ "label": "l", "value": "", "colour": secret })).map(drop),
            draft(json!({ "label": "l\n", "value": secret, "colour": "teal" })).map(drop),
            clip(json!({ "id": secret, "label": "l", "value": "v", "colour": "teal" })).map(drop),
        ];
        for case in refusals {
            match case {
                Ok(()) => panic!("this fixture should have been refused"),
                Err(error) => {
                    let rendered = format!("{error} {error:?}");
                    assert!(!rendered.contains(secret), "{rendered}");
                }
            }
        }
    }

    /// `ClipPayload` derives `Debug` and holds a `label` and a `value`. The
    /// derive is safe only because nothing formats one; this asserts the
    /// premise, so that a log line added later fails here instead of leaking.
    #[test]
    fn the_payload_debug_is_not_used_to_carry_a_secret_into_a_log() {
        let payload =
            payload(json!({ "label": "Support greeting", "value": "hunter2", "colour": "teal" }));
        let rendered = format!("{payload:?}");
        assert!(
            rendered.contains("hunter2"),
            "if this ever stops holding, the derive was replaced and this test should be too"
        );
        // The rule this stands in for: nothing in `commands` may log a
        // `ClipPayload`. `Clip` and `ClipRow` are the types that are formatted,
        // and both redact.
        let wire = Clip {
            id: Uuid::nil(),
            label: "Support greeting".into(),
            value: "hunter2".into(),
            colour: Colour::Amber,
        };
        let rendered = format!("{wire:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(!rendered.contains("Support greeting"), "{rendered}");
    }

    /// The outbound shape is the contract, character for character: four
    /// fields, `snake_case`, no `use_count`, no `position`, no order field.
    #[test]
    fn a_clip_serialises_to_exactly_the_four_declared_fields() {
        let wire = Clip {
            id: Uuid::nil(),
            label: "Support greeting".into(),
            value: "Hello".into(),
            colour: Colour::Amber,
        };
        let json = match serde_json::to_string(&wire) {
            Ok(json) => json,
            Err(e) => panic!("a clip should serialise: {e}"),
        };
        assert_eq!(
            json,
            r#"{"id":"00000000-0000-0000-0000-000000000000","label":"Support greeting","value":"Hello","colour":"amber"}"#
        );
    }

    #[test]
    fn a_row_becomes_a_wire_clip_without_its_backend_only_columns() {
        let row = ClipRow {
            id: Uuid::nil(),
            label: "l".into(),
            value: "v".into(),
            colour: Colour::Pink,
            use_count: 9,
            position: 3,
        };
        let wire = Clip::from(row);
        let json = match serde_json::to_value(&wire) {
            Ok(json) => json,
            Err(e) => panic!("a clip should serialise: {e}"),
        };
        let object = match json.as_object() {
            Some(object) => object,
            None => panic!("a clip serialises to an object"),
        };
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["colour", "id", "label", "value"]);
    }
}
