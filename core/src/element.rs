//! The element model (PRD req 3): a file is a set of addressable elements.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{Error, Result};

/// The universal category sitting on top of a raw tree-sitter kind.
///
/// v1 promotes exactly three categories. Config/data languages classify as
/// `block` under fs2's taxonomy and are deliberately **not indexed** (PRD
/// req 43), so `block` is not a variant here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    /// Functions, methods, constructors.
    Callable,
    /// Classes, structs, interfaces, enums, traits, impls.
    Type,
    /// Markdown heading sections (PRD req 22).
    Section,
}

impl ElementKind {
    /// The stable wire/storage spelling. Matches the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ElementKind::Callable => "callable",
            ElementKind::Type => "type",
            ElementKind::Section => "section",
        }
    }
}

impl fmt::Display for ElementKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A content address: the git blob SHA of the bytes an element came from
/// (PRD req 5), or a plain content hash for non-git folders (PRD req 23).
///
/// Validated on construction so a malformed key can never reach the store.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobRef(String);

impl BlobRef {
    /// Shortest abbreviation git will hand out; anything shorter is a typo.
    const MIN_LEN: usize = 7;
    /// A sha-256 hex digest — the longest hash fs3 keys on.
    const MAX_LEN: usize = 64;

    /// Build a blob reference from a lowercase hex digest.
    ///
    /// # Errors
    /// Returns [`Error::InvalidBlobRef`] when the value is not lowercase hex of
    /// a plausible digest length.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let reason = if value.len() < Self::MIN_LEN {
            Some("too short to be a content hash")
        } else if value.len() > Self::MAX_LEN {
            Some("longer than a sha-256 digest")
        } else if !value
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() && b.is_ascii_hexdigit())
        {
            Some("not lowercase hexadecimal")
        } else {
            None
        };
        match reason {
            Some(reason) => Err(Error::InvalidBlobRef { value, reason }),
            None => Ok(BlobRef(value)),
        }
    }

    /// The digest as stored.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for BlobRef {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        BlobRef::new(value)
    }
}

impl From<BlobRef> for String {
    fn from(value: BlobRef) -> Self {
        value.0
    }
}

/// One addressable unit of content: a callable, a type, or a markdown section.
///
/// The address is `(path, qualified_name, start_line..=end_line)` plus the
/// content key `blob`; `ts_kind` is kept verbatim so a raw grammar kind is
/// never lost behind the universal [`ElementKind`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    /// Repo-relative path of the file the element came from.
    pub path: String,
    /// Content key of the file's bytes.
    pub blob: BlobRef,
    /// The raw tree-sitter node kind, unmodified.
    pub ts_kind: String,
    /// The universal category derived from `ts_kind`.
    pub kind: ElementKind,
    /// Nested name, e.g. `Calculator.new` or `Main Title > Section One`.
    pub qualified_name: String,
    /// 1-based inclusive first line.
    pub start_line: u32,
    /// 1-based inclusive last line.
    pub end_line: u32,
    /// The element's own source text — what a [`crate::Summarizer`] reads.
    pub text: String,
    /// Whether the file parsed with an ERROR node.
    ///
    /// Metadata only. POC learning L8: error recovery routinely yields correct
    /// elements, so this must never be used to reject a file.
    pub has_error: bool,
}

impl Element {
    /// Number of lines the element spans, inclusive of both ends.
    pub const fn line_count(&self) -> u32 {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

/// Size-floor summarisation (PRD req 32).
///
/// Only elements at or above the configured line floor earn their own LLM
/// summary; smaller ones ride on their parent's summary while still getting
/// raw-content embeddings. Pure — the caller supplies the floor from config.
pub const fn needs_summary(element: &Element, min_lines: u32) -> bool {
    element.line_count() >= min_lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(start: u32, end: u32) -> Element {
        Element {
            path: "core/src/element.rs".into(),
            blob: BlobRef::new("0123456789abcdef").unwrap(),
            ts_kind: "function_item".into(),
            kind: ElementKind::Callable,
            qualified_name: "needs_summary".into(),
            start_line: start,
            end_line: end,
            text: String::new(),
            has_error: false,
        }
    }

    #[test]
    fn blob_ref_accepts_a_git_style_digest() {
        let blob = BlobRef::new("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391").unwrap();
        assert_eq!(blob.as_str(), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn blob_ref_rejects_non_hex_and_short_values() {
        assert!(matches!(
            BlobRef::new("not-a-hash!"),
            Err(Error::InvalidBlobRef { .. })
        ));
        assert!(matches!(
            BlobRef::new("abc"),
            Err(Error::InvalidBlobRef { .. })
        ));
        // Uppercase is a different string for the same bytes; refuse it so the
        // store never holds two keys for one blob.
        assert!(matches!(
            BlobRef::new("ABCDEF0123456789"),
            Err(Error::InvalidBlobRef { .. })
        ));
    }

    #[test]
    fn line_count_is_inclusive_of_both_ends() {
        assert_eq!(element(10, 10).line_count(), 1);
        assert_eq!(element(10, 12).line_count(), 3);
    }

    #[test]
    fn needs_summary_is_a_size_floor_not_a_kind_filter() {
        let floor = 10;
        assert!(
            !needs_summary(&element(1, 9), floor),
            "9 lines is below the floor"
        );
        assert!(
            needs_summary(&element(1, 10), floor),
            "the floor itself qualifies"
        );
        assert!(needs_summary(&element(100, 140), floor));
    }
}
