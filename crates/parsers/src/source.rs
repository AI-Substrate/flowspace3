//! The generic code walk. One function, no language branches.
//!
//! The shape is: descend the syntax tree; a node that classifies *and* has a
//! name becomes an element and its subtree becomes that element's children; a
//! node that does neither is spliced through, so the declarations inside a
//! wrapper node (a `decorated_definition`, an error-recovery stub, a bare
//! block) are never lost — they simply attach to the nearest real ancestor.
//!
//! An element's text starts at its documentation, not at its keyword. The doc
//! comments, plain comments and attributes directly above a declaration (see
//! [`leading_start`]) are taken into its span and `raw_text`, because that text
//! is what both its raw vector and its summary are built from: a declaration
//! whose own documentation sits outside its node is otherwise documented
//! nowhere in the index (DL-042). Addresses do not move; only `span.start_line`
//! now names the first line of that documentation.

use fs3_core::{ADDRESS_SEGMENT, Element, ElementKind, Span, classify};
use tree_sitter::Node;

/// Fields that can carry a node's name, in priority order.
///
/// The order is load-bearing (POC learning L3). `type` must come **last**: it
/// is the target type for a Rust `impl_item`, but the *return* type in C and
/// C++, where trying it early produces elements literally named `void` and
/// `bool`.
const NAME_FIELDS: &[&str] = &["name", "declarator", "path", "pattern", "type"];

/// Binding nodes whose `value` field may declare a callable.
const FUNCTION_BINDINGS: &[&str] = &["variable_declarator", "public_field_definition"];

/// Function-shaped values promoted under their binding's name.
const FUNCTION_VALUES: &[&str] = &[
    "arrow_function",
    "function_expression",
    "generator_function",
];

/// Nodes that wrap a declaration without being one, so the declaration's
/// documentation sits above the wrapper: `export function`, a Python
/// `decorated_definition`, `const handler = () => {}`, TypeScript `declare`.
const WRAPPERS: &[&str] = &[
    "export_statement",
    "decorated_definition",
    "lexical_declaration",
    "variable_declaration",
    "ambient_declaration",
];

/// Annotations that belong to the declaration below them.
const ATTRIBUTES: &[&str] = &["attribute_item", "decorator"];

/// Every declaration inside `root`, as a source-ordered forest.
pub(crate) fn declarations(root: Node<'_>, source: &str, path: &str) -> Vec<Element> {
    let mut out = Vec::new();
    collect(root, source, path, &mut out);
    ordered(out)
}

/// Walk a node's children, promoting the ones that are declarations.
fn collect(node: Node<'_>, source: &str, scope: &str, out: &mut Vec<Element>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match element_at(child, source, scope) {
            Some(element) => out.push(element),
            // Not a declaration itself — but its subtree may hold several, and
            // they belong to this level. PRD req 42: skipping a nameless node
            // must not cost us the named declarations inside it, nor leave a
            // gap in their addresses.
            None => collect(child, source, scope, out),
        }
    }
}

/// Turn one node into an element, children and all, if it is a declaration.
fn element_at(node: Node<'_>, source: &str, scope: &str) -> Option<Element> {
    let ts_kind = node.kind();
    let kind = classify(ts_kind).or_else(|| function_binding_kind(node))?;
    // PRD req 42 wants genuine *named* declarations. A classified node with no
    // name is not one — it is error-recovery debris, or a suffix match on an
    // anonymous construct. Emitting it as `<anonymous>` invented an element
    // nobody can address.
    let name = node_name(node, source)?;

    // A receiver (Go's `func (c Calculator) Add`) is real parenting the syntax
    // tree cannot express: the method is not nested inside the type. It scopes
    // the address (L4b) without becoming part of the declaration's own name.
    let receiver = node
        .child_by_field_name("receiver")
        .and_then(|receiver| first_identifier_text(receiver, source));
    let address = match &receiver {
        Some(receiver) => format!("{scope}{ADDRESS_SEGMENT}{receiver}{ADDRESS_SEGMENT}{name}"),
        None => format!("{scope}{ADDRESS_SEGMENT}{name}"),
    };

    let mut children = Vec::new();
    collect(node, source, &address, &mut children);

    let start = leading_start(node, source);
    Some(
        Element::new(
            kind,
            ts_kind,
            name,
            &address,
            Span::new(
                start.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
            ),
            source
                .get(start.start_byte()..node.end_byte())
                .unwrap_or_default(),
        )
        .with_children(ordered(children)),
    )
}

/// The first node of a declaration's text: its leading documentation, or the
/// declaration itself when it has none.
///
/// Two moves, both generic. First climb out through [`WRAPPERS`] the node opens
/// (an `export` keyword or a decorator list belongs to the declaration). Then
/// take each sibling directly above — doc comments, plain comments and
/// [`ATTRIBUTES`] — while each touches the line below it. The run stops at:
///
/// * a blank line, so a comment set apart from the code is not claimed;
/// * an inner doc comment or shebang (`//!`, `/*!`, `#!`), which documents the
///   enclosing module or file, never the item after it;
/// * a plain comment that ends like code (`;`, `{`, `}`): commented-out code
///   is not documentation (doc comments are exempt — their examples are code);
/// * a comment that trails code on its own line.
///
/// A plain-comment run that starts on the file's first line is the file header
/// (a licence, a module note), so it is left to the file element; a doc comment
/// or attribute below it still attaches.
fn leading_start<'tree>(node: Node<'tree>, source: &str) -> Node<'tree> {
    let mut anchor = node;
    while let Some(parent) = anchor.parent()
        && WRAPPERS.contains(&parent.kind())
        && opens(parent, anchor)
    {
        anchor = parent;
    }

    let mut run = Vec::new();
    let mut below = anchor;
    while let Some(above) = node_above(below) {
        if last_row(above) + 1 < below.start_position().row {
            break;
        }
        let taken = if ATTRIBUTES.contains(&above.kind()) {
            Leading::Attribute
        } else if above.kind().contains("comment") && !trails_code(above) {
            leading_comment(node_text(above, source).trim())
        } else {
            Leading::Stop
        };
        if taken == Leading::Stop {
            break;
        }
        run.push((above, taken));
        below = above;
    }

    // `run` is bottom-up; a header is a plain-comment prefix at the very top.
    if run
        .last()
        .is_some_and(|(top, _)| top.start_position().row == 0 && top.prev_sibling().is_none())
    {
        while run
            .last()
            .is_some_and(|(_, taken)| *taken == Leading::Plain)
        {
            run.pop();
        }
    }
    run.last().map_or(anchor, |(top, _)| *top)
}

/// What a node above a declaration contributes to it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leading {
    /// `///`, `/** */`: documentation by the language's own rule.
    Doc,
    /// `//`, `/* */`, `#`: documentation by placement.
    Plain,
    /// `#[derive]`, `@decorator`.
    Attribute,
    /// Not the declaration's: ends the run.
    Stop,
}

/// Classify a comment by its text, which every grammar here spells the same.
fn leading_comment(text: &str) -> Leading {
    if ["//!", "/*!", "#!"]
        .iter()
        .any(|inner| text.starts_with(inner))
    {
        Leading::Stop
    } else if (text.starts_with("///") && !text.starts_with("////"))
        || (text.starts_with("/**") && text != "/**/")
    {
        Leading::Doc
    } else if text
        .trim_end_matches("*/")
        .trim_end()
        .ends_with([';', '{', '}'])
    {
        Leading::Stop
    } else {
        Leading::Plain
    }
}

/// `wrapper` begins with `node`'s declaration: the first of its kind inside, so
/// `const a = 1, b = () => {}` gives the comment above `const` to `a` alone.
fn opens(wrapper: Node<'_>, node: Node<'_>) -> bool {
    let mut cursor = wrapper.walk();
    wrapper
        .named_children(&mut cursor)
        .find(|child| child.kind() == node.kind())
        .is_some_and(|first| first.id() == node.id())
}

/// The node directly above `node` in source order. A body that begins at its
/// first declaration (a Python `block`) is stepped out of, because the grammar
/// leaves a comment between `class X:` and that first `def` outside the block.
fn node_above(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        if let Some(above) = current.prev_sibling() {
            return Some(above);
        }
        let parent = current.parent()?;
        if parent.start_byte() != current.start_byte() {
            return None;
        }
        current = parent;
    }
}

/// A comment sharing its line with the code before it annotates that code.
fn trails_code(comment: Node<'_>) -> bool {
    comment.prev_sibling().is_some_and(|before| {
        !before.kind().contains("comment") && last_row(before) == comment.start_position().row
    })
}

/// The last row a node's text occupies. A line comment that owns its newline
/// ends at column 0 of the next row, which it does not occupy.
fn last_row(node: Node<'_>) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

/// Promote a function-valued binding without teaching the walk any language.
fn function_binding_kind(node: Node<'_>) -> Option<ElementKind> {
    if !FUNCTION_BINDINGS.contains(&node.kind()) {
        return None;
    }
    let value = node.child_by_field_name("value")?;
    FUNCTION_VALUES
        .contains(&value.kind())
        .then_some(ElementKind::Function)
}

/// Stamp source order onto a sibling list.
fn ordered(mut elements: Vec<Element>) -> Vec<Element> {
    for (index, element) in elements.iter_mut().enumerate() {
        element.sibling_order = index as u32;
    }
    elements
}

/// Derive a node's own name from its fields, in [`NAME_FIELDS`] priority order.
fn node_name(node: Node<'_>, source: &str) -> Option<String> {
    NAME_FIELDS
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .and_then(|named| first_identifier_text(named, source))
}

/// Take a name node's text, descending to the first identifier when the node is
/// a compound declarator or a generic type.
///
/// Returns `None` for a blank node. Tree-sitter's error recovery inserts
/// *zero-width* MISSING nodes, so a name field can be present and still hold
/// nothing: `impl<T> {` yields an `impl_item` whose `type` field is empty. That
/// used to become an element with an empty address — addressable by nobody, and
/// a blank segment in the address of everything beneath it.
fn first_identifier_text(node: Node<'_>, source: &str) -> Option<String> {
    let text = node_text(node, source);
    if text.trim().is_empty() {
        return None;
    }
    if !text.contains(['(', '<', ' ', '*', '&']) {
        return Some(text);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind().contains("identifier") || child.kind().contains("type") {
            return first_identifier_text(child, source);
        }
    }
    Some(text)
}

fn node_text(node: Node<'_>, source: &str) -> String {
    source
        .get(node.byte_range())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use fs3_core::{ADDRESS_SEGMENT, ElementKind, ElementTree};

    use crate::scan;

    /// Rust that tree-sitter can only recover from.
    ///
    /// `impl<T> {` is the witness that matters: the grammar still produces an
    /// `impl_item` — which classifies — but fills its `type` field with a
    /// *zero-width* MISSING node. The field is present and empty, so this
    /// arrives as `Some("")` rather than `None`, which is exactly how an
    /// empty-named element used to slip through.
    const MALFORMED: &str = "fn good_one() {}\n\
                             \n\
                             impl<T> {\n\
                                 fn inside_a_nameless_impl() {}\n\
                             }\n\
                             \n\
                             fn also_good() {}\n";

    fn rust(source: &str) -> ElementTree {
        scan(Path::new("parsers/src/probe.rs"), source.as_bytes()).expect("Rust always parses")
    }

    fn addresses(tree: &ElementTree) -> Vec<String> {
        tree.iter().map(|e| e.address.clone()).collect()
    }

    /// State assertion: this really is the error-recovery path, so the negative
    /// assertions below are exercising the branch they claim to.
    #[test]
    fn the_malformed_fixture_actually_triggers_error_recovery() {
        assert!(
            rust(MALFORMED).has_error,
            "the fixture must put the tree in recovery, or this suite proves nothing"
        );
    }

    /// The finding this kills: nameless classified nodes were emitted as
    /// `<anonymous>`, which PRD req 42's "genuine named declaration" forbids.
    #[test]
    fn no_element_is_nameless_or_anonymous() {
        let tree = rust(MALFORMED);
        for element in tree.iter().filter(|e| e.kind != ElementKind::File) {
            assert!(
                !element.name.trim().is_empty(),
                "empty name from {}: {element:?}",
                element.subkind
            );
            assert!(
                !element.address.contains("<anonymous>"),
                "invented name from {}: {element:?}",
                element.subkind
            );
        }
    }

    /// Skipping the nameless node must not skip its subtree, and must not leave
    /// a hole in the addresses of what is inside it.
    #[test]
    fn children_of_a_nameless_node_are_still_found_and_cleanly_addressed() {
        let tree = rust(MALFORMED);
        let addresses = addresses(&tree);
        let file = "parsers/src/probe.rs";

        assert!(
            addresses.contains(&format!("{file}::good_one")),
            "{addresses:?}"
        );
        assert!(
            addresses.contains(&format!("{file}::also_good")),
            "{addresses:?}"
        );
        assert!(
            addresses.contains(&format!("{file}::inside_a_nameless_impl")),
            "the function inside the nameless impl must survive, reparented onto \
             the file rather than onto a nameless ghost: {addresses:?}"
        );
        assert!(
            addresses
                .iter()
                .all(|address| !address.ends_with(ADDRESS_SEGMENT) && !address.contains(":::")),
            "a skipped node must leave no gap in an address: {addresses:?}"
        );
    }

    /// Well-formed source is unaffected: nesting still produces nesting.
    #[test]
    fn well_formed_source_nests_through_its_scopes() {
        let tree = rust("mod geometry { struct Rect; impl Rect { fn area() {} } }");
        assert!(
            addresses(&tree).contains(&"parsers/src/probe.rs::geometry::Rect::area".to_string()),
            "{:?}",
            addresses(&tree)
        );
    }

    /// A receiver scopes the address without polluting the name (L4b).
    #[test]
    fn a_declaration_keeps_its_own_short_name() {
        let tree = rust("mod geometry { impl Rect { fn area() {} } }");
        let area = tree
            .find("parsers/src/probe.rs::geometry::Rect::area")
            .expect("nested fn is addressable");
        assert_eq!(area.name, "area", "the name is the declaration's own");
        assert_eq!(area.subkind, "function_item");
        assert_eq!(area.kind, ElementKind::Function);
    }

    /// Leading documentation (DL-042): the comments and attributes above a
    /// declaration are part of its text, so its vector and summary carry them.
    mod leading_documentation {
        use std::path::Path;

        use fs3_core::{Element, ElementTree};

        use crate::scan;

        fn tree(path: &str, source: &str) -> ElementTree {
            scan(Path::new(path), source.as_bytes()).expect("fixture parses")
        }

        fn element<'a>(tree: &'a ElementTree, name: &str) -> &'a Element {
            tree.iter()
                .find(|element| element.name == name)
                .unwrap_or_else(|| panic!("no element named {name}: {:#?}", tree.root))
        }

        fn first_line(element: &Element) -> &str {
            element.raw_text.lines().next().unwrap_or_default()
        }

        mod rust {
            use super::*;

            fn rust(source: &str) -> ElementTree {
                tree("src/probe.rs", source)
            }

            #[test]
            fn doc_comments_and_attributes_join_the_item_below() {
                let tree = rust(
                    "use std::fmt;\n\
                     \n\
                     /// Bounds a failed job's error before it is stored.\n\
                     /// Second line.\n\
                     #[inline]\n\
                     #[must_use]\n\
                     pub fn bounded(error: &str) -> String { error.into() }\n",
                );
                let bounded = element(&tree, "bounded");
                assert_eq!(
                    first_line(bounded),
                    "/// Bounds a failed job's error before it is stored."
                );
                assert!(bounded.raw_text.contains("#[must_use]\npub fn bounded"));
                assert_eq!((bounded.span.start_line, bounded.span.end_line), (3, 7));
                assert_eq!(
                    bounded.address, "src/probe.rs::bounded",
                    "addresses do not move"
                );
            }

            #[test]
            fn block_docs_plain_comments_and_nested_items_are_taken() {
                let tree = rust(
                    "/** A rectangle. */\n\
                     #[derive(Debug)]\n\
                     struct Rect;\n\
                     \n\
                     impl Rect {\n    \
                         // Width times height.\n    \
                         fn area(&self) -> u32 { 0 }\n\
                     }\n",
                );
                assert_eq!(first_line(element(&tree, "Rect")), "/** A rectangle. */");
                assert_eq!(
                    first_line(element(&tree, "area")).trim(),
                    "// Width times height."
                );
                assert_eq!(element(&tree, "area").span.start_line, 6);
            }

            #[test]
            fn doc_examples_that_end_like_code_still_attach() {
                let tree = rust(
                    "/// Adds one.\n\
                     /// ```\n\
                     /// let two = inc(1);\n\
                     /// ```\n\
                     fn inc(n: u32) -> u32 { n + 1 }\n",
                );
                assert_eq!(first_line(element(&tree, "inc")), "/// Adds one.");
            }

            #[test]
            fn a_comment_set_apart_by_a_blank_line_is_not_taken() {
                let tree = rust("fn before() {}\n// About the section.\n\nfn gap() {}\n");
                assert_eq!(first_line(element(&tree, "gap")), "fn gap() {}");
            }

            #[test]
            fn a_file_header_stays_with_the_file() {
                let licence = rust(
                    "// Copyright 2026 Example.\n// SPDX-License-Identifier: MIT\nfn first() {}\n",
                );
                assert_eq!(first_line(element(&licence, "first")), "fn first() {}");

                let module_doc = rust("//! What this module is for.\nfn first() {}\n");
                assert_eq!(first_line(element(&module_doc, "first")), "fn first() {}");

                let inner = rust("#![allow(dead_code)]\nfn first() {}\n");
                assert_eq!(first_line(element(&inner, "first")), "fn first() {}");

                let header_then_doc =
                    rust("// Copyright 2026 Example.\n/// Starts things.\nfn first() {}\n");
                assert_eq!(
                    first_line(element(&header_then_doc, "first")),
                    "/// Starts things."
                );
            }

            #[test]
            fn commented_out_code_is_not_documentation() {
                let tree = rust(
                    "fn keep() {}\n\
                     // fn old() {\n\
                     //     legacy();\n\
                     // }\n\
                     fn replacement() {}\n",
                );
                assert_eq!(
                    first_line(element(&tree, "replacement")),
                    "fn replacement() {}"
                );

                let mixed = rust(
                    "fn keep() {}\n// let retired = 1;\n// Replaces the retired value.\nfn current() {}\n",
                );
                assert_eq!(
                    first_line(element(&mixed, "current")),
                    "// Replaces the retired value."
                );
            }

            #[test]
            fn a_comment_trailing_the_line_above_is_not_taken() {
                let tree = rust("fn a() {} // about a\nfn b() {}\n");
                assert_eq!(first_line(element(&tree, "b")), "fn b() {}");
            }
        }

        mod python {
            use super::*;

            fn python(source: &str) -> ElementTree {
                tree("pkg/probe.py", source)
            }

            #[test]
            fn comments_and_decorators_join_the_definition_and_docstrings_already_do() {
                let tree = python(
                    "import functools\n\
                     \n\
                     # Caches the expensive lookup.\n\
                     @functools.cache\n\
                     def lookup(key):\n    \
                         \"\"\"Find the value stored under key.\"\"\"\n    \
                         return key\n",
                );
                let lookup = element(&tree, "lookup");
                assert_eq!(first_line(lookup), "# Caches the expensive lookup.");
                assert!(lookup.raw_text.contains("@functools.cache\ndef lookup"));
                assert!(
                    lookup
                        .raw_text
                        .contains("\"\"\"Find the value stored under key.\"\"\"")
                );
                assert_eq!(lookup.span.start_line, 3);
            }

            #[test]
            fn a_comment_after_a_dedent_documents_the_next_definition() {
                let tree = python(
                    "def first():\n    \
                         return 1\n\
                     \n\
                     # Explains second.\n\
                     def second():\n    \
                         return 2\n",
                );
                assert_eq!(first_line(element(&tree, "second")), "# Explains second.");
                assert!(!element(&tree, "first").raw_text.contains("Explains"));
            }

            #[test]
            fn methods_take_their_comments_and_negatives_hold() {
                let tree = python(
                    "#!/usr/bin/env python\n\
                     class Store:\n    \
                         # Opens the store.\n    \
                         def open(self):\n        \
                             pass\n\
                     \n    \
                         # About the section.\n\
                     \n    \
                         def close(self):\n        \
                             pass\n",
                );
                assert_eq!(first_line(element(&tree, "Store")), "class Store:");
                assert_eq!(
                    first_line(element(&tree, "open")).trim(),
                    "# Opens the store."
                );
                assert_eq!(
                    first_line(element(&tree, "close")).trim(),
                    "def close(self):"
                );
            }
        }

        mod typescript {
            use super::*;

            #[test]
            fn jsdoc_reaches_through_export_and_bindings() {
                let tree = tree(
                    "src/probe.ts",
                    "import { x } from './x';\n\
                     \n\
                     /** Handles one request. */\n\
                     export function handler(): void {}\n\
                     \n\
                     /**\n * Builds a maker.\n */\n\
                     export const make = () => 1;\n\
                     \n\
                     // Plain note on a local.\n\
                     const local = function () {};\n",
                );
                let handler = element(&tree, "handler");
                assert_eq!(first_line(handler), "/** Handles one request. */");
                assert!(handler.raw_text.contains("export function handler"));
                assert_eq!(handler.span.start_line, 3);
                assert_eq!(first_line(element(&tree, "make")), "/**");
                assert!(element(&tree, "make").raw_text.contains("Builds a maker."));
                assert_eq!(
                    first_line(element(&tree, "local")),
                    "// Plain note on a local."
                );
            }

            #[test]
            fn class_members_take_jsdoc_and_decorators() {
                let tree = tree(
                    "src/probe.ts",
                    "/** A service. */\n\
                     @Injectable()\n\
                     export class Service {\n  \
                         /** Starts it. */\n  \
                         @Log()\n  \
                         start(): void {}\n\
                     \n  \
                         // stop(): void {}\n\
                     \n  \
                         halt(): void {}\n\
                     }\n",
                );
                assert_eq!(first_line(element(&tree, "Service")), "/** A service. */");
                assert_eq!(
                    first_line(element(&tree, "start")).trim(),
                    "/** Starts it. */"
                );
                assert!(element(&tree, "start").raw_text.contains("@Log()"));
                assert_eq!(first_line(element(&tree, "halt")).trim(), "halt(): void {}");
            }

            #[test]
            fn a_second_binding_does_not_take_the_first_bindings_doc() {
                let tree = tree(
                    "src/probe.ts",
                    "/** About a. */\nconst a = () => 1, b = () => 2;\n",
                );
                assert_eq!(first_line(element(&tree, "a")), "/** About a. */");
                assert!(!element(&tree, "b").raw_text.contains("About a"));
            }

            #[test]
            fn tsx_components_take_their_jsdoc() {
                let tree = tree(
                    "src/view.tsx",
                    "/** Renders the page. */\nexport function View() { return <main>Hi</main>; }\n",
                );
                assert_eq!(
                    first_line(element(&tree, "View")),
                    "/** Renders the page. */"
                );
            }
        }
    }
}
