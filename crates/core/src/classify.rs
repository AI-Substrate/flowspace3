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
//! new kind strings that the tables already cover, or it earns one more table
//! entry — never a language branch.
//!
//! [`ElementKind::File`] is never returned here. A file element is synthesised
//! by the scanner as the tree's root; no grammar node classifies as one.

use crate::element::ElementKind;

/// Substrings that mark a callable. Checked first: "constructor" contains
/// "struct", so container matching must not win the race.
const CALLABLE_HINTS: &[&str] = &[
    "function",
    "method",
    "constructor",
    "subroutine",
    "procedure",
];

/// Substrings that mark a container — something other declarations live inside.
///
/// `mod` covers Rust's `mod_item` and every grammar that spells it `module` or
/// `module_declaration`. A module *is* an element in the tree model: it is the
/// parent of what it contains, and `mod` is exactly the kind of language detail
/// that belongs in `subkind` under a `container`.
const CONTAINER_HINTS: &[&str] = &[
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
    "mod",
    "namespace",
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

/// `_specifier` is a declaration only for containers (C/C++ `struct_specifier`);
/// for callables it is a parameter modifier.
const CONTAINER_ONLY_SUFFIXES: &[&str] = &["_specifier"];

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
    // TypeScript namespaces use this bare kind rather than a declaration suffix.
    "internal_module",
    "atx_heading",
    "setext_heading",
];

/// The ungated substring guess — stage 1, exposed so the gate's effect is
/// observable in tests and so callers can report *why* a node was rejected.
///
/// This is the classifier fs2 shipped, and on its own it is wrong.
pub fn category_hint(ts_kind: &str) -> Option<ElementKind> {
    if CALLABLE_HINTS.iter().any(|hint| ts_kind.contains(hint)) {
        return Some(ElementKind::Function);
    }
    if CONTAINER_HINTS.iter().any(|hint| ts_kind.contains(hint)) {
        return Some(ElementKind::Container);
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
    hint == ElementKind::Container && CONTAINER_ONLY_SUFFIXES.iter().any(|s| ts_kind.ends_with(s))
}

/// Map a raw tree-sitter kind to a universal category, or `None` when the node
/// is not an element.
///
/// ```
/// use fs3_core::{classify, ElementKind};
///
/// assert_eq!(classify("function_item"), Some(ElementKind::Function));
/// assert_eq!(classify("struct_item"), Some(ElementKind::Container));
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
        assert_eq!(classify("function_item"), Some(ElementKind::Function));
        assert_eq!(
            classify("function_signature_item"),
            Some(ElementKind::Function)
        );
        assert_eq!(classify("struct_item"), Some(ElementKind::Container));
        assert_eq!(classify("enum_item"), Some(ElementKind::Container));
        assert_eq!(classify("trait_item"), Some(ElementKind::Container));
        assert_eq!(classify("impl_item"), Some(ElementKind::Container));
        assert_eq!(classify("union_item"), Some(ElementKind::Container));
    }

    /// Python's two declaration kinds, and the wrapper that is not one.
    #[test]
    fn python_declarations_classify() {
        assert_eq!(classify("function_definition"), Some(ElementKind::Function));
        assert_eq!(classify("class_definition"), Some(ElementKind::Container));
        // A decorated def is a wrapper around the real declaration; promoting
        // it would twin every decorated function.
        assert_eq!(classify("decorated_definition"), None);
    }

    #[test]
    fn markdown_headings_are_sections() {
        assert_eq!(classify("atx_heading"), Some(ElementKind::Section));
        assert_eq!(classify("setext_heading"), Some(ElementKind::Section));
    }

    /// A module is a container element in the tree model — it parents what is
    /// inside it, and `mod` is `subkind` detail, not a new kind.
    #[test]
    fn modules_are_containers_not_invisible_scopes() {
        assert_eq!(classify("mod_item"), Some(ElementKind::Container));
        assert_eq!(classify("module_declaration"), Some(ElementKind::Container));
        assert_eq!(
            classify("namespace_declaration"),
            Some(ElementKind::Container)
        );
        // `mod` is a substring, so guard the near-miss it could catch: a Java /
        // C# `modifiers` node is not a declaration and must stay out.
        assert_eq!(classify("modifiers"), None);
    }

    #[test]
    fn typescript_declaration_decisions_are_explicit() {
        for callable in [
            "function_declaration",
            "method_definition",
            "method_signature",
            "abstract_method_signature",
            "function_signature",
        ] {
            assert_eq!(
                classify(callable),
                Some(ElementKind::Function),
                "{callable}"
            );
        }
        for container in [
            "class_declaration",
            "abstract_class_declaration",
            "interface_declaration",
            "enum_declaration",
            "type_alias_declaration",
            "internal_module",
        ] {
            assert_eq!(
                classify(container),
                Some(ElementKind::Container),
                "{container}"
            );
        }
        for wrapper_or_binding in [
            "export_statement",
            "lexical_declaration",
            "import_statement",
            "variable_declarator",
            "public_field_definition",
            "arrow_function",
            "function_expression",
            "generator_function",
        ] {
            assert_eq!(
                classify(wrapper_or_binding),
                None,
                "{wrapper_or_binding} is handled by splicing or value shape, not raw kind"
            );
        }
    }

    /// PRD req 42 / POC learning L2 — the exemplar this whole gate exists for.
    ///
    /// Each kind below is what fs2's substring classifier promoted to an
    /// element. Every one is a real node in a real grammar, and none of them is
    /// a declaration.
    #[test]
    fn declaration_gate_rejects_nodes_the_substring_guess_accepts() {
        let invented = [
            // Rust `Self { .. }` — a literal, classified container by substring.
            ("struct_expression", ElementKind::Container),
            // TypeScript — a duplicate anonymous type per interface.
            ("interface_body", ElementKind::Container),
            // C++ — twinned every `function_definition`, named after the
            // return type (elements literally called `void`).
            ("function_declarator", ElementKind::Function),
            // TypeScript — every anonymous test callback promoted.
            ("arrow_function", ElementKind::Function),
            // Ruby — every `foo.method_call` matched the `method` substring.
            ("method_call", ElementKind::Function),
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
        assert_eq!(classify("method"), Some(ElementKind::Function));
        assert_eq!(classify("class"), Some(ElementKind::Container));
        assert_eq!(classify("module"), Some(ElementKind::Container));
    }

    #[test]
    fn specifier_is_a_declaration_only_for_containers() {
        assert!(is_declaration_shaped(
            "struct_specifier",
            ElementKind::Container
        ));
        assert!(!is_declaration_shaped(
            "storage_class_specifier",
            ElementKind::Function
        ));
    }

    #[test]
    fn bindings_are_not_elements() {
        // Consts and statics are deliberately excluded.
        assert_eq!(classify("const_item"), None);
        assert_eq!(classify("static_item"), None);
        assert_eq!(classify("identifier"), None);
        assert_eq!(classify("type_identifier"), None);
    }

    #[test]
    fn constructor_is_callable_not_container() {
        // "constructor" contains "struct"; hint order is load-bearing.
        assert_eq!(
            classify("constructor_declaration"),
            Some(ElementKind::Function)
        );
    }
}
