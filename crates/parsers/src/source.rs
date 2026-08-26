//! The generic code walk. One function, no language branches.

use fs3_core::{BlobRef, Element, classify};
use tree_sitter::{Node, Parser};

use crate::ParseError;

/// Fields that can carry a node's name, in priority order.
///
/// The order is load-bearing (POC learning L3). `type` must come **last**: it
/// is the target type for a Rust `impl_item`, but the *return* type in C and
/// C++, where trying it early produces elements literally named `void` and
/// `bool`.
const NAME_FIELDS: &[&str] = &["name", "declarator", "path", "pattern", "type"];

/// Kinds that contribute a qualified-name segment without being elements
/// themselves (POC learning L4a): modules, namespaces, packages.
///
/// This is what turns `UserService` into `MyApp.Services.UserService`.
fn is_container_only(kind: &str) -> bool {
    kind.contains("namespace")
        || matches!(
            kind,
            "mod_item" | "module_declaration" | "package_declaration"
        )
}

/// Separator between qualified-name segments in code.
const SEGMENT: &str = ".";

/// Parse Rust source into elements.
pub(crate) fn parse_rust(
    path: &str,
    blob: &BlobRef,
    source: &str,
) -> Result<Vec<Element>, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|_| ParseError::Unparseable {
            path: path.to_string(),
            language: "rust",
        })?;
    let tree = parser.parse(source, None).ok_or(ParseError::Unparseable {
        path: path.to_string(),
        language: "rust",
    })?;

    // L8: `has_error` is metadata. Error recovery routinely yields correct
    // elements, so it must never be used to reject a file.
    let has_error = tree.root_node().has_error();

    let mut elements = Vec::new();
    let mut scope: Vec<String> = Vec::new();
    walk(
        tree.root_node(),
        source,
        path,
        blob,
        has_error,
        &mut scope,
        &mut elements,
    );
    Ok(elements)
}

fn walk(
    node: Node<'_>,
    source: &str,
    path: &str,
    blob: &BlobRef,
    has_error: bool,
    scope: &mut Vec<String>,
    out: &mut Vec<Element>,
) {
    let kind = node.kind();
    let name = node_name(node, source);

    let mut pushed_scope = false;

    // PRD req 42 wants genuine *named* declarations. A classified node with no
    // name is not one — it is error-recovery debris, or a suffix match on an
    // anonymous construct. Emitting it as `<anonymous>` invented an element
    // nobody can address, and worse, pushed that junk onto the scope of every
    // real declaration beneath it. Skip the node but keep walking its children:
    // a nameless parent must not cost us the named declarations inside it.
    if let Some(element_kind) = classify(kind)
        && let Some(name) = name.clone()
    {
        let qualified_name = qualify(scope, &name);
        out.push(Element {
            path: path.to_string(),
            blob: blob.clone(),
            ts_kind: kind.to_string(),
            kind: element_kind,
            qualified_name,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            text: node_text(node, source),
            has_error,
        });
        scope.push(name);
        pushed_scope = true;
    } else if is_container_only(kind)
        && let Some(name) = name
    {
        scope.push(name);
        pushed_scope = true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, path, blob, has_error, scope, out);
    }

    if pushed_scope {
        scope.pop();
    }
}

/// Derive a node's name from its fields, in [`NAME_FIELDS`] priority order.
///
/// A `receiver` field scopes a callable onto its type (POC learning L4b), which
/// is how Go's `Add` becomes `Calculator.Add`.
fn node_name(node: Node<'_>, source: &str) -> Option<String> {
    let receiver = node
        .child_by_field_name("receiver")
        .and_then(|r| first_identifier_text(r, source));

    let own = NAME_FIELDS
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .and_then(|named| first_identifier_text(named, source))?;

    Some(match receiver {
        Some(receiver) => format!("{receiver}{SEGMENT}{own}"),
        None => own,
    })
}

/// Take a name node's text, descending to the first identifier when the node is
/// a compound declarator or a generic type.
///
/// Returns `None` for a blank node. Tree-sitter's error recovery inserts
/// *zero-width* MISSING nodes, so a name field can be present and still hold
/// nothing: `impl<T> {` yields an `impl_item` whose `type` field is empty. That
/// used to become an element with an empty qualified name — addressable by
/// nobody, and a blank segment in the scope of everything beneath it.
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

fn qualify(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}{SEGMENT}{name}", scope.join(SEGMENT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn parse(source: &str) -> Vec<Element> {
        let blob = BlobRef::new("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391")
            .expect("literal is a valid digest");
        parse_rust("parsers/src/probe.rs", &blob, source).expect("Rust always parses")
    }

    /// State assertion: this really is the error-recovery path, so the negative
    /// assertions below are exercising the branch they claim to.
    #[test]
    fn the_malformed_fixture_actually_triggers_error_recovery() {
        let elements = parse(MALFORMED);
        assert!(
            elements.iter().all(|element| element.has_error),
            "the fixture must put the tree in recovery, or this suite proves nothing"
        );
    }

    /// The finding this kills: nameless classified nodes were emitted as
    /// `<anonymous>`, which PRD req 42's "genuine named declaration" forbids.
    #[test]
    fn no_element_is_nameless_or_anonymous() {
        for element in parse(MALFORMED) {
            assert!(
                !element.qualified_name.trim().is_empty(),
                "empty name from {}: {element:?}",
                element.ts_kind
            );
            assert!(
                !element.qualified_name.contains("<anonymous>"),
                "invented name from {}: {element:?}",
                element.ts_kind
            );
        }
    }

    /// Skipping the nameless node must not skip its subtree, and must not leave
    /// a hole in the qualified names of what is inside it.
    #[test]
    fn children_of_a_nameless_node_are_still_found_and_cleanly_named() {
        let names: Vec<String> = parse(MALFORMED)
            .into_iter()
            .map(|element| element.qualified_name)
            .collect();

        assert!(names.contains(&"good_one".to_string()), "{names:?}");
        assert!(names.contains(&"also_good".to_string()), "{names:?}");
        assert!(
            names.contains(&"inside_a_nameless_impl".to_string()),
            "the function inside the nameless impl must survive, unprefixed: {names:?}"
        );
        assert!(
            names.iter().all(|name| !name.starts_with(SEGMENT)
                && !name.contains("..")
                && !name.ends_with(SEGMENT)),
            "a skipped node must leave no gap in a qualified name: {names:?}"
        );
    }

    /// Well-formed source is unaffected: scoping still nests.
    #[test]
    fn well_formed_source_still_qualifies_through_its_scopes() {
        let names: Vec<String> = parse("mod geometry { struct Rect; impl Rect { fn area() {} } }")
            .into_iter()
            .map(|element| element.qualified_name)
            .collect();
        assert!(
            names.contains(&"geometry.Rect.area".to_string()),
            "{names:?}"
        );
    }
}
