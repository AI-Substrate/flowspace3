//! Exemplar: the parser fixture tier.
//!
//! Known files in, an exact element **tree** out. Copy this shape when a
//! grammar is added — the assertion is the whole tree, not a spot-check,
//! because a classifier regression shows up as an *extra* row far more often
//! than a missing one, and a parenting regression shows up as the same rows at
//! the wrong depth.
//!
//! Every row reads `<kind> <subkind> <address> #<sibling_order> <span>`,
//! indented by depth. That one line covers everything the model promises about
//! a node except its text, which the hash tests below cover instead.

use std::collections::BTreeMap;
use std::path::Path;

use fs3_core::{Element, ElementKind, ElementTree};
use fs3_parsers::scan;

const RUST_FIXTURE: &str = include_str!("../fixtures/sample.rs");
const PYTHON_FIXTURE: &str = include_str!("../fixtures/sample.py");
const MARKDOWN_FIXTURE: &str = include_str!("../fixtures/sample.md");
const TYPESCRIPT_FIXTURE: &str = include_str!("../fixtures/sample.ts");
const TSX_FIXTURE: &str = include_str!("../fixtures/sample.tsx");

const RUST_PATH: &str = "parsers/fixtures/sample.rs";
const PYTHON_PATH: &str = "parsers/fixtures/sample.py";
const MARKDOWN_PATH: &str = "parsers/fixtures/sample.md";
const TYPESCRIPT_PATH: &str = "parsers/fixtures/sample.ts";
const TSX_PATH: &str = "parsers/fixtures/sample.tsx";

fn tree(path: &str, source: &str) -> ElementTree {
    scan(Path::new(path), source.as_bytes()).expect("fixtures parse")
}

/// One line per element, indented by depth — so the assertion covers parenting
/// and sibling order, not just membership.
fn rows(tree: &ElementTree) -> Vec<String> {
    fn walk(element: &Element, depth: usize, out: &mut Vec<String>) {
        out.push(format!(
            "{:indent$}{} {} {} #{} {}",
            "",
            element.kind,
            element.subkind,
            element.address,
            element.sibling_order,
            element.span,
            indent = depth * 2
        ));
        for child in &element.children {
            walk(child, depth + 1, out);
        }
    }

    let mut out = Vec::new();
    walk(&tree.root, 0, &mut out);
    out
}

fn expect(tree: &ElementTree, expected: &[&str]) {
    assert_eq!(rows(tree), expected, "the whole tree, not a spot-check");
}

/// Every subkind in the tree, root included.
fn subkinds(tree: &ElementTree) -> Vec<&str> {
    tree.iter()
        .map(|element| element.subkind.as_str())
        .collect()
}

#[test]
fn rust_fixture_yields_the_expected_tree() {
    expect(
        &tree(RUST_PATH, RUST_FIXTURE),
        &[
            "file rust parsers/fixtures/sample.rs #0 1-42",
            // A module is a container element AND the parent of what it holds.
            "  container mod_item parsers/fixtures/sample.rs::geometry #0 3-30",
            // Line spans are the declaration's own extent — the doc comment on
            // `Rect` is a sibling node, so the struct starts at 5, not 4.
            "    container struct_item parsers/fixtures/sample.rs::geometry::Rect #0 5-8",
            // `impl Rect` shares the struct's address on purpose: it is the same
            // logical entity, seen in another piece. An address identifies a
            // thing, not a node — `(address, span)` identifies a node.
            "    container impl_item parsers/fixtures/sample.rs::geometry::Rect #1 10-20",
            // Methods hang off the impl, and their address reads as you would
            // write it by hand.
            "      function function_item parsers/fixtures/sample.rs::geometry::Rect::new #0 11-15",
            "      function function_item parsers/fixtures/sample.rs::geometry::Rect::area #1 17-19",
            "    container trait_item parsers/fixtures/sample.rs::geometry::Shape #2 22-24",
            // A bodiless trait method is still a declaration.
            "      function function_signature_item parsers/fixtures/sample.rs::geometry::Shape::area #0 23-23",
            "    container enum_item parsers/fixtures/sample.rs::geometry::Kind #3 26-29",
            "  function function_item parsers/fixtures/sample.rs::main_entry #1 34-42",
            // A fn inside a fn belongs to that fn, not to the file.
            "    function function_item parsers/fixtures/sample.rs::main_entry::half #0 37-39",
        ],
    );
}

/// PRD req 42 / POC learning L2, on a real file rather than a kind string:
/// `Self { .. }` is not a container, `MAX_SIDES` is not anything, and enum
/// variants and struct fields are not declarations.
#[test]
fn rust_fixture_invents_nothing() {
    let tree = tree(RUST_PATH, RUST_FIXTURE);
    let subkinds = subkinds(&tree);

    for refused in [
        "struct_expression",
        "const_item",
        "enum_variant",
        "field_declaration",
    ] {
        assert!(
            !subkinds.contains(&refused),
            "{refused} must not become an element; got {subkinds:?}"
        );
    }
}

#[test]
fn python_fixture_yields_the_expected_tree() {
    expect(
        &tree(PYTHON_PATH, PYTHON_FIXTURE),
        &[
            "file python parsers/fixtures/sample.py #0 1-36",
            "  function function_definition parsers/fixtures/sample.py::trace #0 11-12",
            "  container class_definition parsers/fixtures/sample.py::Rect #1 15-31",
            "    function function_definition parsers/fixtures/sample.py::Rect::__init__ #0 20-22",
            "    function function_definition parsers/fixtures/sample.py::Rect::area #1 24-28",
            // A def inside a def, two levels down from the file.
            "      function function_definition parsers/fixtures/sample.py::Rect::area::scale #0 25-26",
            // A class inside a class.
            "    container class_definition parsers/fixtures/sample.py::Rect::Kind #2 30-31",
            // `@trace` wraps the def in a `decorated_definition`. That wrapper is
            // spliced through rather than promoted, so the function appears once
            // — and its span is the `def`, matching how a Rust doc comment sits
            // outside the declaration it documents.
            "  function function_definition parsers/fixtures/sample.py::main_entry #2 35-36",
        ],
    );
}

/// The Python twin of the Rust negative: module- and class-level assignments
/// are bindings, and a decorated def must not be twinned.
#[test]
fn python_fixture_invents_nothing() {
    let tree = tree(PYTHON_PATH, PYTHON_FIXTURE);
    let subkinds = subkinds(&tree);

    for refused in [
        "decorated_definition",
        "expression_statement",
        "assignment",
        "block",
    ] {
        assert!(
            !subkinds.contains(&refused),
            "{refused} must not become an element; got {subkinds:?}"
        );
    }
    assert!(
        tree.find("parsers/fixtures/sample.py::MAX_SIDES").is_none(),
        "a module-level binding is not a declaration"
    );
    assert_eq!(
        tree.iter()
            .filter(|element| element.name == "main_entry")
            .count(),
        1,
        "a decorated def must appear exactly once"
    );
}

#[test]
fn typescript_fixture_yields_the_expected_tree() {
    expect(
        &tree(TYPESCRIPT_PATH, TYPESCRIPT_FIXTURE),
        &[
            "file typescript parsers/fixtures/sample.ts #0 1-48",
            "  function function_declaration parsers/fixtures/sample.ts::top #0 3-8",
            "    function function_declaration parsers/fixtures/sample.ts::top::nested #0 4-6",
            "  container class_declaration parsers/fixtures/sample.ts::Service #1 10-14",
            "    function method_definition parsers/fixtures/sample.ts::Service::run #0 11-11",
            "    function public_field_definition parsers/fixtures/sample.ts::Service::field #1 13-13",
            "  container abstract_class_declaration parsers/fixtures/sample.ts::AbstractStore #2 16-18",
            "    function abstract_method_signature parsers/fixtures/sample.ts::AbstractStore::load #0 17-17",
            "  container interface_declaration parsers/fixtures/sample.ts::Runner #3 20-22",
            "    function method_signature parsers/fixtures/sample.ts::Runner::run #0 21-21",
            "  container enum_declaration parsers/fixtures/sample.ts::Mode #4 24-26",
            "  container type_alias_declaration parsers/fixtures/sample.ts::Result #5 28-28",
            "  function function_signature parsers/fixtures/sample.ts::declared #6 30-30",
            "  container internal_module parsers/fixtures/sample.ts::Tools #7 32-38",
            "    function function_declaration parsers/fixtures/sample.ts::Tools::inside #0 33-33",
            "    container interface_declaration parsers/fixtures/sample.ts::Tools::Nested #1 35-37",
            "      function method_signature parsers/fixtures/sample.ts::Tools::Nested::call #0 36-36",
            "  function variable_declarator parsers/fixtures/sample.ts::plain #8 40-40",
            "  function variable_declarator parsers/fixtures/sample.ts::assigned #9 41-41",
            "  function variable_declarator parsers/fixtures/sample.ts::exported #10 42-42",
            "  function variable_declarator parsers/fixtures/sample.ts::asyncTask #11 43-43",
            "  function variable_declarator parsers/fixtures/sample.ts::generated #12 44-46",
        ],
    );
}

#[test]
fn tsx_fixture_yields_the_expected_tree() {
    let tree = tree(TSX_PATH, TSX_FIXTURE);
    assert!(!tree.has_error, "the JSX-heavy fixture must parse cleanly");
    expect(
        &tree,
        &[
            "file tsx parsers/fixtures/sample.tsx #0 1-16",
            "  container interface_declaration parsers/fixtures/sample.tsx::CardProps #0 3-6",
            "  function function_declaration parsers/fixtures/sample.tsx::Card #1 8-16",
            "    function variable_declarator parsers/fixtures/sample.tsx::Card::local #0 9-9",
        ],
    );
}

#[test]
fn typescript_fixtures_invent_nothing_and_never_emit_blank_identity() {
    let typescript = tree(TYPESCRIPT_PATH, TYPESCRIPT_FIXTURE);
    let subkinds = subkinds(&typescript);
    for refused in [
        "import_statement",
        "export_statement",
        "lexical_declaration",
        "class_heritage",
    ] {
        assert!(
            !subkinds.contains(&refused),
            "{refused} must not become an element; got {subkinds:?}"
        );
    }
    for refused in ["x", "cfg"] {
        assert!(
            typescript
                .find(&format!("{TYPESCRIPT_PATH}::{refused}"))
                .is_none(),
            "non-function binding {refused} must not become an element"
        );
    }

    let tsx = tree(TSX_PATH, TSX_FIXTURE);
    for element in typescript.iter().chain(tsx.iter()) {
        assert!(
            !element.name.trim().is_empty(),
            "{} has an empty name",
            element.subkind
        );
        assert!(
            !element.address.trim().is_empty()
                && !element.address.contains("::<anonymous>")
                && !element.address.contains("::::"),
            "{} has an invalid address {:?}",
            element.subkind,
            element.address
        );
    }
}

#[test]
fn markdown_fixture_yields_the_expected_tree() {
    expect(
        &tree(MARKDOWN_PATH, MARKDOWN_FIXTURE),
        &[
            "file markdown parsers/fixtures/sample.md #0 1-30",
            // A section's span runs to the line before the next heading of
            // equal-or-shallower level (L9), so a parent's span covers its
            // children's.
            "  section atx_heading parsers/fixtures/sample.md::Main Title #0 1-30",
            "    section atx_heading parsers/fixtures/sample.md::Main Title::Section One #0 5-12",
            "      section atx_heading parsers/fixtures/sample.md::Main Title::Section One::Subsection 1.1 #0 9-12",
            "    section atx_heading parsers/fixtures/sample.md::Main Title::Section Two #1 13-21",
            "    section atx_heading parsers/fixtures/sample.md::Main Title::Section Three #2 22-30",
            "      section atx_heading parsers/fixtures/sample.md::Main Title::Section Three::Deep One #0 24-30",
            "        section atx_heading parsers/fixtures/sample.md::Main Title::Section Three::Deep One::Deeper Two #0 26-30",
            "          section atx_heading parsers/fixtures/sample.md::Main Title::Section Three::Deep One::Deeper Two::Deepest Three #0 28-30",
        ],
    );
}

/// POC learning L9 — the concrete argument for parsing markdown rather than
/// grepping it. `grep '^#'` finds six heading-looking lines in this fixture;
/// one of them is a shell comment inside a fenced code block.
#[test]
fn fenced_code_comments_are_not_headings() {
    let heading_shaped_lines = MARKDOWN_FIXTURE
        .lines()
        .filter(|line| line.starts_with('#'))
        .count();
    assert_eq!(heading_shaped_lines, 9, "the fixture must contain the trap");

    let tree = tree(MARKDOWN_PATH, MARKDOWN_FIXTURE);
    assert_eq!(
        tree.len() - 1,
        8,
        "only the eight real headings are sections"
    );
    assert!(
        !tree
            .iter()
            .any(|element| element.name.contains("shell comment")),
        "a fenced code comment leaked into the element tree"
    );
}

/// Element text is what a `Summarizer` reads, so a section must carry its whole
/// body — not just its heading line.
#[test]
fn section_text_carries_the_body_not_just_the_heading() {
    let tree = tree(MARKDOWN_PATH, MARKDOWN_FIXTURE);
    let section_one = tree
        .find("parsers/fixtures/sample.md::Main Title::Section One")
        .expect("fixture has Section One");

    assert!(section_one.raw_text.starts_with("## Section One"));
    assert!(section_one.raw_text.contains("Body of section one."));
    assert!(section_one.raw_text.contains("### Subsection 1.1"));
    assert!(!section_one.raw_text.contains("## Section Two"));
}

/// `(address, start_line)` — an address alone is not unique (a struct and its
/// `impl` share one), and this is the map that proves the hash story, so it must
/// not silently collapse two elements into one.
fn hashes(tree: &ElementTree) -> BTreeMap<(String, u32), String> {
    let mut map = BTreeMap::new();
    for element in tree.iter() {
        let previous = map.insert(
            (element.address.clone(), element.span.start_line),
            element.raw_hash().to_string(),
        );
        assert!(
            previous.is_none(),
            "two elements at {} line {}",
            element.address,
            element.span.start_line
        );
    }
    map
}

#[test]
fn the_same_bytes_always_produce_the_same_hashes() {
    let once = tree(RUST_PATH, RUST_FIXTURE);
    let twice = tree(RUST_PATH, RUST_FIXTURE);

    assert_eq!(hashes(&once), hashes(&twice));
    assert_eq!(once, twice, "the whole tree is a deterministic value");

    // And the path is not part of the element hash — only the text is. Two
    // copies of one file differ by address, never by dirtiness.
    let elsewhere = tree("vendor/copy.rs", RUST_FIXTURE);
    assert_eq!(once.root.raw_hash(), elsewhere.root.raw_hash());
    assert_eq!(once.blob, elsewhere.blob);
}

/// The dirtiness key earning its name: a one-character edit must re-hash the
/// elements that CONTAIN the edit, and nothing else. If it re-hashed siblings,
/// every commit would re-embed the whole file.
#[test]
fn a_one_character_edit_rehashes_only_the_elements_containing_it() {
    let before = tree(RUST_PATH, RUST_FIXTURE);

    let edited = RUST_FIXTURE.replace(
        "self.width * self.height",
        "self.width + self.height", // one character, inside `Rect::area`
    );
    assert_ne!(edited, RUST_FIXTURE, "the edit must actually apply");
    let after = tree(RUST_PATH, &edited);

    let before_hashes = hashes(&before);
    let after_hashes = hashes(&after);
    assert_eq!(
        before_hashes.keys().collect::<Vec<_>>(),
        after_hashes.keys().collect::<Vec<_>>(),
        "an edit inside a body must not move any element's address or span"
    );

    let changed: Vec<&(String, u32)> = before_hashes
        .iter()
        .filter(|(key, hash)| after_hashes.get(*key) != Some(*hash))
        .map(|(key, _)| key)
        .collect();

    assert_eq!(
        changed,
        vec![
            // The file, the module, the impl block, the method: the containment
            // chain of the edited line, and only that chain.
            &("parsers/fixtures/sample.rs".to_string(), 1),
            &("parsers/fixtures/sample.rs::geometry".to_string(), 3),
            &("parsers/fixtures/sample.rs::geometry::Rect".to_string(), 10),
            &(
                "parsers/fixtures/sample.rs::geometry::Rect::area".to_string(),
                17
            ),
        ],
        "only the containment chain of the edit may re-hash"
    );

    // Named explicitly, because "the sibling did not change" is the property
    // that makes incremental indexing worth having.
    for untouched in [
        ("parsers/fixtures/sample.rs::geometry::Rect".to_string(), 5),
        (
            "parsers/fixtures/sample.rs::geometry::Rect::new".to_string(),
            11,
        ),
        ("parsers/fixtures/sample.rs::main_entry".to_string(), 34),
    ] {
        assert_eq!(
            before_hashes[&untouched], after_hashes[&untouched],
            "{untouched:?} does not contain the edit and must keep its hash"
        );
    }
}

/// The file element is the whole file, so its hash is the file's content key —
/// one comparison answers "did this file change at all?".
#[test]
fn the_file_element_hashes_the_whole_file() {
    let tree = tree(RUST_PATH, RUST_FIXTURE);
    assert_eq!(tree.root.kind, ElementKind::File);
    assert_eq!(tree.root.raw_text, RUST_FIXTURE);
    assert_eq!(tree.root.raw_hash(), tree.blob.as_str());
}
