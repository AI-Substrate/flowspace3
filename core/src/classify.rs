//! Universal element classification — the same code for every language.
//!
//! Two stages, deliberately separate so the gate can be proven on its own:
//!
//! 1. [`category_hint`] maps a raw tree-sitter kind to a category by substring.
//! 2. [`is_declaration_shaped`] gates that hint on the kind genuinely being a
//!    declaration.
//!
//! Stage 2 exists because stage 1 alone **invents elements** — the POC measured
//! a C++ file yielding 58 claimed elements against 22 real ones once
//! `function_declarator` twins and elements named `void` were counted
//! (learning L2, PRD req 42). [`classify`] is the composition and is all a
//! caller needs.
//!
//! Adding a language never edits this file (PRD req 21): a new grammar brings
//! new `ts_kind` strings that the tables already cover, or it earns one more
//! table entry — never a language branch.

use crate::element::ElementKind;

/// Substrings that mark a callable. Checked first: "constructor" contains
/// "struct", so type matching must not win the race.
const CALLABLE_HINTS: &[&str] = &[
    "function",
    "method",
    "constructor",
    "subroutine",
    "procedure",
];

/// Substrings that mark a type-like declaration.
const TYPE_HINTS: &[&str] = &[
    "class",
    "struct",
    "interface",
    "enum",
    "trait",
    "type",
    "record",
    "protocol",
    "impl",
    "union",
    "module",
];

/// Substrings that mark a document section (PRD req 22).
const SECTION_HINTS: &[&str] = &["heading", "section"];

/// Suffixes shared by declaration nodes across grammars.
const DECL_SUFFIXES: &[&str] = &[
    "_item",
    "_declaration",
    "_definition",
    "_signature",
    "_spec",
];

/// `_specifier` is a declaration only for types (C/C++ `struct_specifier`);
/// for callables it is a parameter modifier.
const TYPE_ONLY_SUFFIXES: &[&str] = &["_specifier"];

/// Grammars that declare with bare kind names and no suffix at all.
///
/// Learning L5: without this table a 6.3 KB Ruby file yields **zero** elements,
/// because Ruby declares with `method` / `class` / `module`. Markdown's
/// headings are bare for the same reason.
const BARE_DECLS: &[&str] = &[
    "method",
    "class",
    "module",
    "function",
    "atx_heading",
    "setext_heading",
];

/// The ungated substring guess — stage 1, exposed so the gate's effect is
/// observable in tests and so callers can report *why* a node was rejected.
///
/// This is the classifier fs2 shipped, and on its own it is wrong.
pub fn category_hint(ts_kind: &str) -> Option<ElementKind> {
    if CALLABLE_HINTS.iter().any(|hint| ts_kind.contains(hint)) {
        return Some(ElementKind::Callable);
    }
    if TYPE_HINTS.iter().any(|hint| ts_kind.contains(hint)) {
        return Some(ElementKind::Type);
    }
    if SECTION_HINTS.iter().any(|hint| ts_kind.contains(hint)) {
        return Some(ElementKind::Section);
    }
    None
}

/// The declaration-shape gate — stage 2 (PRD req 42).
///
/// A node only becomes an element if its kind is declaration-shaped: a known
/// declaration suffix, or an exact bare-declaration word.
pub fn is_declaration_shaped(ts_kind: &str, hint: ElementKind) -> bool {
    if BARE_DECLS.contains(&ts_kind) {
        return true;
    }
    if DECL_SUFFIXES.iter().any(|suffix| ts_kind.ends_with(suffix)) {
        return true;
    }
    hint == ElementKind::Type && TYPE_ONLY_SUFFIXES.iter().any(|s| ts_kind.ends_with(s))
}

/// Map a raw tree-sitter kind to a universal category, or `None` when the node
/// is not an element.
///
/// ```
/// use fs3_core::{classify, ElementKind};
///
/// assert_eq!(classify("function_item"), Some(ElementKind::Callable));
/// assert_eq!(classify("struct_item"), Some(ElementKind::Type));
/// // `Self { .. }` is a literal, not a declaration:
/// assert_eq!(classify("struct_expression"), None);
/// ```
pub fn classify(ts_kind: &str) -> Option<ElementKind> {
    let hint = category_hint(ts_kind)?;
    if is_declaration_shaped(ts_kind, hint) {
        Some(hint)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_declarations_classify() {
        assert_eq!(classify("function_item"), Some(ElementKind::Callable));
        assert_eq!(
            classify("function_signature_item"),
            Some(ElementKind::Callable)
        );
        assert_eq!(classify("struct_item"), Some(ElementKind::Type));
        assert_eq!(classify("enum_item"), Some(ElementKind::Type));
        assert_eq!(classify("trait_item"), Some(ElementKind::Type));
        assert_eq!(classify("impl_item"), Some(ElementKind::Type));
        assert_eq!(classify("union_item"), Some(ElementKind::Type));
    }

    #[test]
    fn markdown_headings_are_sections() {
        assert_eq!(classify("atx_heading"), Some(ElementKind::Section));
        assert_eq!(classify("setext_heading"), Some(ElementKind::Section));
    }

    /// PRD req 42 / POC learning L2 — the exemplar this whole gate exists for.
    ///
    /// Each kind below is what fs2's substring classifier promoted to an
    /// element. Every one is a real node in a real grammar, and none of them is
    /// a declaration.
    #[test]
    fn declaration_gate_rejects_nodes_the_substring_guess_accepts() {
        let invented = [
            // Rust `Self { .. }` — a literal, classified `type` by substring.
            ("struct_expression", ElementKind::Type),
            // TypeScript — a duplicate anonymous type per interface.
            ("interface_body", ElementKind::Type),
            // C++ — twinned every `function_definition`, named after the
            // return type (elements literally called `void`).
            ("function_declarator", ElementKind::Callable),
            // TypeScript — every anonymous test callback promoted.
            ("arrow_function", ElementKind::Callable),
            // Ruby — every `foo.method_call` matched the `method` substring.
            ("method_call", ElementKind::Callable),
        ];

        for (ts_kind, expected_hint) in invented {
            assert_eq!(
                category_hint(ts_kind),
                Some(expected_hint),
                "{ts_kind}: the substring guess should still fire — that is the point"
            );
            assert!(
                !is_declaration_shaped(ts_kind, expected_hint),
                "{ts_kind}: is not declaration-shaped"
            );
            assert_eq!(
                classify(ts_kind),
                None,
                "{ts_kind}: must not become an element"
            );
        }
    }

    #[test]
    fn bare_kind_grammars_still_declare() {
        // Ruby: no suffix anywhere. Without the bare table these yield nothing.
        assert_eq!(classify("method"), Some(ElementKind::Callable));
        assert_eq!(classify("class"), Some(ElementKind::Type));
        assert_eq!(classify("module"), Some(ElementKind::Type));
    }

    #[test]
    fn specifier_is_a_declaration_only_for_types() {
        assert!(is_declaration_shaped("struct_specifier", ElementKind::Type));
        assert!(!is_declaration_shaped(
            "storage_class_specifier",
            ElementKind::Callable
        ));
    }

    #[test]
    fn containers_and_bindings_are_not_elements() {
        // `mod_item` contributes a qualified-name segment (parsers' job); it is
        // not an element itself. Consts and statics are deliberately excluded.
        assert_eq!(classify("mod_item"), None);
        assert_eq!(classify("const_item"), None);
        assert_eq!(classify("static_item"), None);
        assert_eq!(classify("identifier"), None);
        assert_eq!(classify("type_identifier"), None);
    }

    #[test]
    fn constructor_is_callable_not_type() {
        // "constructor" contains "struct"; hint order is load-bearing.
        assert_eq!(
            classify("constructor_declaration"),
            Some(ElementKind::Callable)
        );
    }
}
