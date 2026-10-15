//! The BM25 keyword leg returns what the trigram leg returns: same hits,
//! same scores, same scope, on identifiers, camelCase, suffixed words and
//! phrases.
//!
//! Needs a Postgres with the `pg_search` extension (the ParadeDB image). On
//! a plain pgvector image the test says so and skips, unless
//! `FS3_REQUIRE_BM25` is set: CI sets it, so a missing extension fails the
//! gate there instead of passing it silently.

mod support;

use std::path::Path;

use fs3_core::{Element, ElementKind, RepoIdentity, Span};
use fs3_store::lexical_bm25::BM25_INDEX_DDL;
use fs3_store::{
    Bm25IndexState, LexicalBackend, PgPool, SearchFilters, bm25_can_answer, bm25_index_ready,
    bm25_index_state, build_bm25_index, register_worktree, search_bm25_page, search_bm25_pages,
    search_lexical_page, search_lexical_pages, sync_worktree_files, upsert_element_tree,
};
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

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

/// Creates the extension and index, or reports why this database cannot.
async fn bm25_available(pool: &PgPool) -> bool {
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_search')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    if !available {
        assert!(
            std::env::var_os("FS3_REQUIRE_BM25").is_none(),
            "FS3_REQUIRE_BM25 is set but this Postgres has no pg_search: \
             point FS3_TEST_DATABASE_URL at the ParadeDB image"
        );
        eprintln!(
            "SKIPPED: this Postgres has no pg_search extension (use the ParadeDB image to run it)"
        );
        return false;
    }
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_search")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(BM25_INDEX_DDL).execute(pool).await.unwrap();
    true
}

#[tokio::test]
async fn bm25_leg_returns_what_the_trigram_leg_returns() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let identity = RepoIdentity::from_path(Path::new("/srv/bm25"));
    let worktree = register_worktree(&pool, &identity, "/srv/bm25", None)
        .await
        .unwrap();
    let root = file(
        "src/lib.rs",
        vec![
            function(
                "bounded_error",
                "fn bounded_error(e: &str) -> String { e.into() }",
                3,
            ),
            function(
                "record_failure",
                "fn record_failure() { let bounded_errors = vec![]; drop(bounded_errors); }",
                7,
            ),
            function(
                "SearchFilters",
                "pub struct SearchFilters { limit: i64 }",
                11,
            ),
            function(
                "scope_of",
                "fn scope_of(f: &SearchFilters) -> i64 { f.limit } // search filters",
                15,
            ),
            function(
                "claim_next",
                "fn claim_next() { /* take the oldest of the pending jobs */ }",
                19,
            ),
            function("unrelated", "fn unrelated() { let x = 1; }", 23),
        ],
    );
    let blob = unique_blob();
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &root, |_| true)
        .await
        .unwrap();
    sync_worktree_files(&pool, worktree, &[("src/lib.rs".to_string(), blob.clone())])
        .await
        .unwrap();

    // The same identifiers in another repository: in scope for a wide
    // search only.
    let other = RepoIdentity::from_path(Path::new("/srv/elsewhere"));
    let other_worktree = register_worktree(&pool, &other, "/srv/elsewhere", None)
        .await
        .unwrap();
    let other_root = file(
        "src/other.rs",
        vec![function(
            "bounded_error_elsewhere",
            "fn bounded_error_elsewhere() {}",
            3,
        )],
    );
    let other_blob = unique_blob();
    upsert_element_tree(&pool, &other_blob, PARSER_VERSION, &other_root, |_| true)
        .await
        .unwrap();
    sync_worktree_files(
        &pool,
        other_worktree,
        &[("src/other.rs".to_string(), other_blob)],
    )
    .await
    .unwrap();

    if !bm25_available(&pool).await {
        database.destroy(pool).await;
        return;
    }
    assert!(bm25_index_ready(&pool).await.unwrap());

    let scoped = SearchFilters {
        repo: Some(identity.key().to_string()),
        worktree: Some("/srv/bm25".to_string()),
        limit: 20,
        ..SearchFilters::default()
    };
    let wide = SearchFilters {
        limit: 20,
        ..SearchFilters::default()
    };
    for query in [
        "bounded_error",
        "SearchFilters",
        "search filters",
        "pending job",
        "claim_next",
    ] {
        assert!(bm25_can_answer(query), "{query}");
        for filters in [&scoped, &wide] {
            let trigram = search_lexical_page(&pool, query, filters).await.unwrap();
            let bm25 = search_bm25_page(&pool, query, filters).await.unwrap();
            let shape = |page: &fs3_store::LexicalPage| {
                page.hits
                    .iter()
                    .map(|hit| (hit.element.name.clone(), (hit.score * 1e6).round() as i64))
                    .collect::<Vec<_>>()
            };
            assert!(!trigram.hits.is_empty(), "the fixture answers {query:?}");
            assert_eq!(
                shape(&bm25),
                shape(&trigram),
                "{query:?} with repo {:?}",
                filters.repo
            );
        }
    }

    // Both pages of one search, as the daemon asks for them.
    let (trigram_scoped, trigram_wide) =
        search_lexical_pages(&pool, "bounded_error", &scoped, true)
            .await
            .unwrap();
    let (bm25_scoped, bm25_wide) = search_bm25_pages(&pool, "bounded_error", &scoped, true)
        .await
        .unwrap();
    let names = |page: &fs3_store::LexicalPage| {
        let mut names: Vec<String> = page
            .hits
            .iter()
            .map(|hit| hit.element.name.clone())
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&bm25_scoped), names(&trigram_scoped));
    assert_eq!(names(&bm25_wide.unwrap()), names(&trigram_wide.unwrap()));

    // Chosen per call: the same entry point, routed by the filters.
    let routed = SearchFilters {
        lexical_backend: LexicalBackend::Bm25,
        ..scoped.clone()
    };
    let via_dispatch = search_lexical_page(&pool, "SearchFilters", &routed)
        .await
        .unwrap();
    let direct = search_bm25_page(&pool, "SearchFilters", &scoped)
        .await
        .unwrap();
    assert_eq!(via_dispatch.hits, direct.hits);

    // The scope is applied inside the BM25 windows, not after them.
    let scoped_names: Vec<String> = search_bm25_page(&pool, "bounded_error", &scoped)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.element.name)
        .collect();
    assert!(
        !scoped_names
            .iter()
            .any(|name| name == "bounded_error_elsewhere")
    );

    // A query with no token to look up belongs to the trigram leg.
    assert!(!bm25_can_answer("E'\\n'"));

    database.destroy(pool).await;
}

#[tokio::test]
async fn a_bm25_failure_answers_from_trigram_instead_of_failing() {
    // No BM25 index here, on either image: the BM25 statement fails, and the
    // search must still return the trigram page.
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let identity = RepoIdentity::from_path(Path::new("/srv/fallback"));
    let worktree = register_worktree(&pool, &identity, "/srv/fallback", None)
        .await
        .unwrap();
    let root = file(
        "src/lib.rs",
        vec![
            function("settle_failure", "fn settle_failure() {}", 3),
            function("other", "fn other() {}", 7),
        ],
    );
    let blob = unique_blob();
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &root, |_| true)
        .await
        .unwrap();
    sync_worktree_files(&pool, worktree, &[("src/lib.rs".to_string(), blob)])
        .await
        .unwrap();
    assert!(!bm25_index_ready(&pool).await.unwrap());

    let trigram = SearchFilters {
        repo: Some(identity.key().to_string()),
        limit: 10,
        ..SearchFilters::default()
    };
    let asked_for_bm25 = SearchFilters {
        lexical_backend: LexicalBackend::Bm25,
        ..trigram.clone()
    };
    let expected = search_lexical_page(&pool, "settle_failure", &trigram)
        .await
        .unwrap();
    let answered = search_lexical_page(&pool, "settle_failure", &asked_for_bm25)
        .await
        .expect("a BM25 failure must not fail the search");
    assert_eq!(answered.hits, expected.hits);
    assert_eq!(answered.hits.len(), 1);

    database.destroy(pool).await;
}

#[tokio::test]
async fn doctor_s_build_creates_the_index_repairs_an_invalid_one_and_is_a_no_op_after() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let state = bm25_index_state(&pool).await.unwrap();
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_search')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    if !available {
        assert_eq!(state, Bm25IndexState::NoExtension);
        // The same SKIPPED / FS3_REQUIRE_BM25 verdict as the parity test.
        assert!(!bm25_available(&pool).await);
        database.destroy(pool).await;
        return;
    }
    assert_eq!(state, Bm25IndexState::Missing);

    let bytes = build_bm25_index(&pool).await.unwrap();
    assert!(bytes > 0);
    assert_eq!(
        bm25_index_state(&pool).await.unwrap(),
        Bm25IndexState::Ready
    );
    // Run again: nothing to build, the same index answers.
    assert_eq!(build_bm25_index(&pool).await.unwrap(), bytes);

    // What an interrupted CONCURRENTLY build leaves behind.
    sqlx::query(
        "UPDATE pg_index SET indisvalid = false
          WHERE indexrelid = 'elements_bm25_idx'::regclass",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        bm25_index_state(&pool).await.unwrap(),
        Bm25IndexState::Invalid
    );
    assert!(!bm25_index_ready(&pool).await.unwrap());
    build_bm25_index(&pool).await.unwrap();
    assert_eq!(
        bm25_index_state(&pool).await.unwrap(),
        Bm25IndexState::Ready
    );

    database.destroy(pool).await;
}
