//! Files in, [`Element`]s out. No trait, no abstraction — tree-sitter direct
//! *is* the point (workshop 001 rule 3).
//!
//! The whole crate is one generic walk plus two tables. What makes it correct
//! rather than merely plausible is that classification is gated on declaration
//! shape in [`fs3_core::classify`] — the POC measured the ungated version
//! claiming 58 elements in a C++ file that has 22.
//!
//! Phase 1 carries exactly two grammars, Rust and Markdown, as the exemplar
//! pair: one code language and one document language. The grammar pack lands
//! with the real pipeline.

mod markdown;
mod source;

use fs3_core::{BlobRef, Element};

/// A grammar fs3 can parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    /// `tree-sitter-rust`.
    Rust,
    /// `tree-sitter-md`. Headings become sections (PRD req 22).
    Markdown,
}

impl Language {
    /// Map a file extension (without the dot, any case) to a grammar.
    ///
    /// Returns `None` for extensions fs3 has no grammar for. That `None` is the
    /// observable "unsupported" outcome PRD req 43 demands — a file must never
    /// be silently skipped.
    pub fn for_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "rs" => Some(Language::Rust),
            "md" | "markdown" => Some(Language::Markdown),
            _ => None,
        }
    }

    /// Map a path to a grammar via its extension.
    pub fn for_path(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.').map(|(_, ext)| ext)?;
        Self::for_extension(extension)
    }

    /// The name used in logs and skip reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Markdown => "markdown",
        }
    }
}

/// Why a file produced no elements.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    /// fs3 has no grammar for this file. An observable outcome, never a silent
    /// gap (PRD req 43).
    #[error("no grammar for {path:?} (extension {extension:?}); the file was skipped, not parsed")]
    NoGrammar {
        /// The path fs3 was asked to parse.
        path: String,
        /// The extension it could not match, or `None` when there is none.
        extension: Option<String>,
    },
    /// tree-sitter refused the grammar or ran out of budget.
    #[error("tree-sitter could not parse {path:?} as {language}")]
    Unparseable {
        /// The path fs3 was asked to parse.
        path: String,
        /// The grammar that was tried.
        language: &'static str,
    },
}

/// Parse a source file into elements, choosing the grammar from the path.
///
/// # Errors
/// [`ParseError::NoGrammar`] when the extension has no grammar;
/// [`ParseError::Unparseable`] when tree-sitter cannot produce a tree.
pub fn parse(path: &str, blob: &BlobRef, source: &str) -> Result<Vec<Element>, ParseError> {
    let language = Language::for_path(path).ok_or_else(|| ParseError::NoGrammar {
        path: path.to_string(),
        extension: path.rsplit_once('.').map(|(_, ext)| ext.to_string()),
    })?;
    parse_with(language, path, blob, source)
}

/// Parse with an explicitly chosen grammar.
///
/// # Errors
/// [`ParseError::Unparseable`] when tree-sitter cannot produce a tree.
pub fn parse_with(
    language: Language,
    path: &str,
    blob: &BlobRef,
    source: &str,
) -> Result<Vec<Element>, ParseError> {
    match language {
        Language::Rust => source::parse_rust(path, blob, source),
        Language::Markdown => markdown::parse_markdown(path, blob, source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_to_grammars() {
        assert_eq!(Language::for_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::for_extension("MD"), Some(Language::Markdown));
        assert_eq!(Language::for_path("a/b/c.rs"), Some(Language::Rust));
    }

    /// PRD req 43: an unsupported file is a reported outcome, not a silent gap.
    #[test]
    fn unsupported_extensions_are_observable() {
        let blob = BlobRef::new("0123456789abcdef").unwrap();
        let error = parse("infra/main.tf", &blob, "resource {}").unwrap_err();
        assert_eq!(
            error,
            ParseError::NoGrammar {
                path: "infra/main.tf".to_string(),
                extension: Some("tf".to_string()),
            }
        );
        assert!(error.to_string().contains("skipped, not parsed"));

        let error = parse("Makefile", &blob, "all:").unwrap_err();
        assert!(matches!(
            error,
            ParseError::NoGrammar {
                extension: None,
                ..
            }
        ));
    }
}
