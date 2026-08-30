//! Ddoc persistence, filters, and inverse-index contracts against real Postgres.

mod support;

use std::path::Path;

use fs3_core::{
    BlobRef, DdocMeta, DdocRel, DerivedState, Element, ElementKind, EmbedBasis, Embedder,
    RepoIdentity, Span,
};
use fs3_store::{
    AnchorScope, DdocCitation, DdocFileRef, EMBEDDING_DIMENSIONS, MIGRATOR, NewEmbedding, PgPool,
    SearchFilters, SearchHit, SourceKind, anchor_has_vectors, get_elements, put_embeddings,
    register_worktree, replace_file_refs, rows_citing, rows_referencing, search_elements,
    sync_worktree_files, upsert_element_tree,
};
use fs3_testkit::fakes::FakeEmbedder;
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

const EMBEDDER: &str = "fake-embedder@ddoc-v1";

fn ddoc_meta(path: &str, id: &str, schema: &str, gate_terminal: Option<bool>) -> DdocMeta {
    let mut meta = DdocMeta::new(
        format!("{path}#criteria/{id}"),
        schema,
        vec!["criteria".to_string(), id.to_string()],
        EmbedBasis::SchemaDeclared,
    );
    meta.state = Some(
        if gate_terminal == Some(true) {
            "done"
        } else {
            "open"
        }
        .to_string(),
    );
    meta.gate_terminal = gate_terminal;
    meta.derived_state = gate_terminal.map(|complete| DerivedState {
        complete,
        incomplete: if complete {
            Vec::new()
        } else {
            vec![format!("check-{id}")]
        },
    });
    meta.doc_title = Some("Ddoc store contract".to_string());
    meta.rels = vec![DdocRel {
        rel: "implements".to_string(),
        target: "src/lib.rs".to_string(),
        kind: "file".to_string(),
        location: format!("$.criteria.{id}.source"),
    }];
    meta.findings = vec!["recorded finding".to_string()];
    meta
}

fn row(path: &str, id: &str, schema: &str, gate_terminal: Option<bool>, order: u32) -> Element {
    let address = format!("{path}#criteria/{id}");
    Element::new(
        ElementKind::Row,
        "criterion",
        id,
        &address,
        Span::new(order + 2, order + 2),
        format!("criterion {id}"),
    )
    .with_sibling_order(order)
    .with_ddoc(ddoc_meta(path, id, schema, gate_terminal))
}

fn ddoc_tree(path: &str, rows: Vec<Element>) -> Element {
    Element::new(
        ElementKind::File,
        "ddoc",
        path,
        path,
        Span::new(1, rows.len() as u32 + 1),
        format!("ddoc {path}"),
    )
    .with_children(rows)
}

async fn vector_for(text: &str) -> Vec<f32> {
    FakeEmbedder {
        dimensions: EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    }
    .embed(&[text.to_string()])
    .await
    .expect("the deterministic embedder should answer")
    .remove(0)
}

struct SearchFixture {
    database: FreshDatabase,
    pool: PgPool,
    rows: Vec<Element>,
}

impl SearchFixture {
    async fn create() -> Self {
        let database = FreshDatabase::create().await;
        let pool = database.migrated_pool().await;
        let path = "docs/plan.dd.json";
        let rows = vec![
            row(path, "ac-0001", "builder/plan", Some(false), 0),
            row(path, "tk-0002", "builder/plan", Some(true), 1),
            row(path, "zz-0003", "builder/other", None, 2),
        ];
        let tree = ddoc_tree(path, rows.clone());
        let blob = unique_blob();
        upsert_element_tree(&pool, &blob, PARSER_VERSION, &tree, |element| {
            element.kind == ElementKind::Row
        })
        .await
        .expect("store ddoc tree");

        let identity = RepoIdentity::from_path(Path::new("/srv/ddoc-store"));
        let worktree = register_worktree(&pool, &identity, "/srv/ddoc-store", None)
            .await
            .expect("register fixture worktree");
        sync_worktree_files(&pool, worktree, &[(path.to_string(), blob)])
            .await
            .expect("register fixture path");

        let mut owned_vectors = Vec::new();
        for row in &rows {
            owned_vectors.push((row.raw_hash().to_string(), vector_for(&row.raw_text).await));
        }
        let embeddings: Vec<NewEmbedding<'_>> = owned_vectors
            .iter()
            .map(|(hash, vector)| NewEmbedding {
                chunk_no: 0,
                source_hash: hash,
                source_kind: SourceKind::Raw,
                vector,
                truncated: false,
            })
            .collect();
        put_embeddings(&pool, EMBEDDER, &embeddings)
            .await
            .expect("store fixture vectors");

        Self {
            database,
            pool,
            rows,
        }
    }

    async fn search_hits(&self, filters: SearchFilters) -> Vec<SearchHit> {
        let query = vector_for("criterion").await;
        search_elements(&self.pool, EMBEDDER, &query, &filters)
            .await
            .expect("search ddoc fixture")
    }

    async fn search(&self, filters: SearchFilters) -> Vec<String> {
        self.search_hits(filters)
            .await
            .into_iter()
            .map(|hit| hit.similar.element.address)
            .collect()
    }

    async fn destroy(self) {
        self.database.destroy(self.pool).await;
    }
}

#[tokio::test]
async fn search_hit_carries_ddoc_payload_without_a_second_query() {
    let fixture = SearchFixture::create().await;
    let hits = fixture
        .search_hits(SearchFilters {
            id_kinds: Some(vec!["ac".to_string()]),
            ..SearchFilters::default()
        })
        .await;

    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0]
            .similar
            .element
            .ddoc
            .as_deref()
            .map(|meta| meta.address.as_str()),
        Some("docs/plan.dd.json#criteria/ac-0001")
    );
    fixture.destroy().await;
}

#[tokio::test]
async fn ddoc_meta_round_trips_byte_identically() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    let written = ddoc_tree(
        "docs/plan.dd.json",
        vec![row(
            "docs/plan.dd.json",
            "ac-0001",
            "builder/plan",
            Some(false),
            0,
        )],
    );

    upsert_element_tree(&pool, &blob, PARSER_VERSION, &written, |_| true)
        .await
        .expect("store ddoc tree");
    let read = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("read ddoc tree")
        .tree
        .expect("tree was stored");

    assert_eq!(read, written);
    database.destroy(pool).await;
}

#[tokio::test]
async fn code_element_round_trip_keeps_ddoc_null() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    let code = Element::new(
        ElementKind::File,
        "rust",
        "lib.rs",
        "src/lib.rs",
        Span::new(1, 1),
        "fn main() {}",
    );

    upsert_element_tree(&pool, &blob, PARSER_VERSION, &code, |_| true)
        .await
        .expect("store code element");
    let read = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("read code element")
        .tree
        .expect("tree was stored");
    let stored_null: bool = sqlx::query_scalar(
        "SELECT ddoc IS NULL FROM elements WHERE blob_sha = $1 AND parser_version = $2",
    )
    .bind(blob.as_str())
    .bind(PARSER_VERSION)
    .fetch_one(&pool)
    .await
    .expect("read raw ddoc column");

    assert_eq!(read, code);
    assert!(stored_null, "code metadata must remain SQL NULL");
    database.destroy(pool).await;
}

#[tokio::test]
async fn search_filter_id_kinds_selects_and_none_is_noop() {
    let fixture = SearchFixture::create().await;
    let all = fixture.search(SearchFilters::default()).await;
    let filtered = fixture
        .search(SearchFilters {
            id_kinds: Some(vec!["ac".to_string()]),
            ..SearchFilters::default()
        })
        .await;

    assert_eq!(all.len(), fixture.rows.len());
    assert_eq!(filtered, vec!["docs/plan.dd.json#criteria/ac-0001"]);
    fixture.destroy().await;
}

#[tokio::test]
async fn search_filter_gate_open_selects_known_rows_and_excludes_unknown() {
    let fixture = SearchFixture::create().await;
    let all = fixture.search(SearchFilters::default()).await;
    let open = fixture
        .search(SearchFilters {
            gate_open: Some(true),
            ..SearchFilters::default()
        })
        .await;
    let closed = fixture
        .search(SearchFilters {
            gate_open: Some(false),
            ..SearchFilters::default()
        })
        .await;

    assert_eq!(all.len(), fixture.rows.len());
    assert_eq!(open, vec!["docs/plan.dd.json#criteria/ac-0001"]);
    assert_eq!(closed, vec!["docs/plan.dd.json#criteria/tk-0002"]);
    assert!(
        !open
            .iter()
            .chain(&closed)
            .any(|address| address.ends_with("zz-0003"))
    );
    fixture.destroy().await;
}

#[tokio::test]
async fn search_filter_gate_open_prefers_derived_state_over_stored_state() {
    let fixture = SearchFixture::create().await;
    sqlx::query(
        "UPDATE elements
            SET ddoc = jsonb_set(
                    jsonb_set(ddoc, '{state}', '\"checked\"'::jsonb),
                    '{derived_state}',
                    '{\"complete\":false,\"incomplete\":[\"dw-0001\"]}'::jsonb)
          WHERE address = 'docs/plan.dd.json#criteria/tk-0002'",
    )
    .execute(&fixture.pool)
    .await
    .expect("seed stored/derived disagreement");

    let open = fixture
        .search(SearchFilters {
            id_kinds: Some(vec!["tk".to_string()]),
            gate_open: Some(true),
            ..SearchFilters::default()
        })
        .await;
    let closed = fixture
        .search(SearchFilters {
            id_kinds: Some(vec!["tk".to_string()]),
            gate_open: Some(false),
            ..SearchFilters::default()
        })
        .await;

    assert_eq!(open, vec!["docs/plan.dd.json#criteria/tk-0002"]);
    assert!(closed.is_empty());
    fixture.destroy().await;
}

#[tokio::test]
async fn search_filter_ddoc_schema_selects_and_none_is_noop() {
    let fixture = SearchFixture::create().await;
    let all = fixture.search(SearchFilters::default()).await;
    let filtered = fixture
        .search(SearchFilters {
            ddoc_schema: Some("builder/other".to_string()),
            ..SearchFilters::default()
        })
        .await;

    assert_eq!(all.len(), fixture.rows.len());
    assert_eq!(filtered, vec!["docs/plan.dd.json#criteria/zz-0003"]);
    fixture.destroy().await;
}

#[tokio::test]
async fn ddoc_content_predicates_cannot_change_anchor_existence() {
    let fixture = SearchFixture::create().await;
    let filtered = fixture
        .search(SearchFilters {
            ddoc_schema: Some("schema/that-is-not-indexed".to_string()),
            ..SearchFilters::default()
        })
        .await;
    assert!(
        filtered.is_empty(),
        "the ddoc content filter matches nothing"
    );

    let exists = anchor_has_vectors(
        &fixture.pool,
        EMBEDDER,
        &AnchorScope {
            repo: None,
            worktree: None,
            path: None,
        },
    )
    .await
    .expect("the scope probe should run");
    assert!(
        exists,
        "an empty ddoc content filter must not erase an indexed anchor"
    );
    fixture.destroy().await;
}

async fn stored_ddoc(
    pool: &PgPool,
    blob: &BlobRef,
    parser_version: &str,
    path: &str,
    ids: &[&str],
) {
    let rows = ids
        .iter()
        .enumerate()
        .map(|(order, id)| row(path, id, "builder/plan", Some(false), order as u32))
        .collect();
    upsert_element_tree(pool, blob, parser_version, &ddoc_tree(path, rows), |_| true)
        .await
        .expect("store inverse-index source rows");
}

fn file_ref(address: &str, target: &str, location: &str) -> DdocFileRef {
    DdocFileRef {
        element_id: 0,
        address: address.to_string(),
        path: target.to_string(),
        rel: "implements".to_string(),
        location: location.to_string(),
    }
}

fn citing_row(
    path: &str,
    id: &str,
    target: &str,
    rel: &str,
    location: &str,
    order: u32,
) -> Element {
    let mut element = row(path, id, "builder/tasks", Some(false), order);
    element
        .ddoc
        .as_deref_mut()
        .expect("a ddoc row carries metadata")
        .rels = vec![DdocRel {
        rel: rel.to_string(),
        target: target.to_string(),
        kind: "document".to_string(),
        location: location.to_string(),
    }];
    element
}

#[tokio::test]
async fn rows_citing_without_matching_relations_is_empty() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    stored_ddoc(
        &pool,
        &blob,
        PARSER_VERSION,
        "docs/tasks.dd.json",
        &["tk-0001"],
    )
    .await;

    let rows = rows_citing(
        &pool,
        None,
        "docs/plan.dd.json#criteria/ac-dead",
        PARSER_VERSION,
        20,
    )
    .await
    .expect("an uncited qualified address is a successful empty result");
    assert!(rows.is_empty());
    database.destroy(pool).await;
}

#[tokio::test]
async fn rows_citing_returns_two_relations_in_stable_order() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    let path = "docs/tasks.dd.json";
    let target = "docs/plan.dd.json#acceptance_criteria/ac-0001";
    let rows = vec![
        citing_row(
            path,
            "tk-0002",
            target,
            "derives",
            "$.tasks[1].criterion",
            0,
        ),
        citing_row(
            path,
            "tk-0001",
            target,
            "satisfies",
            "$.tasks[0].criterion",
            1,
        ),
        citing_row(
            path,
            "tk-0003",
            "docs/plan.dd.json#acceptance_criteria/ac-other",
            "satisfies",
            "$.tasks[2].criterion",
            2,
        ),
    ];
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &ddoc_tree(path, rows), |_| {
        true
    })
    .await
    .expect("store citing rows");
    upsert_element_tree(
        &pool,
        &blob,
        "test-parser@other",
        &ddoc_tree(
            path,
            vec![citing_row(
                path,
                "tk-dead",
                target,
                "satisfies",
                "$.tasks[3].criterion",
                0,
            )],
        ),
        |_| true,
    )
    .await
    .expect("store retained-generation citing row");

    let identity = RepoIdentity::from_path(Path::new("/srv/citation-index"));
    let worktree = register_worktree(&pool, &identity, "/srv/citation-index", None)
        .await
        .expect("register citation fixture");
    sync_worktree_files(&pool, worktree, &[(path.to_string(), blob)])
        .await
        .expect("register citation source path");

    let citations = rows_citing(&pool, Some(identity.key()), target, PARSER_VERSION, 20)
        .await
        .expect("read citing rows");
    assert_eq!(
        citations
            .iter()
            .map(|citation| (
                citation.address.as_str(),
                citation.rel.as_str(),
                citation.location.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "docs/tasks.dd.json#criteria/tk-0001",
                "satisfies",
                "$.tasks[0].criterion",
            ),
            (
                "docs/tasks.dd.json#criteria/tk-0002",
                "derives",
                "$.tasks[1].criterion",
            ),
        ]
    );
    assert!(
        citations
            .iter()
            .all(|citation: &DdocCitation| citation.element_id > 0)
    );
    database.destroy(pool).await;
}

#[tokio::test]
async fn rows_referencing_without_file_edges_is_empty() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    stored_ddoc(
        &pool,
        &blob,
        PARSER_VERSION,
        "docs/plan.dd.json",
        &["ac-0001"],
    )
    .await;

    let rows = rows_referencing(&pool, None, "src/lib.rs", PARSER_VERSION, 20)
        .await
        .expect("an edge-free corpus is valid");
    assert!(rows.is_empty());
    database.destroy(pool).await;
}

#[tokio::test]
async fn rows_referencing_returns_seeded_rows_in_stable_order() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    let path = "docs/plan.dd.json";
    stored_ddoc(&pool, &blob, PARSER_VERSION, path, &["ac-0002", "ac-0001"]).await;
    let other_parser_version = "test-parser@other";
    stored_ddoc(&pool, &blob, other_parser_version, path, &["ac-dead"]).await;

    let identity = RepoIdentity::from_path(Path::new("/srv/inverse-index"));
    let worktree = register_worktree(&pool, &identity, "/srv/inverse-index", None)
        .await
        .expect("register inverse-index fixture");
    sync_worktree_files(&pool, worktree, &[(path.to_string(), blob.clone())])
        .await
        .expect("register inverse-index source path");

    let refs = vec![
        file_ref(
            "docs/plan.dd.json#criteria/ac-0002",
            "src/lib.rs",
            "$.criteria[1].source",
        ),
        file_ref(
            "docs/plan.dd.json#criteria/ac-0001",
            "src/lib.rs",
            "$.criteria[0].source",
        ),
    ];
    let outcome = replace_file_refs(&pool, &blob, PARSER_VERSION, &refs)
        .await
        .expect("replace file refs");
    assert_eq!(outcome.attached, 2);
    assert!(outcome.unattached.is_empty());
    let other_outcome = replace_file_refs(
        &pool,
        &blob,
        other_parser_version,
        &[file_ref(
            "docs/plan.dd.json#criteria/ac-dead",
            "src/lib.rs",
            "$.criteria[0].source",
        )],
    )
    .await
    .expect("replace retained-generation file refs");
    assert_eq!(other_outcome.attached, 1);

    let rows = rows_referencing(
        &pool,
        Some(identity.key()),
        "src/lib.rs",
        PARSER_VERSION,
        20,
    )
    .await
    .expect("read inverse index");
    assert_eq!(
        rows.iter()
            .map(|row| row.address.as_str())
            .collect::<Vec<_>>(),
        vec![
            "docs/plan.dd.json#criteria/ac-0001",
            "docs/plan.dd.json#criteria/ac-0002",
        ]
    );
    assert!(
        rows_referencing(
            &pool,
            Some("git:example.invalid/other"),
            "src/lib.rs",
            PARSER_VERSION,
            20,
        )
        .await
        .expect("repo-scoped miss is valid")
        .is_empty()
    );
    database.destroy(pool).await;
}

#[tokio::test]
async fn replace_file_refs_reports_unattached_without_losing_attached_edges() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let blob = unique_blob();
    stored_ddoc(
        &pool,
        &blob,
        PARSER_VERSION,
        "docs/plan.dd.json",
        &["ac-0001"],
    )
    .await;

    let missing = "docs/plan.dd.json#criteria/ac-dead";
    let outcome = replace_file_refs(
        &pool,
        &blob,
        PARSER_VERSION,
        &[
            file_ref(
                "docs/plan.dd.json#criteria/ac-0001",
                "src/lib.rs",
                "$.criteria[0].source",
            ),
            // Identical resolved inputs collapse under the table's UNIQUE key
            // and `attached` reports the post-replacement row count.
            file_ref(
                "docs/plan.dd.json#criteria/ac-0001",
                "src/lib.rs",
                "$.criteria[0].source",
            ),
            file_ref(missing, "src/missing.rs", "$.criteria[1].source"),
        ],
    )
    .await
    .expect("a missing source row is reported, not fatal");

    assert_eq!(outcome.attached, 1);
    assert_eq!(outcome.unattached, vec![missing]);
    assert_eq!(
        rows_referencing(&pool, None, "src/lib.rs", PARSER_VERSION, 20)
            .await
            .expect("attached edge remains")
            .len(),
        1
    );
    assert!(
        rows_referencing(&pool, None, "src/missing.rs", PARSER_VERSION, 20)
            .await
            .expect("missing edge lookup is valid")
            .is_empty()
    );
    database.destroy(pool).await;
}

async fn apply_migrations(pool: &PgPool, first: i64, last: i64) {
    for migration in MIGRATOR
        .iter()
        .filter(|migration| (first..=last).contains(&migration.version))
    {
        sqlx::raw_sql(&migration.sql)
            .execute(pool)
            .await
            .unwrap_or_else(|error| {
                panic!("migration {} should apply: {error}", migration.version)
            });
    }
}

async fn seed_code_row(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO elements
           (blob_sha, parser_version, kind, subkind, name, address,
            span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
         VALUES ('before-ddoc', 'parser@1', 'function', 'function_item', 'old',
                 'src/lib.rs::old', 1, 1, 0, 'fn old() {}', 'old-hash', true)",
    )
    .execute(pool)
    .await
    .expect("seed pre-0014 code row");
}

#[tokio::test]
async fn migration_applies_over_existing_code_elements_and_accepts_row() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;
    apply_migrations(&pool, 1, 16).await;
    seed_code_row(&pool).await;

    apply_migrations(&pool, 17, 17).await;

    let preserved: bool =
        sqlx::query_scalar("SELECT ddoc IS NULL FROM elements WHERE address = 'src/lib.rs::old'")
            .fetch_one(&pool)
            .await
            .expect("existing code row survives with NULL metadata");
    assert!(preserved);

    sqlx::query(
        "INSERT INTO elements
           (blob_sha, parser_version, kind, subkind, name, address,
            span_start, span_end, sibling_order, raw_text, raw_hash, enrich, ddoc)
         VALUES ('ddoc', 'parser@1', 'row', 'criterion', 'ac-0001',
                 'plan.dd.json#criteria/ac-0001', 1, 1, 0, 'criterion', 'row-hash', true,
                 '{\"id_kind\":\"ac\"}'::jsonb)",
    )
    .execute(&pool)
    .await
    .expect("0014 admits row elements");

    database.destroy(pool).await;
}

#[tokio::test]
async fn migration_kind_check_rejects_unknown_kind() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let error = sqlx::query(
        "INSERT INTO elements
           (blob_sha, parser_version, kind, subkind, name, address,
            span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
         VALUES ('unknown', 'parser@1', 'mystery', '', 'bad', 'bad',
                 1, 1, 0, 'bad', 'bad-hash', false)",
    )
    .execute(&pool)
    .await
    .expect_err("the known-kind constraint must reject arbitrary values");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("elements_kind_known")
    );

    database.destroy(pool).await;
}
