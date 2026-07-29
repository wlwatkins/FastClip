//! One redaction, used everywhere text that could be a clip is formatted.
//!
//! ADR-0002 forbids a clip `value` reaching a log line at any level, and the
//! `label` is redacted on the same reasoning. A `Debug` derive on any type
//! holding either is the way that rule is broken by accident, so the types that
//! hold one write their own `Debug` and reach for this.
//!
//! It lives in its own module rather than beside the first type that needed it,
//! because two spellings of a redaction is one spelling too many: a second copy
//! can be relaxed without the first one changing.

use std::fmt;

/// A character count standing in for text that must never be printed.
pub(crate) struct Redacted(pub usize);

impl Redacted {
    /// Count the scalar values of `text` and forget it.
    ///
    /// Characters, not bytes — the same unit contract §0 counts lengths in.
    pub(crate) fn of(text: &str) -> Self {
        Self(text.chars().count())
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted, {} chars>", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_is_never_rendered_and_the_count_is_in_characters() {
        let rendered = format!("{:?}", Redacted::of("hunter2"));
        assert_eq!(rendered, "<redacted, 7 chars>");
        assert!(!rendered.contains("hunter2"));
    }

    #[test]
    fn a_character_outside_the_basic_multilingual_plane_counts_once() {
        // `s.len()` would say 4 and JavaScript's `s.length` would say 2.
        assert_eq!(
            format!("{:?}", Redacted::of("\u{1F600}")),
            "<redacted, 1 chars>"
        );
    }
}
