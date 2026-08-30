//! Exemplar: the integration tier — the element tree, against a real Postgres.
//!
//! Runs against the dockerized Postgres from `compose.yaml` — there is no
//! in-memory store to run against instead, and that is deliberate (workshop 001
//! refuses a repository trait over sqlx).
//!
//! Every test here takes a THROWAWAY database, migrates it, and drops it. It
//! used to work in the long-lived `flowspace3` database with a unique blob key
//! per test, which was adequate isolation and a genuine hazard: running the
//! suite applied whatever migrations were in the working tree to the database
//! every other worker shares, so an unpushed `0003` left every sibling's
//! `cargo test` failing with `VersionMissing(3)` against a tree that does not
//! contain it. Migrations under development belong in a database that is
//! deleted thirty milliseconds later.
//!
//! If docker is not running these tests FAIL rather than skipping, and name the
//! exact command. A silently-skipped integration test is how a store regression
//! reaches main.

mod support;

use std::collections::BTreeSet;

use fs3_core::{Element, ElementKind, Span};
use fs3_store::{
    ElementTreeWrite, StoreError, element_tree_inconsistencies, get_elements, upsert_element_tree,
};
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

/// A file with a container that has two functions in it — the smallest tree
/// with a grandchild, which is the shape a flat table could not hold.
fn tree(path: &str) -> Element {
    let first = Element::new(
        ElementKind::Function,
        "function_item",
        "new",
        format!("{path}::Rect::new"),
        Span::new(4, 6),
        "pub fn new() -> Self { Self }",
    )
    .with_sibling_order(0);
    let second = Element::new(
        ElementKind::Function,
        "function_item",
        "area",
        format!("{path}::Rect::area"),
        Span::new(8, 10),
        "pub fn area(&self) -> u32 { 0 }",
    )
    .with_sibling_order(1);
    let container = Element::new(
        ElementKind::Container,
        "impl_item",
        "Rect",
        format!("{path}::Rect"),
        Span::new(3, 11),
        "impl Rect { /* .. */ }",
    )
    .with_children(vec![first, second]);

    Element::new(
        ElementKind::File,
        "rust",
        "sample.rs",
        path,
        Span::new(1, 12),
        "// sample.rs\n",
    )
    .with_children(vec![container])
}

/// Connect, migrate, round-trip a whole tree. The exemplar in one test.
#[tokio::test]
async fn migration_applies_and_an_element_tree_round_trips() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();

    let written = tree("src/geometry.rs");
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &written, |_| true)
        .await
        .expect("insert");

    let read_back = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("select")
        .tree
        .expect("the blob was just written, so it has been parsed");
    assert_eq!(
        read_back, written,
        "the tree must survive the trip — nesting, sibling order and all"
    );
    assert_eq!(
        read_back.children[0].children[1].raw_hash(),
        written.children[0].children[1].raw_hash(),
        "the dirtiness key is derived from the text, so it survives a trip \
         through a table whose copy of it is never read back"
    );

    // The upsert is keyed by content, so re-scanning is idempotent — and
    // because a blob is the hash of the bytes, the key set cannot change
    // between runs, which is why no reconciling delete is needed.
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &written, |_| true)
        .await
        .expect("re-insert");
    let again = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("select")
        .tree
        .expect("still parsed");
    assert_eq!(again, written, "a second scan must not duplicate anything");

    database.destroy(pool).await;
}

#[tokio::test]
async fn concurrent_paths_for_one_blob_converge_without_error() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    let left = tree("src/a.rs");
    let right = tree("src/b.rs");

    let (left_result, right_result) = tokio::join!(
        upsert_element_tree(&pool, &blob, PARSER_VERSION, &left, |_| true),
        upsert_element_tree(&pool, &blob, PARSER_VERSION, &right, |_| true),
    );
    let outcomes = [
        left_result.expect("left writer converges"),
        right_result.expect("right writer converges"),
    ];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ElementTreeWrite::Stored))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ElementTreeWrite::Reused { .. }))
            .count(),
        1,
        "the losing writer reuses the winner instead of surfacing a constraint error"
    );

    let roots: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM elements
          WHERE blob_sha = $1 AND parser_version = $2
            AND parent_id IS NULL AND kind = 'file'",
    )
    .bind(blob.as_str())
    .bind(PARSER_VERSION)
    .fetch_one(&pool)
    .await
    .expect("count roots");
    assert_eq!(roots, 1, "the database invariant permits one file root");

    let read = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("read converged tree");
    assert!(read.tree.is_some());
    assert_eq!(read.inconsistency, None);

    database.destroy(pool).await;
}

#[tokio::test]
async fn duplicate_root_reader_reports_and_serves_lowest_id_survivor() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    sqlx::query("DROP INDEX elements_one_file_root_per_blob_parser_idx")
        .execute(&pool)
        .await
        .expect("simulate a pre-0020 database");
    let blob = unique_blob();
    let first = tree("src/first.rs");
    let second = tree("src/second.rs");

    assert_eq!(
        upsert_element_tree(&pool, &blob, PARSER_VERSION, &first, |_| true)
            .await
            .expect("first pre-fix write"),
        ElementTreeWrite::Stored
    );
    assert_eq!(
        upsert_element_tree(&pool, &blob, PARSER_VERSION, &second, |_| true)
            .await
            .expect("second pre-fix write"),
        ElementTreeWrite::Stored
    );

    let read = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("dirty data is reported, not failed");
    assert_eq!(
        read.tree.as_ref().map(|tree| tree.address.as_str()),
        Some("src/first.rs")
    );
    let issue = read.inconsistency.expect("duplicate roots are visible");
    assert_eq!(issue.paths, ["src/first.rs", "src/second.rs"]);

    let all = element_tree_inconsistencies(&pool)
        .await
        .expect("status integrity query completes");
    assert_eq!(all, [issue]);

    database.destroy(pool).await;
}

/// Two elements at one address are two rows.
///
/// The scanner emits `struct Rect` and `impl Rect` as separate elements sharing
/// an address — `(address, span_start)` is what identifies a node. A unique key
/// on address alone made the second write silently overwrite the first, which
/// is a data-loss bug that looks exactly like a successful scan.
#[tokio::test]
async fn two_elements_sharing_an_address_are_two_rows() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();

    let declaration = Element::new(
        ElementKind::Container,
        "struct_item",
        "Rect",
        "src/geometry.rs::Rect",
        Span::new(3, 6),
        "pub struct Rect { w: u32, h: u32 }",
    )
    .with_sibling_order(0);
    let implementation = Element::new(
        ElementKind::Container,
        "impl_item",
        "Rect",
        "src/geometry.rs::Rect",
        Span::new(8, 12),
        "impl Rect { /* .. */ }",
    )
    .with_sibling_order(1);
    let file = Element::new(
        ElementKind::File,
        "rust",
        "geometry.rs",
        "src/geometry.rs",
        Span::new(1, 13),
        "// geometry.rs\n",
    )
    .with_children(vec![declaration, implementation]);

    upsert_element_tree(&pool, &blob, PARSER_VERSION, &file, |_| true)
        .await
        .expect("insert");

    let read_back = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("select")
        .tree
        .expect("just written");
    assert_eq!(
        read_back, file,
        "both elements at that address must survive"
    );
    assert_eq!(
        read_back.children.len(),
        2,
        "the struct and its impl are two nodes, not one"
    );
    assert_eq!(
        read_back.children[0].subkind, "struct_item",
        "and the first one must not have been overwritten by the second"
    );

    database.destroy(pool).await;
}

/// A blob nobody has parsed reads back as `None`, not as an empty tree. That
/// distinction IS the scan flow's skip: `Some` means the work is already done.
#[tokio::test]
async fn an_unparsed_blob_is_none_rather_than_empty() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();

    assert!(
        get_elements(&pool, &blob, PARSER_VERSION)
            .await
            .expect("select")
            .tree
            .is_none()
    );

    // Same blob, different parser: also unparsed. The key is the pair, which is
    // what lets a parser bump re-mint elements without touching enrichment.
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &tree("src/a.rs"), |_| true)
        .await
        .expect("insert");
    assert!(
        get_elements(&pool, &blob, "test-parser@2")
            .await
            .expect("select")
            .tree
            .is_none(),
        "a parser_version bump must not read the previous parser's rows"
    );

    database.destroy(pool).await;
}

/// Every kind the tree model has must be a legal row. `file` most of all: 0001
/// predated the file element existing, and 0002 is what made it legal.
#[tokio::test]
async fn every_tree_model_kind_is_a_legal_row() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();

    let children: Vec<Element> = [
        ElementKind::Container,
        ElementKind::Function,
        ElementKind::Section,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| {
        let line = index as u32 + 2;
        Element::new(
            kind,
            "probe",
            kind.as_str(),
            format!("kind-probe.rs::{kind}"),
            Span::new(line, line),
            "probe",
        )
        .with_sibling_order(index as u32)
    })
    .collect();

    let root = Element::new(
        ElementKind::File,
        "rust",
        "kind-probe.rs",
        "kind-probe.rs",
        Span::new(1, 5),
        "probe file",
    )
    .with_children(children);

    upsert_element_tree(&pool, &blob, PARSER_VERSION, &root, |_| false)
        .await
        .unwrap_or_else(|error| panic!("every tree-model kind must be legal: {error}"));

    let kinds: Vec<ElementKind> = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("select")
        .tree
        .expect("just written")
        .iter()
        .map(|element| element.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            ElementKind::File,
            ElementKind::Container,
            ElementKind::Function,
            ElementKind::Section,
        ]
    );

    database.destroy(pool).await;
}

/// pgvector is a requirement of the image, not an optional extra (PRD req 4).
#[tokio::test]
async fn the_stack_really_has_pgvector() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let installed: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')")
            .fetch_one(&pool)
            .await
            .expect("extension probe should run");
    assert!(
        installed,
        "migration 0001 creates the vector extension; the image must provide it \
         (compose.yaml pins pgvector/pgvector)"
    );

    database.destroy(pool).await;
}

/// The CHECK constraints are part of the schema's contract, so prove they bite.
#[tokio::test]
async fn the_schema_refuses_an_impossible_span() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();

    let backwards = Element::new(
        ElementKind::File,
        "rust",
        "backwards.rs",
        "backwards.rs",
        Span::new(40, 10),
        "",
    );
    let error = upsert_element_tree(&pool, &blob, PARSER_VERSION, &backwards, |_| false)
        .await
        .expect_err("span_end < span_start must be refused by the database, not just by Rust");

    // `is_err()` alone passed for ANY failure — a dropped connection, a typo in
    // the SQL, a missing table. What this test claims is that the CHECK bit, so
    // it has to read the SQLSTATE the database sent.
    let StoreError::Query(sqlx::Error::Database(database_error)) = &error else {
        panic!("expected a database error carrying a SQLSTATE, got: {error}");
    };
    assert_eq!(
        database_error.code().as_deref(),
        Some("23514"),
        "23514 is check_violation; another code means the row was refused for \
         the wrong reason: {error}"
    );
    assert_eq!(
        database_error.constraint(),
        Some("elements_span_ordered"),
        "the span CHECK must be the constraint that bit, not another one: {error}"
    );

    // The transaction rolled back, so the refusal left nothing behind.
    assert!(
        get_elements(&pool, &blob, PARSER_VERSION)
            .await
            .expect("select")
            .tree
            .is_none(),
        "a refused tree must not leave a partial write"
    );

    database.destroy(pool).await;
}

/// The isolation seed has to vary WITHIN a process: these tests run
/// concurrently under one `cargo test` binary, and the same seed feeds the
/// throwaway DATABASE names — which are DROPped. A repeat there is not
/// interference, it is one test deleting another's database mid-run.
#[test]
fn every_isolation_key_is_unique_within_a_process() {
    let keys: BTreeSet<String> = (0..1000)
        .map(|_| unique_blob().as_str().to_string())
        .collect();
    assert_eq!(
        keys.len(),
        1000,
        "a key derived from the pid alone repeats on every call"
    );

    let key = unique_blob();
    assert_eq!(key.as_str().len(), 40, "a blob key is a 40-char digest");
    assert!(
        key.as_str()
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "a blob key is lowercase hex: {}",
        key.as_str()
    );
}
