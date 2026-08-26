//! Exemplar: the parser fixture tier.
//!
//! Known files in, an exact element table out. Copy this shape when a grammar
//! is added — the assertion is the whole table, not a spot-check, because a
//! classifier regression shows up as an *extra* row far more often than a
//! missing one.

use fs3_core::{BlobRef, ElementKind};
use fs3_parsers::parse;

fn blob() -> BlobRef {
    BlobRef::new("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391").unwrap()
}

/// `(ts_kind, kind, qualified_name, start_line, end_line)`
type Row = (&'static str, ElementKind, &'static str, u32, u32);

fn table(elements: &[fs3_core::Element]) -> Vec<(String, ElementKind, String, u32, u32)> {
    elements
        .iter()
        .map(|e| {
            (
                e.ts_kind.clone(),
                e.kind,
                e.qualified_name.clone(),
                e.start_line,
                e.end_line,
            )
        })
        .collect()
}

fn expect(actual: &[fs3_core::Element], expected: &[Row]) {
    let expected: Vec<(String, ElementKind, String, u32, u32)> = expected
        .iter()
        .map(|(kind, category, name, start, end)| {
            (
                (*kind).to_string(),
                *category,
                (*name).to_string(),
                *start,
                *end,
            )
        })
        .collect();
    assert_eq!(table(actual), expected);
}

#[test]
fn rust_fixture_yields_the_expected_element_table() {
    let source = include_str!("../fixtures/sample.rs");
    let elements = parse("parsers/fixtures/sample.rs", &blob(), source).unwrap();

    expect(
        &elements,
        &[
            // Line spans are the declaration's own extent — the doc comment on
            // `Rect` is a sibling node, so the struct starts at 5, not 4.
            ("struct_item", ElementKind::Type, "geometry.Rect", 5, 8),
            // `impl Rect` is both an element and a name scope; its name comes
            // from the `type` field, tried last so C/C++ return types never win.
            ("impl_item", ElementKind::Type, "geometry.Rect", 10, 20),
            (
                "function_item",
                ElementKind::Callable,
                "geometry.Rect.new",
                11,
                15,
            ),
            (
                "function_item",
                ElementKind::Callable,
                "geometry.Rect.area",
                17,
                19,
            ),
            ("trait_item", ElementKind::Type, "geometry.Shape", 22, 24),
            // A bodiless trait method is still a declaration.
            (
                "function_signature_item",
                ElementKind::Callable,
                "geometry.Shape.area",
                23,
                23,
            ),
            ("enum_item", ElementKind::Type, "geometry.Kind", 26, 29),
            ("function_item", ElementKind::Callable, "main_entry", 34, 36),
        ],
    );
}

/// PRD req 42 / POC learning L2, on a real file rather than a kind string:
/// `mod geometry` scopes but is not an element, `Self { .. }` is not a type,
/// `MAX_SIDES` is not a callable, and enum variants are not declarations.
#[test]
fn rust_fixture_invents_nothing() {
    let source = include_str!("../fixtures/sample.rs");
    let elements = parse("parsers/fixtures/sample.rs", &blob(), source).unwrap();

    let kinds: Vec<&str> = elements.iter().map(|e| e.ts_kind.as_str()).collect();
    for refused in [
        "struct_expression",
        "mod_item",
        "const_item",
        "enum_variant",
        "field_declaration",
    ] {
        assert!(
            !kinds.contains(&refused),
            "{refused} must not become an element; got {kinds:?}"
        );
    }

    // `mod geometry` is invisible as an element but visible in every name.
    assert!(
        elements
            .iter()
            .filter(|e| e.qualified_name != "main_entry")
            .all(|e| e.qualified_name.starts_with("geometry.")),
        "the module must contribute a qualified-name segment"
    );
}

#[test]
fn markdown_fixture_yields_nested_sections_with_real_spans() {
    let source = include_str!("../fixtures/sample.md");
    let elements = parse("parsers/fixtures/sample.md", &blob(), source).unwrap();

    expect(
        &elements,
        &[
            ("atx_heading", ElementKind::Section, "Main Title", 1, 20),
            (
                "atx_heading",
                ElementKind::Section,
                "Main Title > Section One",
                5,
                12,
            ),
            (
                "atx_heading",
                ElementKind::Section,
                "Main Title > Section One > Subsection 1.1",
                9,
                12,
            ),
            (
                "atx_heading",
                ElementKind::Section,
                "Main Title > Section Two",
                13,
                20,
            ),
        ],
    );
}

/// POC learning L9 — the concrete argument for parsing markdown rather than
/// grepping it. `grep '^#'` finds five heading-looking lines in this fixture;
/// one of them is a shell comment inside a fenced code block.
#[test]
fn fenced_code_comments_are_not_headings() {
    let source = include_str!("../fixtures/sample.md");
    let heading_shaped_lines = source.lines().filter(|l| l.starts_with('#')).count();
    assert_eq!(heading_shaped_lines, 5, "the fixture must contain the trap");

    let elements = parse("parsers/fixtures/sample.md", &blob(), source).unwrap();
    assert_eq!(
        elements.len(),
        4,
        "only the four real headings are sections"
    );
    assert!(
        !elements
            .iter()
            .any(|e| e.qualified_name.contains("shell comment")),
        "a fenced code comment leaked into the element table"
    );
}

/// Element text is what a `Summarizer` reads, so a section must carry its whole
/// body — not just its heading line.
#[test]
fn section_text_carries_the_body_not_just_the_heading() {
    let source = include_str!("../fixtures/sample.md");
    let elements = parse("parsers/fixtures/sample.md", &blob(), source).unwrap();

    let section_one = elements
        .iter()
        .find(|e| e.qualified_name == "Main Title > Section One")
        .expect("fixture has Section One");
    assert!(section_one.text.starts_with("## Section One"));
    assert!(section_one.text.contains("Body of section one."));
    assert!(section_one.text.contains("### Subsection 1.1"));
    assert!(!section_one.text.contains("## Section Two"));
}
