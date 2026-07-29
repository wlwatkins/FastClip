//! `Colour` — the closed set of palette tokens.
//!
//! The contract (§1 `Colour`, closed question 13) makes this an enum rather than
//! a validated `String`, so an unrenderable token cannot reach storage: the
//! check happens once, at the boundary, instead of at every call site.
//!
//! The variants are exactly the Token column of `docs/src/product/palette.md`,
//! ratified at WP-10. The provisional `"unset"` placeholder WP-04 shipped is not
//! among them and must never be reintroduced — a build carrying it renders a
//! colour whose appearance was never designed.
//!
//! **The store is unaffected.** `colour` remains a `TEXT` column
//! (`storage.md` § Schema): a token is stored by name, never as hex, so the
//! palette can be retuned without rewriting a single row. What this module adds
//! is the type at the seam, not a schema change.
//!
//! Nothing here can carry a secret. A token is one of nine fixed words, so the
//! `Debug` derive is safe — unlike [`crate::storage::ClipRow`], which hand-writes
//! its own.

use serde::Serialize;

use crate::error::{ClipError, InvalidReason};

/// A palette token.
///
/// Serialises to its `snake_case` wire name and is **not** `Deserialize` by
/// design. Contract §1 (*Argument deserialisation*) requires every argument
/// field to be lenient and resolved by hand, so that an unknown token becomes
/// `invalid_input { reason: "not_a_palette_token" }` rather than a serde enum
/// failure that reaches the frontend as a plain string. Deriving `Deserialize`
/// would put that mistake one `#[derive]` away. Use [`Colour::from_wire`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
    Red,
    Amber,
    Lime,
    Green,
    Teal,
    Blue,
    Violet,
    Pink,
    Slate,
}

/// Every token, in the order of the palette table.
///
/// The Rust counterpart of the frontend's `COLOUR_TOKENS`
/// (`src/lib/contract/types.ts`). One list, two consumers; a test in this module
/// reads that file and fails if the two ever disagree.
pub const COLOUR_TOKENS: [Colour; 9] = [
    Colour::Red,
    Colour::Amber,
    Colour::Lime,
    Colour::Green,
    Colour::Teal,
    Colour::Blue,
    Colour::Violet,
    Colour::Pink,
    Colour::Slate,
];

impl Colour {
    /// The token a clip gets when none was chosen (`palette.md` § Default).
    ///
    /// `slate` carries no category meaning, so a clip that was never assigned a
    /// colour does not resemble one that was set to `red` on purpose.
    ///
    /// **Not a fallback for an absent `colour`.** Contract §4 makes every field
    /// required and an absent one `invalid_input { reason: "required" }`; there
    /// is no default for a missing argument on that page. This constant is the
    /// value the *frontend's* picker starts at, and a backend seed for tests.
    /// Substituting it for a field the caller omitted would store a colour the
    /// user never chose and hide a frontend defect.
    pub const DEFAULT: Self = Self::Slate;

    /// The wire name. The inverse of [`Colour::parse_token`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Amber => "amber",
            Self::Lime => "lime",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Blue => "blue",
            Self::Violet => "violet",
            Self::Pink => "pink",
            Self::Slate => "slate",
        }
    }

    /// Resolve a token name, or `None` if it is not in the palette.
    ///
    /// Exact match only. No trimming and no case folding: contract §0 makes a
    /// UUID the one input parsed case-insensitively, and accepting `"Red"` here
    /// would put a second spelling of one token on the wire.
    pub fn parse_token(token: &str) -> Option<Self> {
        match token {
            "red" => Some(Self::Red),
            "amber" => Some(Self::Amber),
            "lime" => Some(Self::Lime),
            "green" => Some(Self::Green),
            "teal" => Some(Self::Teal),
            "blue" => Some(Self::Blue),
            "violet" => Some(Self::Violet),
            "pink" => Some(Self::Pink),
            "slate" => Some(Self::Slate),
            _ => None,
        }
    }

    /// Resolve a token arriving over IPC.
    ///
    /// An unrecognised token is
    /// `invalid_input { field: "colour", reason: "not_a_palette_token" }`
    /// (contract §1 `Colour`, §4 *Validation rules*). The field name is fixed
    /// because `colour` is the wire name everywhere it appears — inside a
    /// `ClipDraft`, a `Clip` and an exported clip alike.
    ///
    /// The rejected token is not logged. The column and the payload field are
    /// both free-form `TEXT` on arrival, and a caller that has bypassed its own
    /// generated types can put anything in one.
    pub fn from_wire(token: &str) -> Result<Self, ClipError> {
        match Self::parse_token(token) {
            Some(colour) => Ok(colour),
            None => Err(ClipError::InvalidInput {
                field: "colour".into(),
                reason: InvalidReason::NotAPaletteToken,
            }),
        }
    }

    /// Resolve a token read back out of the store.
    ///
    /// A stored value that is not a token is `storage`, which is the only
    /// variant `list_clips` declares for a read fault. No error variant was
    /// added for this: one modelling a condition that lasts a single work
    /// package would outlive it by the life of the product (contract §1
    /// `Colour`).
    ///
    /// The log line carries the remedy instead. A development store written by a
    /// WP-04..WP-09 build holds `colour = 'unset'` in every row, and the fix is
    /// to delete the whole `~/.fast-clip/` directory — not `clips.db` alone,
    /// which would leave a stale `clips.db-wal` beside a fresh database.
    ///
    /// The offending value is not logged, on the same reasoning as
    /// [`crate::storage::clips`]'s id parser: a damaged `TEXT` column can hold a
    /// fragment of anything, including a clip `value`, and no clip value reaches
    /// a log line at any level (ADR-0002).
    pub fn from_stored(token: &str) -> Result<Self, ClipError> {
        match Self::parse_token(token) {
            Some(colour) => Ok(colour),
            None => {
                log::error!(
                    "a stored clip carries a colour that is not a palette token. \
                     A development store written before WP-10 holds 'unset' in every \
                     row; delete the whole ~/.fast-clip/ directory and restart."
                );
                Err(ClipError::Storage)
            }
        }
    }
}

impl Default for Colour {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl std::fmt::Display for Colour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl rusqlite::ToSql for Colour {
    /// Bind a token as the `TEXT` the column holds. Having the enum bind itself
    /// is what stops a call site reaching for a `&str` and writing something the
    /// palette does not contain.
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(
            rusqlite::types::ValueRef::Text(self.as_str().as_bytes()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nine tokens of `docs/src/product/palette.md`, written out here rather
    /// than derived from the enum, so that a variant added, removed or renamed
    /// in `Colour` fails against the ratified table instead of redefining it.
    const RATIFIED: [&str; 9] = [
        "red", "amber", "lime", "green", "teal", "blue", "violet", "pink", "slate",
    ];

    #[test]
    fn the_token_set_is_exactly_the_ratified_palette() {
        let ours: Vec<&str> = COLOUR_TOKENS.iter().map(|c| c.as_str()).collect();
        assert_eq!(ours, RATIFIED.to_vec());
    }

    /// WP-10's definition of done. `"unset"` was a placeholder for the interval
    /// in which the palette did not exist, and a release carrying it is a
    /// `BLOCK`-level finding (contract §1 `Colour`).
    #[test]
    fn unset_is_not_a_token_and_is_rejected_everywhere() {
        assert!(!RATIFIED.contains(&"unset"));
        assert!(!COLOUR_TOKENS.iter().any(|c| c.as_str() == "unset"));
        assert_eq!(Colour::parse_token("unset"), None);
        assert_eq!(
            Colour::from_wire("unset"),
            Err(ClipError::InvalidInput {
                field: "colour".into(),
                reason: InvalidReason::NotAPaletteToken,
            })
        );
        assert_eq!(Colour::from_stored("unset"), Err(ClipError::Storage));
    }

    #[test]
    fn every_token_round_trips_through_its_wire_name() {
        for colour in COLOUR_TOKENS {
            assert_eq!(Colour::parse_token(colour.as_str()), Some(colour));
            assert_eq!(Colour::from_wire(colour.as_str()), Ok(colour));
            assert_eq!(Colour::from_stored(colour.as_str()), Ok(colour));
        }
    }

    #[test]
    fn every_token_serialises_to_its_snake_case_wire_name() {
        for colour in COLOUR_TOKENS {
            let expected = format!("\"{}\"", colour.as_str());
            match serde_json::to_string(&colour) {
                Ok(json) => assert_eq!(json, expected),
                Err(e) => panic!("{colour} failed to serialise: {e}"),
            }
        }
    }

    #[test]
    fn no_token_is_declared_twice() {
        let mut seen: Vec<&str> = COLOUR_TOKENS.iter().map(|c| c.as_str()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn the_default_token_is_slate() {
        assert_eq!(Colour::DEFAULT, Colour::Slate);
        assert_eq!(Colour::default(), Colour::Slate);
        assert_eq!(Colour::default().as_str(), "slate");
    }

    /// Everything that is not a token is refused, including the shapes the
    /// pre-refactor build actually stored (contract §7): free-form rgba strings
    /// from a Mantine picker, and the unreachable `"red"`-style default.
    #[test]
    fn anything_that_is_not_a_token_is_not_a_palette_token() {
        let rejected = [
            "",
            " ",
            "unset",
            "chartreuse",
            "Red",
            "RED",
            " red",
            "red ",
            "red\n",
            "#ce2a38",
            "rgba(47, 119, 150, 0.7)",
            "clip-red",
            "grey",
            "gray",
            "orange",
            "purple",
            "null",
        ];
        for token in rejected {
            assert_eq!(Colour::parse_token(token), None, "{token:?} was accepted");
            assert_eq!(
                Colour::from_wire(token),
                Err(ClipError::InvalidInput {
                    field: "colour".into(),
                    reason: InvalidReason::NotAPaletteToken,
                }),
                "{token:?} produced the wrong error"
            );
            assert_eq!(
                Colour::from_stored(token),
                Err(ClipError::Storage),
                "{token:?} produced the wrong error"
            );
        }
    }

    /// A rejected token must never be echoed back. `field` names the field, not
    /// the value, so a caller cannot use the error to reflect its own input.
    #[test]
    fn the_rejection_never_carries_the_rejected_value() {
        let token = "rgba(47, 119, 150, 0.7)";
        match Colour::from_wire(token) {
            Err(error) => {
                let rendered = format!("{error} {error:?}");
                assert!(!rendered.contains(token), "{rendered}");
            }
            Ok(colour) => panic!("{token:?} should not resolve, but gave {colour}"),
        }
    }

    /// One list, two consumers (contract §1 `Colour`). The frontend derives its
    /// `Colour` union and its boundary validator from `COLOUR_TOKENS`; this enum
    /// is the same fact on the other side of the seam. Drift between them is
    /// silent — the type would accept a token the other rejects — so it is
    /// checked rather than trusted.
    #[test]
    fn the_rust_tokens_match_the_frontend_declaration() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src/lib/contract/types.ts");
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(e) => panic!("{} could not be read: {e}", path.display()),
        };

        let after = match source.split_once("export const COLOUR_TOKENS = [") {
            Some((_, after)) => after,
            None => panic!("COLOUR_TOKENS is not declared in {}", path.display()),
        };
        let body = match after.split_once(']') {
            Some((body, _)) => body,
            None => panic!("the COLOUR_TOKENS array is unterminated"),
        };

        let frontend: Vec<&str> = body
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| entry.trim_matches('"'))
            .collect();

        let ours: Vec<&str> = COLOUR_TOKENS.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            frontend,
            ours,
            "the Rust palette and {} disagree",
            path.display()
        );
    }
}
