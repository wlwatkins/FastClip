//! `ExportFile` — the on-disk shape `export_clips` writes and `import_clips`
//! reads.
//!
//! `docs/src/architecture/contract.md` §1 `ExportFile` is the definition. It is
//! **a file format, not an IPC payload**, and it is versioned separately from
//! everything else (contract § Versions on disk): this build writes and accepts
//! `version: 1`.
//!
//! Two rules shape everything here, and they pull in opposite directions.
//!
//! **Rendering is strict.** Four fields per clip, `use_count` never among them
//! (ADR-0008, spec §4.6): importing another machine's counts would corrupt the
//! ranking, so the column has no path onto the disk at all — the renderer takes
//! a [`crate::commands::Clip`], which does not carry one.
//!
//! **Parsing is forgiving about *shape* and strict about *content*.** The file
//! is read as a `serde_json::Value` and every field is judged by hand, because
//! the `import` variant carries a `field` name and an `index` that serde's own
//! error text would only give up as a substring. `deny_unknown_fields` appears
//! nowhere: it cannot name the field it refused, which is the one thing spec
//! §4.6 asks for.
//!
//! **Nothing in this module logs a clip.** A parse failure names the field and
//! the index, never the value — a rejected `value` is the user's secret whether
//! or not the file it came from was plaintext.

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::colour::Colour;
use crate::commands::wire::{Clip, LABEL_MAX_CHARS, VALUE_MAX_CHARS};
use crate::error::{ClipError, ImportReason};

/// The `format` marker. Anything else is not a FastClip export.
pub const FORMAT: &str = "fastclip-export";

/// The export format version this build writes and accepts.
pub const EXPORT_VERSION: i64 = 1;

/// The top-level keys, in the order they are written.
const TOP_LEVEL_FIELDS: [&str; 3] = ["format", "version", "clips"];

/// The keys a clip may carry. `id` is written and **ignored on import**
/// (contract §1 `ExportFile`): import mints a fresh id for every clip, so two
/// clips in one file carrying the same id produce two distinct clips.
const CLIP_FIELDS: [&str; 4] = ["id", "label", "value", "colour"];

/// A clip that has been read out of an export file and validated.
///
/// It has **no `id`**, by construction. Import mints one per clip, and a type
/// that could carry the file's id is a type from which one could be inserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedClip {
    pub label: String,
    pub value: String,
    pub colour: Colour,
}

fn import_error(reason: ImportReason, field: Option<&str>, index: Option<usize>) -> ClipError {
    ClipError::Import {
        reason,
        field: field.map(str::to_owned),
        // A file with more than `u32::MAX` clips cannot be built by this
        // application and could not be read into memory here in any case. The
        // saturating conversion is so that no arithmetic in this module can
        // panic on input nobody controls.
        index: index.map(|index| u32::try_from(index).unwrap_or(u32::MAX)),
    }
}

/// Render the export file's bytes.
///
/// Pretty-printed with a trailing newline. A SQLite store is not hand-readable
/// and export is the only user-facing recovery path this design has
/// (`storage.md` § No legacy path), so the one artefact a user can open in an
/// editor is written to be read in one.
///
/// This cannot fail on any input it can be given: `Clip` is four owned fields of
/// `Serialize` types with no map keys and no non-finite numbers. It returns a
/// `Result` regardless, because the alternative is an `unwrap()` on a path
/// reachable after startup.
pub fn render(clips: &[Clip]) -> Result<Vec<u8>, ClipError> {
    let document = serde_json::json!({
        "format": FORMAT,
        "version": EXPORT_VERSION,
        "clips": clips,
    });

    match serde_json::to_vec_pretty(&document) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Ok(bytes)
        }
        Err(error) => {
            // **The classification, not the error's own text.** A serialiser
            // error's `Display` is free to quote what it was given, and what it
            // was given here is every clip the user owns. `classify()` is a
            // three-valued enum and cannot carry one (ADR-0002). This is the
            // same rule `settings::read` follows for its parse failures.
            log::error!(
                "the export document could not be serialised ({:?})",
                error.classify()
            );
            Err(ClipError::Internal)
        }
    }
}

/// Parse and validate a whole export file.
///
/// **Phase one of the import** (contract, `import_clips`): the first failure
/// aborts with an `import` error naming the reason, the field and the index of
/// the offending clip, and nothing has been written because nothing has yet
/// touched the store. A file whose tenth clip is bad imports none of the first
/// nine.
///
/// A leading UTF-8 byte order mark is skipped. An export file this build wrote
/// has none, but a user who opened one in Notepad to fix a typo has one, and
/// refusing their whole backup over three bytes serves nobody.
pub fn parse(bytes: &[u8]) -> Result<Vec<ImportedClip>, ClipError> {
    let text = match std::str::from_utf8(strip_bom(bytes)) {
        Ok(text) => text,
        Err(_) => return Err(import_error(ImportReason::MalformedJson, None, None)),
    };

    let document: Value = match serde_json::from_str(text) {
        Ok(document) => document,
        Err(_) => return Err(import_error(ImportReason::MalformedJson, None, None)),
    };

    let object = match document.as_object() {
        Some(object) => object,
        // A JSON array, string or number is well-formed JSON and is still not
        // an export file.
        None => return Err(import_error(ImportReason::MalformedJson, None, None)),
    };

    check_format(object)?;
    // **Before the unknown-field check, and that ordering is the point.** A file
    // written by a later build may carry a top-level field this one has never
    // heard of; reporting "unknown field" for it would send the user looking for
    // a typo when the true answer is that their FastClip is too old.
    check_version(object)?;
    reject_unknown(object, &TOP_LEVEL_FIELDS, None)?;

    let clips = match object.get("clips") {
        None => {
            return Err(import_error(
                ImportReason::MissingField,
                Some("clips"),
                None,
            ))
        }
        Some(Value::Array(clips)) => clips,
        Some(_) => {
            return Err(import_error(
                ImportReason::InvalidValue,
                Some("clips"),
                None,
            ))
        }
    };

    clips
        .iter()
        .enumerate()
        .map(|(index, clip)| parse_clip(clip, index))
        .collect()
}

/// A UTF-8 byte order mark, if the file starts with one.
fn strip_bom(bytes: &[u8]) -> &[u8] {
    match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => rest,
        None => bytes,
    }
}

fn check_format(object: &Map<String, Value>) -> Result<(), ClipError> {
    match object.get("format") {
        None => Err(import_error(
            ImportReason::MissingField,
            Some("format"),
            None,
        )),
        Some(Value::String(format)) if format == FORMAT => Ok(()),
        // Contract §1 `ExportFile`: anything else is `malformed_json`, and not
        // `invalid_value`. The file may be perfectly well-formed JSON; what it
        // is not is a FastClip export, and that is the sentence the copy deck
        // has for this reason.
        Some(_) => Err(import_error(
            ImportReason::MalformedJson,
            Some("format"),
            None,
        )),
    }
}

/// Judge the `version` header.
///
/// A `version` **above** [`EXPORT_VERSION`] is `unsupported_version`; a
/// `version` below it is `malformed_json`, because only a higher value can have
/// been written by a FastClip (contract §1 `ExportFile`, *A version below 1 is
/// not a version problem*, which carries the argument).
///
/// The boundary is the lowest version this build can read, which is
/// [`EXPORT_VERSION`] only while that is also the only version. When a version 2
/// exists, a version 1 file is below the current version and must still be read.
fn check_version(object: &Map<String, Value>) -> Result<(), ClipError> {
    match object.get("version") {
        None => Err(import_error(
            ImportReason::MissingField,
            Some("version"),
            None,
        )),
        Some(Value::Number(version)) => match version.as_i64() {
            Some(EXPORT_VERSION) => Ok(()),
            // A file from a newer FastClip. The user's build is too old, which
            // is what the copy deck's sentence for this reason says.
            Some(version) if version > EXPORT_VERSION => Err(import_error(
                ImportReason::UnsupportedVersion,
                Some("version"),
                None,
            )),
            // No build ever wrote a version below 1, so this is not an export
            // from another version — it is not a FastClip export at all, and
            // gets the same answer a `format` no build ever wrote gets.
            Some(_) => Err(import_error(
                ImportReason::MalformedJson,
                Some("version"),
                None,
            )),
            // `1.5`, or an integer outside `i64`. Not a version at all.
            None => Err(import_error(
                ImportReason::InvalidValue,
                Some("version"),
                None,
            )),
        },
        Some(_) => Err(import_error(
            ImportReason::InvalidValue,
            Some("version"),
            None,
        )),
    }
}

/// Reject the first undeclared key, naming it.
///
/// **The first is the lexicographically smallest**, because `serde_json::Map` is
/// a `BTreeMap` while the crate's `preserve_order` feature is off. Contract §1
/// states that dependency for the argument path and it holds identically here: a
/// transitive dependency enabling that feature would silently change which field
/// a rejection names. A test in this module fixes the behaviour so the change
/// would be visible.
fn reject_unknown(
    object: &Map<String, Value>,
    declared: &[&str],
    index: Option<usize>,
) -> Result<(), ClipError> {
    match object.keys().find(|key| !declared.contains(&key.as_str())) {
        None => Ok(()),
        // `use_count` included. Contract §1 `ExportFile` rejects it here as an
        // unknown field rather than as `not_permitted`, which is an
        // `InvalidReason` and has no counterpart in `ImportReason`.
        Some(first) => Err(import_error(ImportReason::UnknownField, Some(first), index)),
    }
}

fn parse_clip(clip: &Value, index: usize) -> Result<ImportedClip, ClipError> {
    let object = match clip.as_object() {
        Some(object) => object,
        None => return Err(import_error(ImportReason::InvalidValue, None, Some(index))),
    };

    reject_unknown(object, &CLIP_FIELDS, Some(index))?;

    // `id` is read by nobody. Contract §1 `ExportFile`: written on export,
    // ignored on import, and it may be absent. It is not even checked for being
    // a UUID — a file whose ids are nonsense still holds the user's clips, and
    // refusing it would fail an import over the one field this build discards.

    let label = require_string(object, "label", index)?;
    validate_label(&label, index)?;

    let value = require_string(object, "value", index)?;
    validate_value(&value, index)?;

    let colour = require_string(object, "colour", index)?;
    let colour = match Colour::parse_token(&colour) {
        Some(colour) => colour,
        // Resolved at review 005: an unknown token inside an import file is
        // `invalid_value`, not `not_a_palette_token`. `ImportReason` has no
        // palette variant, and `invalid_value` is the catch-all for every
        // field-value failure on this path.
        None => {
            return Err(import_error(
                ImportReason::InvalidValue,
                Some("colour"),
                Some(index),
            ))
        }
    };

    Ok(ImportedClip {
        label,
        value,
        colour,
    })
}

fn require_string(
    object: &Map<String, Value>,
    field: &str,
    index: usize,
) -> Result<String, ClipError> {
    match object.get(field) {
        None => Err(import_error(
            ImportReason::MissingField,
            Some(field),
            Some(index),
        )),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(_) => Err(import_error(
            ImportReason::InvalidValue,
            Some(field),
            Some(index),
        )),
    }
}

/// The same limits the IPC boundary applies (contract §4, *Validation rules*),
/// counted in the same unit.
///
/// They are re-applied rather than trusted because an import file is not a
/// payload this application wrote: a clip that got past this check would sit in
/// the store unrenderable, and the reason import validates the whole file first
/// is so that it never gets there.
fn validate_label(label: &str, index: usize) -> Result<(), ClipError> {
    let invalid = label.trim().is_empty()
        || label.chars().count() > LABEL_MAX_CHARS
        || label.chars().any(char::is_control);
    if invalid {
        // One reason for all three faults: `ImportReason` has no `too_long` and
        // no `contains_control_characters`, and inventing a distinction the
        // contract does not carry would put a value the frontend cannot map on
        // the wire.
        return Err(import_error(
            ImportReason::InvalidValue,
            Some("label"),
            Some(index),
        ));
    }
    Ok(())
}

fn validate_value(value: &str, index: usize) -> Result<(), ClipError> {
    if value.trim().is_empty() || value.chars().count() > VALUE_MAX_CHARS {
        return Err(import_error(
            ImportReason::InvalidValue,
            Some("value"),
            Some(index),
        ));
    }
    Ok(())
}

/// The clips of an export file, as the wire type the renderer takes.
///
/// Used by the readback [`crate::commands::export_import::export_clips`] runs
/// before it renames the temporary file into place: what it needs is the count,
/// and the count is only trustworthy if the file parsed.
pub fn parsed_count(bytes: &[u8]) -> Result<usize, ClipError> {
    parse(bytes).map(|clips| clips.len())
}

/// A fresh id for an imported clip.
///
/// Here rather than at the call site so that "the backend mints every id"
/// (contract §5, *Identity*) is one function on the import path too, and the
/// file's own `id` field has nowhere to be read from.
pub fn mint_id() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn clip(label: &str, value: &str, colour: Colour) -> Clip {
        Clip {
            id: match Uuid::parse_str("3f2504e0-4f89-41d3-9a0c-0305e82c3301") {
                Ok(id) => id,
                Err(e) => panic!("the fixture id should parse: {e}"),
            },
            label: label.into(),
            value: value.into(),
            colour,
        }
    }

    fn rendered(clips: &[Clip]) -> Vec<u8> {
        match render(clips) {
            Ok(bytes) => bytes,
            Err(e) => panic!("the render should succeed: {e}"),
        }
    }

    fn document(bytes: &[u8]) -> Value {
        match serde_json::from_slice(bytes) {
            Ok(document) => document,
            Err(e) => panic!("the rendered file should be JSON: {e}"),
        }
    }

    fn bytes_of(value: &Value) -> Vec<u8> {
        value.to_string().into_bytes()
    }

    /// The length up to and including the last byte that is not whitespace.
    ///
    /// A truncation test cuts to every prefix of a file, and the prefixes that
    /// only drop trailing whitespace are still the whole document — asserting
    /// that one of those fails would be asserting that JSON is
    /// whitespace-sensitive.
    fn significant_length(bytes: &[u8]) -> usize {
        match bytes.iter().rposition(|byte| !byte.is_ascii_whitespace()) {
            Some(last) => last + 1,
            None => 0,
        }
    }

    fn imported(value: Value) -> Result<Vec<ImportedClip>, ClipError> {
        parse(&bytes_of(&value))
    }

    fn rejected(
        reason: ImportReason,
        field: Option<&str>,
        index: Option<u32>,
    ) -> Result<Vec<ImportedClip>, ClipError> {
        Err(ClipError::Import {
            reason,
            field: field.map(str::to_owned),
            index,
        })
    }

    /// One clip in the shape contract §1 `ExportFile` prints.
    fn file(clips: Value) -> Value {
        json!({ "format": FORMAT, "version": EXPORT_VERSION, "clips": clips })
    }

    fn one_clip() -> Value {
        json!({
            "id": "8a1f0c6e-0000-4000-8000-000000000001",
            "label": "Support greeting",
            "value": "Hello, thank you for waiting",
            "colour": "amber",
        })
    }

    // ---- rendering ----

    #[test]
    fn the_rendered_file_carries_the_declared_envelope_and_four_fields_per_clip() {
        let bytes = rendered(&[clip("Support greeting", "Hello", Colour::Amber)]);
        let document = document(&bytes);

        assert_eq!(document.get("format"), Some(&json!(FORMAT)));
        assert_eq!(document.get("version"), Some(&json!(1)));

        let clips = match document.get("clips").and_then(Value::as_array) {
            Some(clips) => clips.clone(),
            None => panic!("clips is an array"),
        };
        assert_eq!(clips.len(), 1);
        let object = match clips[0].as_object() {
            Some(object) => object.clone(),
            None => panic!("a clip is an object"),
        };
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["colour", "id", "label", "value"]);
    }

    /// ADR-0008 and spec §4.6. The count describes how this machine was used,
    /// not what a clip is, and there is no path by which it can reach the file:
    /// the renderer's input type does not carry one.
    #[test]
    fn use_count_appears_nowhere_in_a_rendered_file() {
        let bytes = rendered(&[
            clip("first", "a value", Colour::Teal),
            clip("second", "another", Colour::Pink),
        ]);
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(e) => panic!("the file should be UTF-8: {e}"),
        };
        assert!(!text.contains("use_count"), "{text}");
        assert!(!text.contains("position"), "{text}");
    }

    #[test]
    fn an_empty_store_renders_an_empty_clips_array_rather_than_failing() {
        let bytes = rendered(&[]);
        assert_eq!(document(&bytes).get("clips"), Some(&json!([])));
        assert_eq!(parse(&bytes), Ok(vec![]));
    }

    /// The one artefact a user can open in an editor when the store will not.
    #[test]
    fn the_file_is_pretty_printed_and_ends_with_a_newline() {
        let bytes = rendered(&[clip("first", "a value", Colour::Teal)]);
        assert_eq!(bytes.last(), Some(&b'\n'));
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(e) => panic!("the file should be UTF-8: {e}"),
        };
        assert!(text.contains("\n  \"format\""), "{text}");
    }

    // ---- the round trip ----

    #[test]
    fn every_clip_survives_a_render_and_a_parse_unchanged() {
        let clips = vec![
            clip(
                "Support greeting",
                "Hello, thank you for waiting",
                Colour::Amber,
            ),
            clip("multi line", "line one\nline two\ttabbed", Colour::Slate),
            clip(
                "unicode \u{1F600}",
                "\u{4F60}\u{597D} \u{1F600}",
                Colour::Violet,
            ),
            clip(
                "quote \" and \\ backslash",
                "{\"not\": \"json\"}",
                Colour::Red,
            ),
        ];
        let parsed = match parse(&rendered(&clips)) {
            Ok(parsed) => parsed,
            Err(e) => panic!("the round trip should succeed: {e}"),
        };

        let expected: Vec<ImportedClip> = clips
            .iter()
            .map(|clip| ImportedClip {
                label: clip.label.clone(),
                value: clip.value.clone(),
                colour: clip.colour,
            })
            .collect();
        assert_eq!(parsed, expected);
    }

    /// The limits are the boundary's, so anything the store can hold survives
    /// its own export. A label of exactly 100 characters must not come back as
    /// an `invalid_value`.
    #[test]
    fn a_clip_at_every_limit_survives_the_round_trip() {
        let clips = vec![clip(
            &"a".repeat(LABEL_MAX_CHARS),
            &"b".repeat(VALUE_MAX_CHARS),
            Colour::Green,
        )];
        match parse(&rendered(&clips)) {
            Ok(parsed) => {
                assert_eq!(parsed.len(), 1);
                assert_eq!(parsed[0].label.chars().count(), LABEL_MAX_CHARS);
                assert_eq!(parsed[0].value.chars().count(), VALUE_MAX_CHARS);
            }
            Err(e) => panic!("a clip at the limits should survive: {e}"),
        }
    }

    /// Every palette token, so a colour added to the enum without a thought for
    /// this path fails here.
    #[test]
    fn every_palette_token_survives_the_round_trip() {
        let clips: Vec<Clip> = crate::colour::COLOUR_TOKENS
            .iter()
            .map(|colour| clip("l", "v", *colour))
            .collect();
        match parse(&rendered(&clips)) {
            Ok(parsed) => {
                let tokens: Vec<Colour> = parsed.iter().map(|clip| clip.colour).collect();
                assert_eq!(tokens, crate::colour::COLOUR_TOKENS.to_vec());
            }
            Err(e) => panic!("every token should survive: {e}"),
        }
    }

    /// Contract §1 `ExportFile`: the id is written and ignored, so two clips in
    /// one file carrying the same id produce two distinct clips. Nothing in the
    /// parsed output can carry the file's id — the type has no field for one.
    #[test]
    fn the_files_ids_are_ignored_however_they_are_spelt() {
        let duplicated = "8a1f0c6e-0000-4000-8000-000000000001";
        let parsed = imported(file(json!([
            { "id": duplicated, "label": "first", "value": "v", "colour": "teal" },
            { "id": duplicated, "label": "second", "value": "v", "colour": "teal" },
            { "label": "third", "value": "v", "colour": "teal" },
            { "id": "not a uuid at all", "label": "fourth", "value": "v", "colour": "teal" },
            { "id": null, "label": "fifth", "value": "v", "colour": "teal" },
            { "id": 7, "label": "sixth", "value": "v", "colour": "teal" },
        ])));

        match parsed {
            Ok(clips) => {
                let labels: Vec<&str> = clips.iter().map(|clip| clip.label.as_str()).collect();
                assert_eq!(
                    labels,
                    vec!["first", "second", "third", "fourth", "fifth", "sixth"]
                );
            }
            Err(e) => panic!("the id must never fail an import: {e}"),
        }
    }

    #[test]
    fn an_empty_clips_array_parses_to_nothing_and_is_not_an_error() {
        assert_eq!(imported(file(json!([]))), Ok(vec![]));
    }

    /// Forgiving on input: a user who opened their backup in Notepad has a byte
    /// order mark, and their clips are still in there.
    #[test]
    fn a_leading_byte_order_mark_is_skipped_rather_than_being_fatal() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(&bytes_of(&file(json!([one_clip()]))));
        match parse(&bytes) {
            Ok(clips) => assert_eq!(clips.len(), 1),
            Err(e) => panic!("a byte order mark should not fail an import: {e}"),
        }
    }

    // ---- the whole file is rejected ----

    #[test]
    fn a_file_that_is_not_json_is_malformed_json() {
        for fixture in [
            &b""[..],
            b"not json at all",
            b"{",
            b"{\"format\":",
            b"[]",
            b"\"a string\"",
            b"42",
            b"null",
            // Invalid UTF-8: a lone continuation byte.
            &[0x7B, 0xFF, 0x7D],
        ] {
            assert_eq!(
                parse(fixture),
                rejected(ImportReason::MalformedJson, None, None),
                "for the fixture {fixture:?}"
            );
        }
    }

    /// **The truncation case, at every cut.** A file that lost its tail must be
    /// refused rather than half-read: acceptance criterion 7 has the user wipe
    /// their store between the export and the import, so a partial read here is
    /// the difference between a backup and data loss.
    #[test]
    fn every_truncation_of_a_valid_file_is_rejected_whole() {
        let clips: Vec<Clip> = (0..5)
            .map(|n| clip(&format!("clip {n}"), "a value", Colour::Teal))
            .collect();
        let whole = rendered(&clips);

        // Up to the last byte that is not trailing whitespace. Cutting the
        // trailing newline alone leaves a complete document, and asserting that
        // it fails would be asserting that JSON is whitespace-sensitive.
        for cut in 0..significant_length(&whole) {
            match parse(&whole[..cut]) {
                Ok(parsed) => panic!(
                    "a file truncated to {cut} bytes parsed {} clips",
                    parsed.len()
                ),
                Err(ClipError::Import { .. }) => {}
                Err(e) => panic!("a truncation should be an import error, not {e}"),
            }
        }
        // And the whole file is still the whole file.
        assert_eq!(parse(&whole).map(|clips| clips.len()), Ok(5));
    }

    #[test]
    fn a_file_with_the_wrong_format_marker_is_malformed_json() {
        for marker in [
            json!("fastclip"),
            json!(""),
            json!(1),
            json!(null),
            json!([]),
        ] {
            let mut body = file(json!([one_clip()]));
            if let Some(object) = body.as_object_mut() {
                object.insert("format".into(), marker.clone());
            }
            assert_eq!(
                imported(body),
                rejected(ImportReason::MalformedJson, Some("format"), None),
                "for the marker {marker}"
            );
        }
    }

    #[test]
    fn a_file_with_no_format_marker_names_the_missing_field() {
        let body = json!({ "version": 1, "clips": [] });
        assert_eq!(
            imported(body),
            rejected(ImportReason::MissingField, Some("format"), None)
        );
    }

    /// **Above the boundary.** Only a higher version can have been written by a
    /// FastClip, so it is the only one that gets `unsupported_version` and its
    /// "from a newer version of FastClip" sentence.
    #[test]
    fn a_newer_version_is_unsupported_rather_than_read_anyway() {
        for version in [EXPORT_VERSION + 1, 3, 99, i64::MAX] {
            let body = json!({ "format": FORMAT, "version": version, "clips": [one_clip()] });
            assert_eq!(
                imported(body),
                rejected(ImportReason::UnsupportedVersion, Some("version"), None),
                "for version {version}"
            );
        }
    }

    /// **Below the boundary.** Contract §1 `ExportFile`, *A version below 1 is
    /// not a version problem*: no build ever wrote `0` or a negative version, so
    /// the file is not an export from another version of this application and
    /// `unsupported_version` would send the user after an upgrade that does not
    /// exist.
    #[test]
    fn a_version_below_one_is_malformed_json_rather_than_an_unsupported_version() {
        for version in [EXPORT_VERSION - 1, -1, i64::MIN] {
            let body = json!({ "format": FORMAT, "version": version, "clips": [one_clip()] });
            assert_eq!(
                imported(body),
                rejected(ImportReason::MalformedJson, Some("version"), None),
                "for version {version}"
            );
        }
    }

    /// The two sides of the boundary answer differently, and one accepted
    /// version sits between them. A change that collapsed the pair into one
    /// answer would fail here as well as in the two tests above.
    #[test]
    fn the_version_boundary_answers_differently_on_each_side() {
        let judge =
            |version: i64| imported(json!({ "format": FORMAT, "version": version, "clips": [] }));
        assert_eq!(
            judge(EXPORT_VERSION - 1),
            rejected(ImportReason::MalformedJson, Some("version"), None)
        );
        assert_eq!(judge(EXPORT_VERSION), Ok(vec![]));
        assert_eq!(
            judge(EXPORT_VERSION + 1),
            rejected(ImportReason::UnsupportedVersion, Some("version"), None)
        );
    }

    #[test]
    fn a_version_that_is_not_an_integer_is_an_invalid_value() {
        for version in [json!(1.5), json!("1"), json!(null), json!(true), json!([1])] {
            let body = json!({ "format": FORMAT, "version": version, "clips": [] });
            assert_eq!(
                imported(body),
                rejected(ImportReason::InvalidValue, Some("version"), None),
                "for version {version}"
            );
        }
    }

    #[test]
    fn a_file_with_no_version_names_the_missing_field() {
        let body = json!({ "format": FORMAT, "clips": [] });
        assert_eq!(
            imported(body),
            rejected(ImportReason::MissingField, Some("version"), None)
        );
    }

    /// **The version is judged before the unknown-field sweep.** A file from a
    /// later build carrying a field this one has never seen must be reported as
    /// too new, not as a typo the user should hunt for.
    #[test]
    fn a_newer_file_reports_its_version_rather_than_its_unknown_fields() {
        let body = json!({
            "format": FORMAT,
            "version": 2,
            "clips": [],
            "exported_at": "2026-07-30T00:00:00Z",
        });
        assert_eq!(
            imported(body),
            rejected(ImportReason::UnsupportedVersion, Some("version"), None)
        );
    }

    #[test]
    fn an_unknown_top_level_field_is_named_with_no_index() {
        let body = json!({
            "format": FORMAT, "version": 1, "clips": [], "exported_at": "yesterday",
        });
        assert_eq!(
            imported(body),
            rejected(ImportReason::UnknownField, Some("exported_at"), None)
        );
    }

    #[test]
    fn a_missing_or_wrongly_typed_clips_array_is_reported_as_itself() {
        assert_eq!(
            imported(json!({ "format": FORMAT, "version": 1 })),
            rejected(ImportReason::MissingField, Some("clips"), None)
        );
        for clips in [json!({}), json!("none"), json!(null), json!(3)] {
            assert_eq!(
                imported(file(clips.clone())),
                rejected(ImportReason::InvalidValue, Some("clips"), None),
                "for clips {clips}"
            );
        }
    }

    // ---- one bad clip rejects the file ----

    #[test]
    fn a_clip_that_is_not_an_object_names_its_index() {
        let body = file(json!([one_clip(), "not a clip", one_clip()]));
        assert_eq!(
            imported(body),
            rejected(ImportReason::InvalidValue, None, Some(1))
        );
    }

    #[test]
    fn a_missing_clip_field_names_the_field_and_the_index() {
        for field in ["label", "value", "colour"] {
            let mut clip = one_clip();
            if let Some(object) = clip.as_object_mut() {
                object.remove(field);
            }
            let body = file(json!([one_clip(), one_clip(), clip]));
            assert_eq!(
                imported(body),
                rejected(ImportReason::MissingField, Some(field), Some(2)),
                "for the missing field {field}"
            );
        }
    }

    #[test]
    fn a_wrongly_typed_clip_field_is_an_invalid_value_at_its_index() {
        for field in ["label", "value", "colour"] {
            for wrong in [json!(42), json!(null), json!([]), json!({}), json!(true)] {
                let mut clip = one_clip();
                if let Some(object) = clip.as_object_mut() {
                    object.insert(field.into(), wrong.clone());
                }
                let body = file(json!([clip]));
                assert_eq!(
                    imported(body),
                    rejected(ImportReason::InvalidValue, Some(field), Some(0)),
                    "for {field} = {wrong}"
                );
            }
        }
    }

    /// Resolved at review 005: an unknown token is `invalid_value`, and
    /// `ImportReason` has no `not_a_palette_token`.
    #[test]
    fn an_unknown_colour_token_is_an_invalid_value_naming_the_colour() {
        for token in [
            "chartreuse",
            "unset",
            "",
            "Red",
            "#ce2a38",
            "rgba(47, 119, 150, 0.7)",
        ] {
            let mut clip = one_clip();
            if let Some(object) = clip.as_object_mut() {
                object.insert("colour".into(), json!(token));
            }
            let body = file(json!([one_clip(), clip]));
            assert_eq!(
                imported(body),
                rejected(ImportReason::InvalidValue, Some("colour"), Some(1)),
                "for the token {token:?}"
            );
        }
    }

    #[test]
    fn a_label_or_value_outside_the_boundarys_limits_is_an_invalid_value() {
        let cases: Vec<(&str, Value)> = vec![
            ("label", json!("")),
            ("label", json!("   ")),
            ("label", json!("a".repeat(LABEL_MAX_CHARS + 1))),
            ("label", json!("two\nlines")),
            ("label", json!("a\tb")),
            ("label", json!("a\u{0}b")),
            ("value", json!("")),
            ("value", json!(" \n ")),
            ("value", json!("a".repeat(VALUE_MAX_CHARS + 1))),
        ];
        for (field, bad) in cases {
            let mut clip = one_clip();
            if let Some(object) = clip.as_object_mut() {
                object.insert(field.into(), bad.clone());
            }
            assert_eq!(
                imported(file(json!([clip]))),
                rejected(ImportReason::InvalidValue, Some(field), Some(0)),
                "for {field} = {bad}"
            );
        }
    }

    /// Contract §1 `ExportFile`: `use_count` is never written and is rejected on
    /// import as an **unknown field** — `not_permitted` is an `InvalidReason`
    /// and has no counterpart in `ImportReason`. Importing another machine's
    /// counts would corrupt the ranking, so the file cannot carry one at all.
    #[test]
    fn a_use_count_in_an_import_file_is_an_unknown_field_at_its_index() {
        let mut clip = one_clip();
        if let Some(object) = clip.as_object_mut() {
            object.insert("use_count".into(), json!(7));
        }
        assert_eq!(
            imported(file(json!([one_clip(), clip]))),
            rejected(ImportReason::UnknownField, Some("use_count"), Some(1))
        );
    }

    /// The three fields the refactor deleted, plus a position that would
    /// contradict the array order.
    #[test]
    fn a_removed_field_on_a_clip_is_reported_by_name() {
        for field in ["icon", "visible", "clear_time", "position"] {
            let mut clip = one_clip();
            if let Some(object) = clip.as_object_mut() {
                object.insert(field.into(), json!("anything"));
            }
            assert_eq!(
                imported(file(json!([clip]))),
                rejected(ImportReason::UnknownField, Some(field), Some(0)),
                "{field} should be reported by name"
            );
        }
    }

    /// Contract §1 fixes which of several unknown keys is named: the
    /// lexicographically smallest, which holds only while `serde_json`'s
    /// `preserve_order` feature is off. A transitive dependency enabling it
    /// would change this without touching this file.
    #[test]
    fn the_unknown_field_named_is_the_lexicographically_smallest() {
        let mut clip = one_clip();
        if let Some(object) = clip.as_object_mut() {
            object.insert("use_count".into(), json!(1));
            object.insert("icon".into(), json!("x"));
        }
        assert_eq!(
            imported(file(json!([clip]))),
            rejected(ImportReason::UnknownField, Some("icon"), Some(0)),
            "icon sorts before use_count"
        );
    }

    /// **The first failure aborts, and nothing after it is examined.** The
    /// caller gets one error naming one clip, so the user is told where to look
    /// rather than handed a list.
    #[test]
    fn the_first_bad_clip_is_the_one_reported() {
        let mut third = one_clip();
        if let Some(object) = third.as_object_mut() {
            object.insert("colour".into(), json!("chartreuse"));
        }
        let mut fifth = one_clip();
        if let Some(object) = fifth.as_object_mut() {
            object.remove("label");
        }
        let body = file(json!([one_clip(), one_clip(), third, one_clip(), fifth,]));
        assert_eq!(
            imported(body),
            rejected(ImportReason::InvalidValue, Some("colour"), Some(2))
        );
    }

    // ---- nothing leaks ----

    /// A rejection names the field and the index, never the text. A clip in an
    /// import file is the user's secret whether or not the file was plaintext,
    /// and an error crosses the seam and reaches a log.
    #[test]
    fn no_rejection_carries_the_rejected_text() {
        let secret = "hunter2-hunter2";
        let cases: Vec<Value> = vec![
            file(json!([{ "label": secret, "value": "v", "colour": "chartreuse" }])),
            file(json!([{ "label": secret.repeat(20), "value": "v", "colour": "teal" }])),
            file(json!([{ "label": "l", "value": secret, "colour": secret }])),
            json!({ "format": secret, "version": 1, "clips": [] }),
            json!({ "format": FORMAT, "version": secret, "clips": [] }),
        ];
        for body in cases {
            match imported(body.clone()) {
                Ok(_) => panic!("this fixture should have been refused: {body}"),
                Err(error) => {
                    let rendered = format!("{error} {error:?}");
                    assert!(!rendered.contains(secret), "{rendered}");
                }
            }
        }
    }

    /// **The one rejection that does echo text from the file, by design.**
    /// Contract §1 `ExportFile` requires the offending field to be named, and
    /// spec §4.6 requires the message to say what was wrong — so an unknown key
    /// is reported verbatim. It is a *key* rather than a clip `label` or
    /// `value`, it is bounded by that, and nothing here writes it to a log: the
    /// name travels in the error and stops at the frontend's sentence.
    #[test]
    fn an_unknown_field_names_the_key_because_the_contract_requires_it() {
        assert_eq!(
            imported(file(
                json!([{ "label": "l", "value": "v", "colour": "teal", "icon": "x" }])
            )),
            rejected(ImportReason::UnknownField, Some("icon"), Some(0))
        );
    }

    // ---- the count the readback checks ----

    #[test]
    fn the_parsed_count_is_the_number_of_clips_in_the_file() {
        for n in [0usize, 1, 5, 40] {
            let clips: Vec<Clip> = (0..n)
                .map(|i| clip(&format!("clip {i}"), "a value", Colour::Teal))
                .collect();
            assert_eq!(parsed_count(&rendered(&clips)), Ok(n));
        }
    }

    #[test]
    fn the_parsed_count_of_a_truncated_file_is_an_error_rather_than_a_short_count() {
        let clips: Vec<Clip> = (0..4)
            .map(|i| clip(&format!("clip {i}"), "a value", Colour::Teal))
            .collect();
        let whole = rendered(&clips);
        let cut = whole.len() / 2;
        match parsed_count(&whole[..cut]) {
            Ok(count) => panic!("a truncated file reported {count} clips"),
            Err(ClipError::Import { .. }) => {}
            Err(e) => panic!("expected an import error, got {e}"),
        }
    }

    #[test]
    fn every_minted_id_is_distinct() {
        let ids: std::collections::HashSet<Uuid> = (0..64).map(|_| mint_id()).collect();
        assert_eq!(ids.len(), 64);
    }
}
