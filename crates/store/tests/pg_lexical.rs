//! Lexical retrieval contracts: verbatim recall, scope, structure, and plan.

mod support;

use std::path::Path;

use fs3_core::{Element, ElementKind, RepoIdentity, Span};
use fs3_store::{
    SearchFilters, register_worktree, search_lexical, sync_worktree_files, upsert_element_tree,
};
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

fn file(path: &str, children: Vec<Element>) -> Element {
    Element::new(
        ElementKind::File,
        "rust",
        path,
        path,
        Span::new(1, 100),
        children
            .iter()
            .map(|child| child.raw_text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .with_children(children)
}

fn function(name: &str, body: &str, line: u32) -> Element {
    Element::new(
        ElementKind::Function,
        "function_item",
        name,
        format!("src/lib.rs::{name}"),
        Span::new(line, line),
        body,
    )
}

#[tokio::test]
async fn verbatim_phrase_hits_all_elements_and_structural_names_rank_first() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let identity = RepoIdentity::from_path(Path::new("/srv/lexical"));
    let worktree = register_worktree(&pool, &identity, "/srv/lexical", None)
        .await
        .unwrap();
    let phrase = "leopon verbatim phrase anchor";
    let root = file(
        "src/lib.rs",
        vec![
            function(phrase, "fn structural() {}", 3),
            function(
                "body_one",
                &format!("fn body_one() {{ /* {phrase} */ }}"),
                7,
            ),
            function(
                "body_two",
                &format!("fn body_two() {{ let s = \"{phrase}\"; }}"),
                11,
            ),
            function("semantic_only", "fn semantic_only() {}", 15),
        ],
    );
    let blob = unique_blob();
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &root, |_| true)
        .await
        .unwrap();
    sync_worktree_files(&pool, worktree, &[("src/lib.rs".to_string(), blob.clone())])
        .await
        .unwrap();

    let hits = search_lexical(
        &pool,
        phrase,
        &SearchFilters {
            repo: Some(identity.key().to_string()),
            worktree: Some("/srv/lexical".to_string()),
            kinds: Some(vec![ElementKind::Function]),
            limit: 10,
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        hits.len(),
        3,
        "every and only phrase-bearing element returns"
    );
    assert_eq!(
        hits[0].element.name, phrase,
        "name match gets structural boost"
    );
    assert!(
        hits.iter()
            .all(|hit| hit.path.as_deref() == Some("src/lib.rs"))
    );

    let filtered = search_lexical(
        &pool,
        phrase,
        &SearchFilters {
            repo: Some(identity.key().to_string()),
            path: Some("docs/%".to_string()),
            limit: 10,
            ..SearchFilters::default()
        },
    )
    .await
    .unwrap();
    assert!(
        filtered.is_empty(),
        "path filtering happens inside the lexical leg"
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn selective_exact_text_uses_the_trigram_index_on_a_prod_shaped_population() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    sqlx::query(
        "INSERT INTO elements
           (blob_sha, parser_version, kind, subkind, name, address, span_start,
            span_end, sibling_order, raw_text, raw_hash, enrich)
         SELECT lpad(i::text, 40, '0'), 'perf@1', 'function', 'function_item',
                'ordinary_' || i, 'src/generated.rs::ordinary_' || i,
                i, i, i, 'fn ordinary_' || i || '() {}', lpad(i::text, 64, '0'), true
           FROM generate_series(1, 25000) i",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE elements")
        .execute(&pool)
        .await
        .unwrap();

    let plan = sqlx::query_scalar::<_, String>(
        "EXPLAIN (ANALYZE, FORMAT TEXT)
         SELECT id FROM elements
          WHERE lower(name || E'\\n' || raw_text) LIKE '%needle-that-is-absent%'
          LIMIT 101",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .join("\n");

    assert!(plan.contains("elements_lexical_trgm_idx"), "{plan}");
    assert!(!plan.contains("Seq Scan on elements"), "{plan}");

    database.destroy(pool).await;
}
