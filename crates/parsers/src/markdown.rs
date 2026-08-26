//! Markdown sections (PRD req 22).
//!
//! tree-sitter-md reports headings as *point* nodes, so a section's range is
//! fs3 code rather than grammar output (POC learning L9): a section runs from
//! its heading line to the line before the next heading of equal-or-shallower
//! level. Those ranges nest exactly, which is what makes a heading hierarchy a
//! tree rather than a flat list with a naming convention.
//!
//! Doing this through the AST rather than `grep '^#'` is not ceremony. The POC
//! measured a 43 KB doc where the regex found 32 heading-looking lines, 8 of
//! them shell comments inside fenced code blocks; tree-sitter returned exactly
//! the 24 real headings.

use fs3_core::{ADDRESS_SEGMENT, Element, ElementKind, Span, classify};
use tree_sitter::Node;

struct Heading {
    level: usize,
    title: String,
    /// 0-based row of the heading line.
    row: usize,
    /// 0-based row of the last line the section covers.
    end_row: usize,
    ts_kind: String,
}

/// Every heading section in the document, nested by heading level.
pub(crate) fn sections(root: Node<'_>, source: &str, path: &str) -> Vec<Element> {
    let lines: Vec<&str> = source.lines().collect();

    let mut found = Vec::new();
    collect_headings(root, source, &mut found);
    found.sort_by_key(|heading| heading.row);

    // A section ends just before the next heading at the same or a shallower
    // level; otherwise at end of file.
    let last_row = lines.len().saturating_sub(1);
    let ends: Vec<usize> = found
        .iter()
        .enumerate()
        .map(|(index, heading)| {
            found[index + 1..]
                .iter()
                .find(|next| next.level <= heading.level)
                .map_or(last_row, |next| next.row.saturating_sub(1))
        })
        .collect();
    for (heading, end_row) in found.iter_mut().zip(ends) {
        heading.end_row = end_row;
    }

    nest(&found, &lines, path)
}

/// Turn a flat, source-ordered heading list into a forest.
///
/// The first heading owns every following heading deeper than it; the next
/// heading at its level or shallower starts a sibling.
fn nest(headings: &[Heading], lines: &[&str], scope: &str) -> Vec<Element> {
    let mut out: Vec<Element> = Vec::new();
    let mut index = 0;

    while index < headings.len() {
        let heading = &headings[index];
        let end = headings[index + 1..]
            .iter()
            .position(|next| next.level <= heading.level)
            .map_or(headings.len(), |offset| index + 1 + offset);

        let address = format!("{scope}{ADDRESS_SEGMENT}{}", heading.title);
        let children = nest(&headings[index + 1..end], lines, &address);

        let body = lines
            .get(heading.row..=heading.end_row.max(heading.row))
            .unwrap_or_default()
            .join("\n");

        out.push(
            Element::new(
                ElementKind::Section,
                &heading.ts_kind,
                &heading.title,
                &address,
                Span::new(heading.row as u32 + 1, heading.end_row as u32 + 1),
                body,
            )
            .with_sibling_order(out.len() as u32)
            .with_children(children),
        );

        index = end;
    }

    out
}

fn collect_headings(node: Node<'_>, source: &str, out: &mut Vec<Heading>) {
    if classify(node.kind()) == Some(ElementKind::Section)
        && let Some(heading) = heading_from(node, source)
    {
        out.push(heading);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_headings(child, source, out);
    }
}

fn heading_from(node: Node<'_>, source: &str) -> Option<Heading> {
    let raw = source.get(node.byte_range())?;
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
        end_row: node.start_position().row,
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
    use std::path::Path;

    use fs3_core::ElementTree;

    use crate::scan;

    const DOC: &str = "docs/probe.md";

    fn markdown(source: &str) -> ElementTree {
        scan(Path::new(DOC), source.as_bytes()).expect("markdown always parses")
    }

    /// Addresses of every section, file element excluded.
    fn titles(source: &str) -> Vec<String> {
        markdown(source)
            .iter()
            .skip(1)
            .map(|element| {
                element
                    .address
                    .strip_prefix(&format!("{DOC}::"))
                    .expect("every section hangs off the file")
                    .to_string()
            })
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
            vec!["Parent".to_string(), "Parent::Child".to_string()]
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

    /// A level jump (h1 straight to h3) still nests: the h3 is inside the h1,
    /// because nesting follows depth, not a fixed step of one.
    #[test]
    fn a_skipped_heading_level_still_nests() {
        let tree = markdown("# Top\n\n### Deep\n\nText.\n");
        let top = tree.find(&format!("{DOC}::Top")).expect("h1");
        assert_eq!(top.children.len(), 1, "the h3 belongs to the h1");
        assert_eq!(top.children[0].name, "Deep");
    }

    /// A shallower heading closes the deeper ones and starts a sibling.
    #[test]
    fn a_shallower_heading_pops_back_out() {
        let tree = markdown("# One\n\n## Under\n\n# Two\n\nText.\n");
        assert_eq!(tree.root.children.len(), 2, "two h1 siblings");
        assert_eq!(tree.root.children[0].children.len(), 1);
        assert_eq!(tree.root.children[1].sibling_order, 1);
        assert!(tree.root.children[1].children.is_empty());
    }
}
