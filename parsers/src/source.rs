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

    if let Some(element_kind) = classify(kind) {
        let name = name.clone().unwrap_or_else(|| "<anonymous>".to_string());
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
fn first_identifier_text(node: Node<'_>, source: &str) -> Option<String> {
    let text = node_text(node, source);
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
