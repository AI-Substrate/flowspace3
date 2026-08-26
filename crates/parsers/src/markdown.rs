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

    let title = heading_title(node, source)?;
    if title.is_empty() {
        return None;
    }

    Some(Heading {
        level,
        title,
        row: node.start_position().row,
        ts_kind: node.kind().to_string(),
    })
}

/// The heading's content, taken from the grammar rather than from the raw line.
///
/// tree-sitter-md exposes the content as an `inline` node, so the level markers
/// are the grammar's business, not ours to guess at. The old code stripped `#`
/// off both ends of the raw line unconditionally, which quietly turned the
/// setext heading `C#` into `C`.
fn heading_title(node: Node<'_>, source: &str) -> Option<String> {
    let inline = first_inline(node)?;
    let content = source.get(inline.byte_range())?.trim();
    let content = if node.kind() == "atx_heading" {
        strip_closing_sequence(content)
    } else {
        content
    };
    Some(content.trim().to_string())
}

/// The first `inline` descendant — the heading's text, marker excluded. ATX
/// hangs it directly off the heading; setext wraps it in a `paragraph`.
fn first_inline<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
    if node.kind() == "inline" {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).find_map(first_inline)
}

/// Remove an ATX *closing sequence*, and only that.
///
/// CommonMark defines one as a run of `#` at the end of the line that is
/// preceded by a space or forms the entire content. So `# C#` keeps its hash
/// and `## Title ##` loses one — a distinction `trim_end_matches('#')` cannot
/// make. Setext headings have no closing sequence at all, so they never reach
/// this function.
fn strip_closing_sequence(content: &str) -> &str {
    let trimmed = content.trim_end();
    let without = trimmed.trim_end_matches('#');
    if without.len() == trimmed.len() {
        return trimmed;
    }
    if without.is_empty() {
        return "";
    }
    if without.ends_with([' ', '\t']) {
        without.trim_end()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(source: &str) -> Vec<String> {
        let blob = BlobRef::new("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391")
            .expect("literal is a valid digest");
        parse_markdown("docs/probe.md", &blob, source)
            .expect("markdown always parses")
            .into_iter()
            .map(|element| element.qualified_name)
            .collect()
    }

    /// The finding this kills: an unconditional `trim_end_matches('#')` turned
    /// the setext heading `C#` into `C`.
    #[test]
    fn a_setext_heading_keeps_a_trailing_hash() {
        assert_eq!(titles("C#\n===\n\nText.\n"), vec!["C#".to_string()]);
        assert_eq!(titles("F#\n---\n\nText.\n"), vec!["F#".to_string()]);
    }

    #[test]
    fn an_atx_heading_keeps_a_hash_that_belongs_to_the_word() {
        assert_eq!(titles("# C#\n\nText.\n"), vec!["C#".to_string()]);
    }

    #[test]
    fn an_atx_closing_sequence_is_removed() {
        assert_eq!(titles("## Title ##\n\nText.\n"), vec!["Title".to_string()]);
        assert_eq!(
            titles("## Title   ####  \n\nText.\n"),
            vec!["Title".to_string()]
        );
    }

    #[test]
    fn setext_headings_still_carry_their_level() {
        assert_eq!(
            titles("Parent\n======\n\nChild\n------\n\nText.\n"),
            vec!["Parent".to_string(), "Parent > Child".to_string()]
        );
    }

    /// L9 again: a heading-shaped line inside a fenced block is not a heading.
    #[test]
    fn a_hash_inside_a_fenced_block_is_not_a_heading() {
        assert_eq!(
            titles("# Real\n\n```sh\n# not a heading\n```\n"),
            vec!["Real".to_string()]
        );
    }
}
