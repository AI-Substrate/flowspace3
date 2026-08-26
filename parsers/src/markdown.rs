//! Markdown sections (PRD req 22).
//!
//! tree-sitter-md reports headings as *point* nodes, so a section's range is
//! fs3 code rather than grammar output (POC learning L9): a section runs from
//! its heading line to the line before the next heading of equal-or-shallower
//! level.
//!
//! Doing this through the AST rather than `grep '^#'` is not ceremony. The POC
//! measured a 43 KB doc where the regex found 32 heading-looking lines, 8 of
//! them shell comments inside fenced code blocks; tree-sitter returned exactly
//! the 24 real headings.

use fs3_core::{BlobRef, Element, ElementKind, classify};
use tree_sitter::{Node, Parser};

use crate::ParseError;

/// Separator between nested heading segments.
const SEGMENT: &str = " > ";

struct Heading {
    level: usize,
    title: String,
    /// 0-based row of the heading line.
    row: usize,
    ts_kind: String,
}

/// Parse Markdown into one element per heading section.
pub(crate) fn parse_markdown(
    path: &str,
    blob: &BlobRef,
    source: &str,
) -> Result<Vec<Element>, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .map_err(|_| ParseError::Unparseable {
            path: path.to_string(),
            language: "markdown",
        })?;
    let tree = parser.parse(source, None).ok_or(ParseError::Unparseable {
        path: path.to_string(),
        language: "markdown",
    })?;
    let has_error = tree.root_node().has_error();

    let lines: Vec<&str> = source.lines().collect();
    let mut headings = Vec::new();
    collect_headings(tree.root_node(), source, &mut headings);
    headings.sort_by_key(|heading| heading.row);

    let last_row = lines.len().saturating_sub(1);
    let mut elements = Vec::with_capacity(headings.len());
    let mut scope: Vec<(usize, String)> = Vec::new();

    for (index, heading) in headings.iter().enumerate() {
        // The section ends just before the next heading at the same or a
        // shallower level; otherwise at end of file.
        let end_row = headings[index + 1..]
            .iter()
            .find(|next| next.level <= heading.level)
            .map_or(last_row, |next| next.row.saturating_sub(1));

        scope.retain(|(level, _)| *level < heading.level);
        let qualified_name = scope
            .iter()
            .map(|(_, title)| title.as_str())
            .chain(std::iter::once(heading.title.as_str()))
            .collect::<Vec<_>>()
            .join(SEGMENT);
        scope.push((heading.level, heading.title.clone()));

        elements.push(Element {
            path: path.to_string(),
            blob: blob.clone(),
            ts_kind: heading.ts_kind.clone(),
            kind: ElementKind::Section,
            qualified_name,
            start_line: heading.row as u32 + 1,
            end_line: end_row as u32 + 1,
            text: lines
                .get(heading.row..=end_row.max(heading.row))
                .unwrap_or_default()
                .join("\n"),
            has_error,
        });
    }

    Ok(elements)
}

fn collect_headings(node: Node<'_>, source: &str, out: &mut Vec<Heading>) {
    if classify(node.kind()) == Some(ElementKind::Section) {
        let raw = source.get(node.byte_range()).unwrap_or_default();
        if let Some(heading) = heading_from(node, raw, source) {
            out.push(heading);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_headings(child, source, out);
    }
}

fn heading_from(node: Node<'_>, raw: &str, source: &str) -> Option<Heading> {
    let first_line = raw.lines().next()?.trim();
    let level = if node.kind() == "setext_heading" {
        // `===` is h1, `---` is h2 — the marker is the grammar's child kind.
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find_map(|child| match child.kind() {
                "setext_h1_underline" => Some(1),
                "setext_h2_underline" => Some(2),
                _ => None,
            })
            .unwrap_or(1)
    } else {
        first_line.chars().take_while(|c| *c == '#').count().max(1)
    };

    let title = first_line
        .trim_start_matches('#')
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }

    let _ = source;
    Some(Heading {
        level,
        title,
        row: node.start_position().row,
        ts_kind: node.kind().to_string(),
    })
}
