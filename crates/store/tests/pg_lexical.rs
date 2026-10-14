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

/// One pass, two pages: the combined statement must return exactly what the
/// scoped and the unscoped statements return on their own, hit for hit, in
/// order, with the same locations and hidden counts.
#[tokio::test]
async fn one_pass_returns_the_scoped_and_the_wide_page_exactly() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let phrase = "okapi single pass anchor";
    let mut identities = Vec::new();
    for (repo, functions) in [("/srv/lex-a", 4_u32), ("/srv/lex-b", 6)] {
        let identity = RepoIdentity::from_path(Path::new(repo));
        let worktree = register_worktree(&pool, &identity, repo, None)
            .await
            .unwrap();
        // Bodies of different lengths, so the density bonus orders them.
        let children = (0..functions)
            .map(|i| {
                function(
                    &format!("f{i}"),
                    &format!(
                        "fn f{i}() {{ /* {phrase} {} */ }}",
                        "x".repeat(i as usize * 40)
                    ),
                    3 + i * 4,
                )
            })
            .collect();
        let blob = unique_blob();
        upsert_element_tree(
            &pool,
            &blob,
            PARSER_VERSION,
            &file("src/lib.rs", children),
            |_| true,
        )
        .await
        .unwrap();
        sync_worktree_files(&pool, worktree, &[("src/lib.rs".to_string(), blob)])
            .await
            .unwrap();
        identities.push(identity.key().to_string());
    }

    let shape = |page: &fs3_store::LexicalPage| {
        page.hits
            .iter()
            .map(|hit| {
                (
                    hit.element.address.clone(),
                    hit.score.to_bits(),
                    hit.identity.clone(),
                    hit.root_path.clone(),
                    hit.path.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    for (limit, path) in [(3, None), (10, None), (3, Some("src/%".to_string()))] {
        let scoped = SearchFilters {
            repo: Some(identities[0].clone()),
            worktree: Some("/srv/lex-a".to_string()),
            path: path.clone(),
            limit,
            ..SearchFilters::default()
        };
        let wide = SearchFilters {
            repo: None,
            worktree: None,
            ..scoped.clone()
        };
        let (one_scoped, one_wide) = fs3_store::search_lexical_pages(&pool, phrase, &scoped, true)
            .await
            .unwrap();
        let one_wide = one_wide.expect("asked for the wide page");
        let alone_scoped = fs3_store::search_lexical_page(&pool, phrase, &scoped)
            .await
            .unwrap();
        let alone_wide = fs3_store::search_lexical_page(&pool, phrase, &wide)
            .await
            .unwrap();

        assert!(
            !alone_scoped.hits.is_empty(),
            "the fixture matches in scope"
        );
        assert_eq!(
            shape(&one_scoped),
            shape(&alone_scoped),
            "scoped page, limit {limit}, path {path:?}"
        );
        assert_eq!(
            shape(&one_wide),
            shape(&alone_wide),
            "wide page, limit {limit}, path {path:?}"
        );
        assert_eq!(one_scoped.hidden_turns, alone_scoped.hidden_turns);
        assert_eq!(one_wide.hidden_turns, alone_wide.hidden_turns);
        assert!(
            one_wide
                .hits
                .iter()
                .any(|hit| hit.identity.as_deref() == Some(&identities[1])),
            "the wide page reaches the other repository"
        );
    }

    database.destroy(pool).await;
}
