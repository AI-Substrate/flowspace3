//! The element model (PRD req 3): a file is a *tree* of addressable elements.
//!
//! One node type, self-parented. A scan returns an owned [`ElementTree`] whose
//! root is the file itself; everything the file declares hangs beneath it in
//! source order. Parent ids, foreign keys and adjacency tables are the store's
//! concern — this model is a value, not a row set.
//!
//! Three properties carry the weight:
//!
//! * [`Element::address`] is stable across re-parses, so an element keeps its
//!   identity when the lines around it move.
//! * [`Element::raw_hash`] is the dirtiness key: same bytes, same hash, and a
//!   hash that changed is the only reason to re-embed or re-summarise.
//! * File-level facts (path, content key, parse health) live once on the
//!   [`ElementTree`], not duplicated onto every node.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

use crate::error::{Error, Result};

/// Separator between address segments: `src/foo.rs::Indexer::scan`.
pub const ADDRESS_SEGMENT: &str = "::";

/// The universal category sitting on top of a raw grammar kind.
///
/// Deliberately CLOSED and deliberately small. Language-specific detail —
/// `impl_item`, `class_definition`, `atx_heading` — is [`Element::subkind`], a
/// free string, so a new grammar never grows this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    /// The file itself: every tree has exactly one, at the root.
    File,
    /// Something that holds other declarations: class, struct, impl, trait,
    /// enum, module.
    Container,
    /// Something callable: function, method, constructor.
    Function,
    /// A document section — a markdown heading and its body (PRD req 22).
    Section,
    /// One turn of a conversation (workshop 005).
    ///
    /// The fourth content type, and it earns a place in a CLOSED enum for the
    /// same reason the other three do: it is a retrieval unit. Search, the
    /// spend guard and GC all root at `elements`, so a turn that is not an
    /// element is a turn nothing can find, enrich or collect.
    Turn,
}

impl ElementKind {
    /// The stable wire/storage spelling. Matches the serde representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ElementKind::File => "file",
            ElementKind::Container => "container",
            ElementKind::Function => "function",
            ElementKind::Section => "section",
            ElementKind::Turn => "turn",
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

/// The sha-256 of some bytes, lowercase hex.
///
/// The one hash function in fs3: it keys element dirtiness and, for a folder
/// with no git to ask, a whole file's content (PRD req 23).
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // `{:02x}` through `write!` would need a `fmt::Write` import and can
        // fail; a two-character table lookup cannot.
        const HEX: &[u8; 16] = b"0123456789abcdef";
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hex
}

/// An inclusive, 1-based line range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// 1-based inclusive first line.
    pub start_line: u32,
    /// 1-based inclusive last line.
    pub end_line: u32,
}

impl Span {
    /// A span over `start_line..=end_line`.
    pub const fn new(start_line: u32, end_line: u32) -> Self {
        Span {
            start_line,
            end_line,
        }
    }

    /// Number of lines the span covers, inclusive of both ends.
    pub const fn line_count(self) -> u32 {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start_line, self.end_line)
    }
}

/// One addressable unit of content, plus everything declared inside it.
///
/// Build with [`Element::new`]: `raw_hash` is derived from `raw_text` there and
/// nowhere else, which is what makes "the hash changed" mean "the text changed".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    /// The universal category.
    pub kind: ElementKind,
    /// The grammar's own kind, verbatim (`impl_item`, `class_definition`), or
    /// for a [`ElementKind::File`] root, the language it was read as.
    pub subkind: String,
    /// The declaration's own name — `scan`, not `Indexer::scan`.
    pub name: String,
    /// Stable identity: `src/foo.rs::Indexer::scan`.
    ///
    /// Deliberately independent of line numbers, so an element keeps its
    /// address when code above it moves.
    pub address: String,
    /// The element's line range within its file.
    pub span: Span,
    /// The element's exact source slice — what a [`crate::Summarizer`] reads.
    pub raw_text: String,
    /// sha-256 of [`Element::raw_text`]. Derived, never set by hand.
    raw_hash: String,
    /// Position among its siblings, 0-based, in source order.
    pub sibling_order: u32,
    /// Declarations nested directly inside this one, in source order.
    pub children: Vec<Element>,
}

impl Element {
    /// A childless element at sibling position 0, with its hash derived.
    pub fn new(
        kind: ElementKind,
        subkind: impl Into<String>,
        name: impl Into<String>,
        address: impl Into<String>,
        span: Span,
        raw_text: impl Into<String>,
    ) -> Self {
        let raw_text = raw_text.into();
        Element {
            kind,
            subkind: subkind.into(),
            name: name.into(),
            address: address.into(),
            span,
            raw_hash: content_hash(raw_text.as_bytes()),
            raw_text,
            sibling_order: 0,
            children: Vec::new(),
        }
    }

    /// Place this element at a sibling position.
    #[must_use]
    pub fn with_sibling_order(mut self, sibling_order: u32) -> Self {
        self.sibling_order = sibling_order;
        self
    }

    /// Attach children, which are assumed to already carry their own order.
    #[must_use]
    pub fn with_children(mut self, children: Vec<Element>) -> Self {
        self.children = children;
        self
    }

    /// sha-256 of the element's raw text — the dirtiness key.
    pub fn raw_hash(&self) -> &str {
        &self.raw_hash
    }

    /// Number of lines the element spans, inclusive of both ends.
    pub const fn line_count(&self) -> u32 {
        self.span.line_count()
    }

    /// This element and every descendant, in source order (pre-order).
    pub fn iter(&self) -> PreOrder<'_> {
        PreOrder { stack: vec![self] }
    }
}

impl<'a> IntoIterator for &'a Element {
    type Item = &'a Element;
    type IntoIter = PreOrder<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Depth-first, source-order walk over an element and its descendants.
pub struct PreOrder<'a> {
    stack: Vec<&'a Element>,
}

impl<'a> Iterator for PreOrder<'a> {
    type Item = &'a Element;

    fn next(&mut self) -> Option<Self::Item> {
        let element = self.stack.pop()?;
        // Reversed, so the first child is popped first and the walk reads in
        // source order.
        self.stack.extend(element.children.iter().rev());
        Some(element)
    }
}

/// One file's elements, owned, plus the facts that are true of the whole file.
///
/// `path`, `blob` and `has_error` live here rather than on every node: they are
/// one value per file, and copying them onto each of a few hundred elements is
/// allocation with no information in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementTree {
    /// Repo-relative path the file was scanned at.
    pub path: String,
    /// Content key of the file's bytes.
    pub blob: BlobRef,
    /// Whether the parse hit an ERROR node.
    ///
    /// Metadata only. POC learning L8: error recovery routinely yields correct
    /// elements, so this must never be used to reject a file.
    pub has_error: bool,
    /// The file element. Always [`ElementKind::File`].
    pub root: Element,
}

impl ElementTree {
    /// The language the file was read as — `rust`, `markdown`, `unknown`.
    ///
    /// PRD req 43 wants "no grammar" to be an observable outcome rather than a
    /// silent gap; this is where a caller sees it.
    pub fn language(&self) -> &str {
        &self.root.subkind
    }

    /// Every element including the root, in source order.
    pub fn iter(&self) -> PreOrder<'_> {
        self.root.iter()
    }

    /// Total element count, root included.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Always false — a tree always has its file element.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The element at `address`, if the tree holds one.
    pub fn find(&self, address: &str) -> Option<&Element> {
        self.iter().find(|element| element.address == address)
    }
}

impl<'a> IntoIterator for &'a ElementTree {
    type Item = &'a Element;
    type IntoIter = PreOrder<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
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
        Element::new(
            ElementKind::Function,
            "function_item",
            "needs_summary",
            "core/src/element.rs::needs_summary",
            Span::new(start, end),
            "",
        )
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

    /// The hash is what the whole dirtiness story rests on, so pin it against a
    /// published vector rather than against itself.
    #[test]
    fn content_hash_matches_the_known_sha256_of_abc() {
        assert_eq!(
            content_hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // And it is always a legal content key.
        assert!(BlobRef::new(content_hash(b"abc")).is_ok());
    }

    #[test]
    fn raw_hash_is_derived_from_raw_text() {
        let element = Element::new(
            ElementKind::Function,
            "function_item",
            "f",
            "a.rs::f",
            Span::new(1, 1),
            "fn f() {}",
        );
        assert_eq!(element.raw_hash(), content_hash(b"fn f() {}"));
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

    fn leaf(name: &str, order: u32) -> Element {
        Element::new(
            ElementKind::Function,
            "function_item",
            name,
            format!("a.rs::{name}"),
            Span::new(1, 1),
            "",
        )
        .with_sibling_order(order)
    }

    fn tree() -> ElementTree {
        let inner = Element::new(
            ElementKind::Container,
            "impl_item",
            "Rect",
            "a.rs::Rect",
            Span::new(1, 9),
            "",
        )
        .with_children(vec![leaf("new", 0), leaf("area", 1)]);

        ElementTree {
            path: "a.rs".to_string(),
            blob: BlobRef::new("0123456789abcdef").unwrap(),
            has_error: false,
            root: Element::new(
                ElementKind::File,
                "rust",
                "a.rs",
                "a.rs",
                Span::new(1, 9),
                "",
            )
            .with_children(vec![inner, leaf("free", 1)]),
        }
    }

    /// A depth-first walk that reordered siblings would silently scramble
    /// `sibling_order`, so assert the sequence, not the set.
    #[test]
    fn iteration_is_depth_first_in_source_order() {
        let tree = tree();
        let addresses: Vec<&str> = tree.iter().map(|e| e.address.as_str()).collect();
        assert_eq!(
            addresses,
            vec![
                "a.rs",
                "a.rs::Rect",
                "a.rs::new",
                "a.rs::area",
                "a.rs::free",
            ]
        );
    }

    #[test]
    fn find_reaches_a_nested_element_by_address() {
        let tree = tree();
        assert_eq!(tree.len(), 5);
        assert_eq!(
            tree.find("a.rs::area").map(|e| e.name.as_str()),
            Some("area")
        );
        assert!(tree.find("a.rs::nope").is_none());
        assert_eq!(tree.language(), "rust");
    }
}
