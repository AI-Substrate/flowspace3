use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use fs3_core::ddoc::{DDOC_GENERATED_BANNER, DdocSchemaFacts, EmbedBasis};
use fs3_core::{ElementKind, ElementTree};
use fs3_parsers::discovery::{
    DiscoverySettings, LanguageFamily, SkipReason, discover, discover_subtree,
};
use fs3_parsers::{is_ddoc_source, is_generated_sibling, scan_ddoc};

const PLAIN: &[u8] = include_bytes!("../fixtures/ddoc/plain.dd.json");
const DYNAMIC: &[u8] = include_bytes!("../fixtures/ddoc/dynamic.dd.json");
const NO_ROWS: &[u8] = include_bytes!("../fixtures/ddoc/no-rows.dd.json");
const REORDER_A: &[u8] = include_bytes!("../fixtures/ddoc/reorder-a.dd.json");
const REORDER_B: &[u8] = include_bytes!("../fixtures/ddoc/reorder-b.dd.json");

fn facts(section: &str, prose: &[&str], strings: &[&str]) -> DdocSchemaFacts {
    DdocSchemaFacts {
        schema: "fixture/schema".into(),
        prose_fields: BTreeMap::from([(
            section.into(),
            prose.iter().map(|field| (*field).to_owned()).collect(),
        )]),
        string_fields: BTreeMap::from([(
            section.into(),
            strings.iter().map(|field| (*field).to_owned()).collect(),
        )]),
        gate_terminal: BTreeSet::from(["shipped".into()]),
    }
}

fn rows(tree: &ElementTree) -> Vec<&fs3_core::Element> {
    tree.iter()
        .filter(|element| element.kind == ElementKind::Row)
        .collect()
}

#[test]
fn plain_section_rows_form_the_required_tree_and_metadata() {
    let schema = facts("acceptance_criteria", &["claim"], &["note"]);
    let tree = scan_ddoc(Path::new("docs/plain.dd.json"), PLAIN, Some(&schema)).unwrap();

    assert_eq!(tree.root.kind, ElementKind::File);
    assert_eq!(tree.root.subkind, "ddoc");
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].kind, ElementKind::Section);
    assert_eq!(tree.root.children[1].kind, ElementKind::Container);
    assert_eq!(tree.root.children[1].children.len(), 2);

    let found = rows(&tree);
    assert_eq!(found.len(), 2);
    let first = found[0];
    assert_eq!(
        first.address,
        "docs/plain.dd.json#acceptance_criteria/ac-0001"
    );
    assert_eq!(first.name, "ac-0001");
    assert_eq!(
        first.raw_text,
        concat!(
            "acceptance_criteria\n",
            "claim: Rows are the searchable unit\n",
            "context note: Keep the containing subject visible"
        )
    );
    let meta = first.ddoc.as_deref().expect("row metadata");
    assert_eq!(meta.schema, "builder/plan");
    assert_eq!(meta.trail, ["acceptance_criteria", "ac-0001"]);
    assert_eq!(meta.id_kind.as_deref(), Some("ac"));
    assert_eq!(meta.state.as_deref(), Some("shipped"));
    assert_eq!(meta.doc_title.as_deref(), Some("Parser contract"));
    assert!(
        meta.sweep_excluded,
        "sweep exclusion is metadata, not admission"
    );
    assert_eq!(meta.embed_basis, EmbedBasis::SchemaDeclared);
    assert!(meta.rels.is_empty());
    assert!(meta.findings.is_empty());

    let unknown = found[1].ddoc.as_deref().unwrap();
    assert_eq!(unknown.id_kind.as_deref(), Some("zz"));
}

#[test]
fn dynamic_key_maps_recurse_without_a_depth_limit_and_keep_every_key() {
    let schema = facts("done_when", &["text"], &[]);
    let tree = scan_ddoc(Path::new("docs/tasks.dd.json"), DYNAMIC, Some(&schema)).unwrap();
    let found = rows(&tree);

    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].address,
        "docs/tasks.dd.json#done_when/tk-0001/assertions/required/dw-0002"
    );
    let meta = found[0].ddoc.as_deref().unwrap();
    assert_eq!(
        meta.trail,
        ["done_when", "tk-0001", "assertions", "required", "dw-0002"]
    );
    assert!(
        found[0]
            .raw_text
            .starts_with("done_when / tk-0001 / assertions / required\n")
    );
}

#[test]
fn a_section_without_ids_is_one_section_chunk() {
    let tree = scan_ddoc(Path::new("docs/reference.dd.json"), NO_ROWS, None).unwrap();

    assert_eq!(tree.len(), 2, "file root plus one section chunk");
    let section = &tree.root.children[0];
    assert_eq!(section.kind, ElementKind::Section);
    assert_eq!(section.address, "docs/reference.dd.json#rationale");
    assert!(section.children.is_empty());
    assert!(
        section
            .raw_text
            .contains("A section with no ids remains retrievable.")
    );
}

#[test]
fn missing_schema_facts_use_the_explicit_fallback() {
    let tree = scan_ddoc(Path::new("docs/plain.dd.json"), PLAIN, None).unwrap();
    let first = rows(&tree)[0];
    let meta = first.ddoc.as_deref().unwrap();

    assert_eq!(meta.embed_basis, EmbedBasis::Fallback);
    assert!(
        first
            .raw_text
            .contains("claim: Rows are the searchable unit")
    );
    assert!(
        first
            .raw_text
            .contains("note: Keep the containing subject visible")
    );
    assert!(!first.raw_text.contains("ac-0001"), "raw ids are metadata");
    assert!(
        !first.raw_text.contains("shipped"),
        "stored state is metadata"
    );
}

#[test]
fn reordering_rows_preserves_addresses_and_raw_hashes() {
    let schema = facts("tasks", &["title"], &["note"]);
    let logical_path = Path::new("docs/tasks.dd.json");
    let before = scan_ddoc(logical_path, REORDER_A, Some(&schema)).unwrap();
    let after = scan_ddoc(logical_path, REORDER_B, Some(&schema)).unwrap();

    let identity = |tree: &ElementTree| {
        rows(tree)
            .into_iter()
            .map(|row| (row.address.clone(), row.raw_hash().to_owned()))
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(identity(&before), identity(&after));
}

#[test]
fn stored_custom_state_is_recorded_without_parser_judgment() {
    let schema = facts("acceptance_criteria", &["claim"], &["note"]);
    let tree = scan_ddoc(Path::new("docs/plain.dd.json"), PLAIN, Some(&schema)).unwrap();
    let meta = rows(&tree)[0].ddoc.as_deref().unwrap();

    assert_eq!(meta.state.as_deref(), Some("shipped"));
    assert_eq!(meta.gate_terminal, None);
    assert_eq!(meta.derived_state, None);
}

#[test]
fn source_and_generated_detection_are_exact_and_banner_aware() {
    assert!(is_ddoc_source(Path::new("plan.dd.json")));
    assert!(!is_ddoc_source(Path::new("plan.json")));
    assert!(!is_ddoc_source(Path::new("plan.DD.JSON")));

    assert!(is_generated_sibling(Path::new("plan.dd.md"), b"anything"));
    assert!(is_generated_sibling(
        Path::new("renamed.md"),
        format!("{DDOC_GENERATED_BANNER}\r\n# Generated").as_bytes(),
    ));
    assert!(!is_generated_sibling(Path::new("notes.md"), b"# Authored"));
}

fn discovery_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/ddoc/discovery")
}

fn assert_discovery_contract(result: &fs3_parsers::discovery::Discovery) {
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].path, "nested/source.dd.json");
    assert_eq!(result.files[0].family, LanguageFamily::Config);

    let skipped = result
        .skipped
        .iter()
        .map(|entry| (entry.path.as_str(), entry.reason))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        skipped.get("nested/ordinary.json"),
        Some(&SkipReason::ConfigFormat)
    );
    assert_eq!(
        skipped.get("nested/source.dd.md"),
        Some(&SkipReason::ConfigFormat)
    );
}

#[test]
fn discovery_admits_only_the_ddoc_source() {
    let result = discover(&discovery_fixture(), &DiscoverySettings::default()).unwrap();
    assert_discovery_contract(&result);
}

#[test]
fn watcher_subtree_discovery_uses_the_same_ddoc_admission() {
    let root = discovery_fixture();
    let result = discover_subtree(&root, &root.join("nested"), &DiscoverySettings::default())
        .unwrap()
        .expect("nested directory is reachable");
    assert_discovery_contract(&result);
}
