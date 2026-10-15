//! The binary copy of the vectors that exact ranking can read (migration
//! 0033): kept in step by trigger, filled by `build_exact_index`, and read by
//! search only when `fs3.exact_rank = compact` and the copy is complete.

mod support;

use fs3_core::RepoIdentity;
use fs3_store::{
    EMBEDDING_DIMENSIONS, ExactIndexState, NewEmbedding, PgPool, SearchFilters, SourceKind,
    build_exact_index, exact_index_state, guard_exact_index, put_embeddings, register_worktree,
    search_elements,
};
use fs3_testkit::FreshDatabase;
use support::PARSER_VERSION;

const EMBEDDER: &str = "exact-index-fixture@1024";

/// A dense pseudo-random unit vector. Binary quantization keeps one sign bit
/// per dimension, so the fixtures must spread their energy over every
/// dimension the way real embeddings do; an axis-aligned vector would carry
/// no signal in its bits.
fn dense(seed: u64) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut vector: Vec<f32> = (0..EMBEDDING_DIMENSIONS)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 11) as f32 / (1_u64 << 53) as f32) - 0.5
        })
        .collect();
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    for x in &mut vector {
        *x /= norm;
    }
    vector
}

/// A unit vector at cosine distance `distance` from `base`.
fn at_distance(base: &[f32], distance: f32, seed: u64) -> Vec<f32> {
    let mut direction = dense(seed);
    let dot: f32 = direction.iter().zip(base).map(|(d, b)| d * b).sum();
    for (slot, b) in direction.iter_mut().zip(base) {
        *slot -= dot * b;
    }
    let norm = direction.iter().map(|d| d * d).sum::<f32>().sqrt();
    let cosine = 1.0 - distance;
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    base.iter()
        .zip(&direction)
        .map(|(b, d)| cosine * b + sine * d / norm)
        .collect()
}

/// `count` function elements in `worktree_id`, each with one raw vector at a
/// distinct distance from `query` between 0.05 and 0.65.
async fn seed_scope(pool: &PgPool, worktree_id: i64, count: i32, query: &[f32]) {
    sqlx::query(
        "INSERT INTO elements
             (blob_sha, parser_version, kind, subkind, name, address,
              span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
         SELECT 'bq-blob-' || n, $1, 'function', 'function_item',
                'f_' || n, 'src/bq-' || n || '.rs::f',
                1, 1, n, 'body ' || n, 'bq-raw-' || n, false
           FROM generate_series(1, $2) AS n",
    )
    .bind(PARSER_VERSION)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO worktree_files (worktree_id, path, blob_sha)
         SELECT $1, 'src/bq-' || n || '.rs', 'bq-blob-' || n
           FROM generate_series(1, $2) AS n",
    )
    .bind(worktree_id)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
    let rows = (1..=count)
        .map(|n| {
            // A deterministic shuffle of distances, so ids and nearness differ.
            let distance = 0.05 + 0.6 * ((n as u64 * 7_919) % count as u64) as f32 / count as f32;
            (
                format!("bq-raw-{n}"),
                at_distance(query, distance, n as u64),
            )
        })
        .collect::<Vec<_>>();
    let embeddings = rows
        .iter()
        .map(|(source_hash, vector)| NewEmbedding {
            chunk_no: 0,
            source_hash,
            source_kind: SourceKind::Raw,
            vector,
            truncated: false,
        })
        .collect::<Vec<_>>();
    put_embeddings(pool, EMBEDDER, &embeddings).await.unwrap();
}

async fn copies(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM embeddings_1024_bq")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn set_database(pool: &PgPool, setting: &str) {
    sqlx::query(&format!(
        "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I SET {setting}', current_database()); \
         END $$;"
    ))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_trigger_keeps_the_binary_copy_in_step_with_every_write() {
    let database = FreshDatabase::create_from(&fs3_testkit::test_database_url(), "bq-trigger")
        .await
        .expect("create migrated database");
    let pool = database.pool().await;
    let first = dense(1);
    let embeddings = [NewEmbedding {
        chunk_no: 0,
        source_hash: "trigger-raw",
        source_kind: SourceKind::Raw,
        vector: &first,
        truncated: false,
    }];
    put_embeddings(&pool, EMBEDDER, &embeddings).await.unwrap();
    let bits = || async {
        sqlx::query_scalar::<_, bool>(
            "SELECT b.bq = binary_quantize(e.vector)::bit(1024)
               FROM embeddings_1024 e
               JOIN embeddings_1024_bq b USING (source_hash, source_kind, model_key, chunk_no)",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
    };
    assert_eq!(bits().await, [true], "an insert writes its copy");

    // `put_embeddings` upserts: a re-embedded vector arrives as an UPDATE.
    let second = dense(2);
    let embeddings = [NewEmbedding {
        vector: &second,
        ..embeddings[0]
    }];
    put_embeddings(&pool, EMBEDDER, &embeddings).await.unwrap();
    assert_eq!(
        bits().await,
        [true],
        "a re-embedded vector rewrites its copy"
    );

    // No writer moves a vector to another key, but if one does the copy
    // moves with it.
    sqlx::query("UPDATE embeddings_1024 SET source_hash = 'moved-raw'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(bits().await, [true], "a moved key moves its copy");
    assert_eq!(copies(&pool).await, 1, "and leaves no orphan behind");

    sqlx::query("DELETE FROM embeddings_1024")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(copies(&pool).await, 0, "a delete removes the copy");

    // TRUNCATE fires no row trigger; its statement trigger empties the copy.
    put_embeddings(&pool, EMBEDDER, &embeddings).await.unwrap();
    assert_eq!(copies(&pool).await, 1);
    sqlx::query("TRUNCATE embeddings_1024")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(copies(&pool).await, 0, "a truncate empties the copy");
    database.destroy(pool).await;
}

#[tokio::test]
async fn the_build_fills_vectors_that_predate_the_trigger_and_records_the_copy_complete() {
    let database = FreshDatabase::create_from(&fs3_testkit::test_database_url(), "bq-build")
        .await
        .expect("create migrated database");
    let pool = database.pool().await;
    let vectors = (0..120).map(dense).collect::<Vec<_>>();
    let hashes = (0..120).map(|n| format!("build-{n}")).collect::<Vec<_>>();
    let embeddings = vectors
        .iter()
        .zip(&hashes)
        .map(|(vector, source_hash)| NewEmbedding {
            chunk_no: 0,
            source_hash,
            source_kind: SourceKind::Raw,
            vector,
            truncated: false,
        })
        .collect::<Vec<_>>();
    put_embeddings(&pool, EMBEDDER, &embeddings).await.unwrap();
    // Rows written before migration 0033 have no copy: drop some to stand in.
    sqlx::query("DELETE FROM embeddings_1024_bq WHERE source_hash LIKE 'build-1%'")
        .execute(&pool)
        .await
        .unwrap();

    let before = exact_index_state(&pool).await.unwrap();
    assert!(
        matches!(&before, ExactIndexState::NotBuilt { coverage }
            if coverage.len() == 1 && coverage[0].vectors == 120 && coverage[0].missing == 31),
        "{before:?}"
    );

    let build = build_exact_index(&pool).await.unwrap();
    assert!(build.ready);
    assert_eq!(build.filled, 31);
    assert!(build.bytes > 0);
    assert!(matches!(
        exact_index_state(&pool).await.unwrap(),
        ExactIndexState::Ready { .. }
    ));

    // A second run only re-checks.
    assert_eq!(build_exact_index(&pool).await.unwrap().filled, 0);

    // A vector that lost its copy behind the trigger's back is drift.
    sqlx::query("DELETE FROM embeddings_1024_bq WHERE source_hash = 'build-7'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        exact_index_state(&pool).await.unwrap(),
        ExactIndexState::Drifted { .. }
    ));
    // ... which the guard clears on sight, so search stops reading it.
    let cleared = guard_exact_index(&pool).await.unwrap();
    assert!(
        cleared.is_some_and(|coverage| coverage[0].missing == 1),
        "the guard names what it found missing"
    );
    assert!(matches!(
        exact_index_state(&pool).await.unwrap(),
        ExactIndexState::NotBuilt { .. }
    ));
    assert_eq!(
        guard_exact_index(&pool).await.unwrap(),
        None,
        "nothing marked, nothing to clear"
    );
    database.destroy(pool).await;
}

/// The recall gate for `fs3.exact_rank = compact`: on a synthetic corpus the
/// binary ranking plus the full-vector re-rank returns the same top ten as
/// the full-vector exact ranking. And it only runs once the copy is recorded
/// complete: before that, `compact` falls back to the full vectors.
#[tokio::test]
async fn compact_exact_ranking_returns_the_float_top_ten() {
    let database = FreshDatabase::create_from(&fs3_testkit::test_database_url(), "bq-recall")
        .await
        .expect("create migrated database");
    let pool = database.pool().await;
    let repo = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/bq-recall").unwrap();
    let worktree = register_worktree(&pool, &repo, "/fixtures/bq-recall", Some("main"))
        .await
        .unwrap();
    let query = dense(424_242);
    seed_scope(&pool, worktree, 3_000, &query).await;
    sqlx::query("ANALYZE elements, worktree_files, embeddings_1024, embeddings_1024_bq")
        .execute(&pool)
        .await
        .unwrap();
    let filters = SearchFilters {
        repo: Some(repo.key().to_string()),
        source: Some(SourceKind::Raw),
        limit: 10,
        parser_version: Some(PARSER_VERSION.to_string()),
        ..SearchFilters::default()
    };
    let top = |page: &fs3_store::SearchPage| {
        page.hits
            .iter()
            .map(|hit| hit.similar.element.address.clone())
            .collect::<Vec<_>>()
    };

    let float = search_elements(&pool, EMBEDDER, &query, &filters)
        .await
        .unwrap();
    assert!(float.exact_ranked && !float.compact_scan);
    assert_eq!(float.hits.len(), 10);

    // Asked for, but the copy was never recorded complete: full vectors.
    set_database(&pool, "fs3.exact_rank = compact").await;
    pool.close().await;
    let pool = database.pool().await;
    let unready = search_elements(&pool, EMBEDDER, &query, &filters)
        .await
        .unwrap();
    assert!(!unready.compact_scan, "an unrecorded copy is never read");
    assert_eq!(top(&unready), top(&float));

    build_exact_index(&pool).await.unwrap();
    let compact = search_elements(&pool, EMBEDDER, &query, &filters)
        .await
        .unwrap();
    assert!(compact.exact_ranked && compact.compact_scan);
    assert_eq!(top(&compact), top(&float), "compact == float, top ten");
    // A copy that lost rows behind the trigger's back stops being read once
    // the guard sees it: never compact on a copy known to be incomplete.
    sqlx::query("DELETE FROM embeddings_1024_bq WHERE source_hash = 'bq-raw-1'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(guard_exact_index(&pool).await.unwrap().is_some());
    let guarded = search_elements(&pool, EMBEDDER, &query, &filters)
        .await
        .unwrap();
    assert!(!guarded.compact_scan, "a cleared mark falls back to float");
    assert_eq!(top(&guarded), top(&float));
    for (compact_hit, float_hit) in compact.hits.iter().zip(&float.hits) {
        assert!(
            (compact_hit.similar.distance - float_hit.similar.distance).abs() < 1e-6,
            "re-ranked by the full vector, so the distances are the full ones"
        );
    }
    database.destroy(pool).await;
}
