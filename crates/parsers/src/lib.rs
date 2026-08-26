//! Bytes in, an [`ElementTree`] out. No trait, no abstraction — tree-sitter
//! direct *is* the point (workshop 001 rule 3).
//!
//! [`scan`] is a **pure function**: it is handed a path and the file's bytes and
//! it returns a value. It opens nothing, writes nothing, and knows nothing about
//! a database, a queue or a clock. That is enforced mechanically — the arch
//! check refuses tokio and sqlx in this crate — and it is what lets the scanner
//! be tested with a `&str` and reused from any caller, sync or async.
//!
//! Three grammars ship as the exemplar set: **Rust** and **Python** (code) and
//! **Markdown** (documents). Anything else still scans: a file fs3 has no
//! grammar for yields a one-element tree — the file itself, `language()` of
//! `unknown`. A missing grammar is an observable outcome (PRD req 43), never an
//! error and never a silent skip.

mod markdown;
mod source;

use std::path::Path;

use fs3_core::{BlobRef, Element, ElementKind, ElementTree, Span, content_hash};

/// A grammar fs3 can parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    /// `tree-sitter-rust`.
    Rust,
    /// `tree-sitter-python`.
    Python,
    /// `tree-sitter-md`. Headings become sections (PRD req 22).
    Markdown,
}

/// What [`ElementTree::language`] reports for a file no grammar covers.
pub const UNKNOWN_LANGUAGE: &str = "unknown";

/// What [`ElementTree::language`] reports for bytes that are not UTF-8.
pub const BINARY_LANGUAGE: &str = "binary";

impl Language {
    /// Map a file extension (without the dot, any case) to a grammar.
    ///
    /// Returns `None` for extensions fs3 has no grammar for — the "unsupported"
    /// outcome, which [`scan`] turns into a file-only tree rather than a
    /// failure.
    ///
    /// Adding a language is this one line plus a grammar crate; the extraction
    /// walk is generic (PRD req 21).
    pub fn for_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "rs" => Some(Language::Rust),
            "py" | "pyi" => Some(Language::Python),
            "md" | "markdown" => Some(Language::Markdown),
            _ => None,
        }
    }

    /// Map a path to a grammar via its extension.
    pub fn for_path(path: &Path) -> Option<Self> {
        Self::for_extension(path.extension()?.to_str()?)
    }

    /// The name used in logs, skip reports and the file element's `subkind`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::Markdown => "markdown",
        }
    }

    /// The tree-sitter grammar.
    fn grammar(self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::Markdown => tree_sitter_md::LANGUAGE.into(),
        }
    }
}

/// Why a scan produced nothing at all.
///
/// There is deliberately no "no grammar" variant: that outcome is a tree, not an
/// error. What is left is tree-sitter itself refusing to work, which the POC
/// (learning L10) never once observed across 11,452 files — so if it fires, it
/// is a real defect and should be loud.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScanError {
    /// tree-sitter refused the grammar or ran out of budget.
    #[error("tree-sitter could not parse {path:?} as {language}")]
    Unparseable {
        /// The path fs3 was asked to scan.
        path: String,
        /// The grammar that was tried.
        language: &'static str,
    },
}

/// Scan one file's bytes into its element tree.
///
/// Pure: `path` is used for addressing and grammar selection only — it is never
/// opened. The returned tree always has a [`ElementKind::File`] root, even when
/// the file is binary or has no grammar.
///
/// # Errors
/// [`ScanError::Unparseable`] when tree-sitter cannot produce a tree at all.
pub fn scan(path: &Path, bytes: &[u8]) -> Result<ElementTree, ScanError> {
    // PRD req 23: with no git to ask, a file's content key is the hash of its
    // bytes. Hashing the raw bytes (not the decoded text) keeps the key right
    // for files that are not text.
    let blob = BlobRef::new(content_hash(bytes))
        .expect("a sha-256 hex digest is always a valid content key");
    let display = path.to_string_lossy();

    let Ok(source) = std::str::from_utf8(bytes) else {
        // Not text: nothing to walk, but the file still has an address and a
        // content key, so it is still a tree.
        return Ok(file_only(&display, blob, BINARY_LANGUAGE, ""));
    };

    let Some(language) = Language::for_path(path) else {
        return Ok(file_only(&display, blob, UNKNOWN_LANGUAGE, source));
    };

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language.grammar())
        .map_err(|_| ScanError::Unparseable {
            path: display.to_string(),
            language: language.as_str(),
        })?;
    let tree = parser.parse(source, None).ok_or(ScanError::Unparseable {
        path: display.to_string(),
        language: language.as_str(),
    })?;

    let root_node = tree.root_node();
    // L8: `has_error` is metadata. Error recovery routinely yields correct
    // elements, so it must never be used to reject a file.
    let has_error = root_node.has_error();

    let children = match language {
        Language::Markdown => markdown::sections(root_node, source, &display),
        Language::Rust | Language::Python => source::declarations(root_node, source, &display),
    };

    Ok(ElementTree {
        blob,
        has_error,
        root: file_element(&display, language.as_str(), source).with_children(children),
        path: display.into_owned(),
    })
}

/// A tree with nothing but its file element.
fn file_only(path: &str, blob: BlobRef, language: &str, source: &str) -> ElementTree {
    ElementTree {
        path: path.to_string(),
        blob,
        has_error: false,
        root: file_element(path, language, source),
    }
}

/// The root every tree has: the file itself, addressed by its path.
fn file_element(path: &str, language: &str, source: &str) -> Element {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    Element::new(
        ElementKind::File,
        language,
        name,
        path,
        Span::new(1, line_count(source)),
        source,
    )
}

/// Lines in a file, counting an empty file as one line so every span is real.
fn line_count(source: &str) -> u32 {
    source.lines().count().max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_to_grammars() {
        assert_eq!(Language::for_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::for_extension("MD"), Some(Language::Markdown));
        assert_eq!(Language::for_extension("py"), Some(Language::Python));
        assert_eq!(
            Language::for_path(Path::new("a/b/c.rs")),
            Some(Language::Rust)
        );
        assert_eq!(Language::for_path(Path::new("Makefile")), None);
    }

    /// PRD req 43: an unsupported file is a reported outcome, not a silent gap
    /// — and, in the tree model, not an error either.
    #[test]
    fn a_file_with_no_grammar_still_scans_to_a_file_element() {
        let tree = scan(Path::new("infra/main.tf"), b"resource \"x\" {}\n").unwrap();

        assert_eq!(tree.language(), UNKNOWN_LANGUAGE);
        assert_eq!(tree.len(), 1, "the file itself, and nothing invented");
        assert_eq!(tree.root.kind, ElementKind::File);
        assert_eq!(tree.root.address, "infra/main.tf");
        assert_eq!(tree.root.name, "main.tf");
        assert_eq!(tree.root.raw_text, "resource \"x\" {}\n");

        // A file with no extension at all takes the same path.
        let tree = scan(Path::new("Makefile"), b"all:\n").unwrap();
        assert_eq!(tree.language(), UNKNOWN_LANGUAGE);
        assert_eq!(tree.root.name, "Makefile");
    }

    /// Bytes that are not text must not panic and must not be lossily decoded
    /// into an element body that never existed.
    #[test]
    fn binary_bytes_scan_to_a_file_element_keyed_on_the_real_bytes() {
        let bytes = [0xffu8, 0xfe, 0x00, 0x01];
        let tree = scan(Path::new("assets/logo.png"), &bytes).unwrap();

        assert_eq!(tree.language(), BINARY_LANGUAGE);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.root.raw_text, "");
        assert_eq!(
            tree.blob.as_str(),
            content_hash(&bytes),
            "the content key is the hash of the bytes, decodable or not"
        );
    }

    /// The file element's own text is the whole file, so its hash is the file's
    /// hash — which is what makes "this file changed" a single comparison.
    #[test]
    fn the_file_elements_hash_is_the_content_key_for_text() {
        let source = "fn main() {}\n";
        let tree = scan(Path::new("src/main.rs"), source.as_bytes()).unwrap();
        assert_eq!(tree.root.raw_hash(), tree.blob.as_str());
    }

    #[test]
    fn an_empty_file_still_has_a_one_line_span() {
        let tree = scan(Path::new("src/empty.rs"), b"").unwrap();
        assert_eq!(tree.root.span, Span::new(1, 1));
        assert_eq!(tree.len(), 1);
    }
}
