//! The generic code walk. One function, no language branches.
//!
//! The shape is: descend the syntax tree; a node that classifies *and* has a
//! name becomes an element and its subtree becomes that element's children; a
//! node that does neither is spliced through, so the declarations inside a
//! wrapper node (a `decorated_definition`, an error-recovery stub, a bare
//! block) are never lost — they simply attach to the nearest real ancestor.

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

    Some(
        Element::new(
            kind,
            ts_kind,
            name,
            &address,
            Span::new(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
            ),
            node_text(node, source),
        )
        .with_children(ordered(children)),
    )
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
}
