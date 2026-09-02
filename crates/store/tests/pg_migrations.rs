//! What the migration story has to be true for: a fresh database becomes a
//! working store, and running it again changes nothing.
//!
//! `pg_round_trip.rs` migrates the *shared* 5433 database, which by the second
//! run is never fresh — so it can only ever prove the no-op half by accident.
//! These tests create a throwaway database per test and prove both halves on
//! purpose. Like every store test, they need the dockerized Postgres and fail
//! naming the command rather than skipping.

mod support;

use fs3_core::{Element, ElementKind, Span};
use fs3_store::{ElementTreeWrite, MIGRATOR, PgPool, migrate, upsert_element_tree};
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

/// Every version the binary carries, in order. Asserting against the embedded
/// set rather than a hardcoded `[1]` keeps this test honest when `0006` lands.
fn embedded_versions() -> Vec<i64> {
    MIGRATOR.iter().map(|migration| migration.version).collect()
}

async fn applied_rows(pool: &PgPool) -> Vec<(i64, String, Vec<u8>, bool)> {
    // `installed_on` is a TIMESTAMPTZ and sqlx has no date/time feature enabled
    // here, so it comes back as text — which is all this needs it for.
    sqlx::query_as("SELECT version, installed_on::text, checksum, success FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .expect("the migration tracking table should be readable")
}

/// Does the named relation exist?
async fn relation(pool: &PgPool, name: &str) -> Option<String> {
    sqlx::query_scalar("SELECT to_regclass($1)::text")
        .bind(format!("public.{name}"))
        .fetch_one(pool)
        .await
        .expect("asking for the relation should succeed")
}

/// A database with nothing in it becomes a usable store.
#[tokio::test]
async fn migrating_an_empty_database_bootstraps_the_whole_schema() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    migrate(&pool)
        .await
        .expect("a fresh database should migrate");

    let versions: Vec<i64> = applied_rows(&pool)
        .await
        .into_iter()
        .map(|(version, _, _, _)| version)
        .collect();
    assert_eq!(
        versions,
        embedded_versions(),
        "every embedded migration should be recorded as applied"
    );

    // Workshop 002's three layers, named one by one. A migration that silently
    // half-applied would still record its version, so the version set alone is
    // not evidence that the schema is there.
    for table in [
        "repos",
        "worktrees",
        "worktree_files",
        "elements",
        "smart_content",
        "embeddings_1024",
        "jobs",
    ] {
        assert_eq!(
            relation(&pool, table).await.as_deref(),
            Some(table),
            "the workshop 002 schema is incomplete without {table}"
        );
    }

    // These indexes carry query shapes, not micro-optimisations: removing one
    // restores a whole-table scan for its retrieval leg.
    for index in [
        "embeddings_1024_vector_idx",
        "elements_lexical_trgm_idx",
        "jobs_live_dedupe_idx",
        "jobs_claim_embed_idx",
        "jobs_claim_general_idx",
    ] {
        assert_eq!(
            relation(&pool, index).await.as_deref(),
            Some(index),
            "{index} carries a decision, not a micro-optimisation"
        );
    }

    // PRD req 4: the store is Postgres *with pgvector*, and the migration is
    // where a stack missing it is supposed to fail.
    let vector: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname = 'vector'")
            .fetch_one(&pool)
            .await
            .expect("asking for the extension should succeed");
    assert_eq!(vector, 1, "0001 should have created the vector extension");

    let trigram: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname = 'pg_trgm'")
            .fetch_one(&pool)
            .await
            .expect("asking for the extension should succeed");
    assert_eq!(trigram, 1, "0018 should have created the trigram extension");
    let definitions: Vec<(String, String)> = sqlx::query_as(
        "SELECT indexname, indexdef
           FROM pg_indexes
          WHERE schemaname = 'public'
            AND indexname IN ('jobs_claim_embed_idx', 'jobs_claim_general_idx')
          ORDER BY indexname",
    )
    .fetch_all(&pool)
    .await
    .expect("the pending claim indexes should be inspectable");
    assert_eq!(definitions.len(), 2, "both claim lanes need an access path");
    for (name, definition) in definitions {
        assert!(
            definition.contains("(priority DESC, id DESC) INCLUDE (not_before)"),
            "{name} must retain priority-first LIFO ordering: {definition}"
        );
        assert!(
            definition.contains("state = 'pending'"),
            "{name} must exclude settled history: {definition}"
        );
        match name.as_str() {
            "jobs_claim_embed_idx" => assert!(definition.contains("kind = 'embed'")),
            "jobs_claim_general_idx" => {
                assert!(definition.contains("scan_file") && definition.contains("summarize"));
            }
            _ => unreachable!("the query admits only the two claim indexes"),
        }
    }

    database.destroy(pool).await;
}

/// 0004 replaces 0001's `elements`, so the columns the tree model needs have to
/// actually be there — and 0001's flat-table columns have to actually be gone.
///
/// A DROP-and-recreate that silently kept the old table (because the DROP was
/// conditional on something untrue, say) would pass every version assertion
/// above and fail on the first write.
#[tokio::test]
async fn the_elements_table_is_the_tree_shape_not_the_flat_one() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = 'elements'
          ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("the column list should be readable");

    for column in [
        "blob_sha",
        "parser_version",
        "parent_id",
        "sibling_order",
        "raw_hash",
        "enrich",
    ] {
        assert!(
            columns.iter().any(|found| found == column),
            "0004 must add {column}; got {columns:?}"
        );
    }
    for gone in ["blob", "qualified_name", "ts_kind", "body", "has_error"] {
        assert!(
            !columns.iter().any(|found| found == gone),
            "{gone} belonged to 0001's flat table and must not survive 0004: {columns:?}"
        );
    }

    database.destroy(pool).await;
}

/// Running migrations again is a no-op — which is what makes "restart the
/// daemon" a safe answer, and what lets boot migrate unconditionally.
#[tokio::test]
async fn migrating_twice_changes_nothing() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    migrate(&pool).await.expect("the first run should apply");
    let after_first = applied_rows(&pool).await;

    migrate(&pool)
        .await
        .expect("the second run should be a no-op");
    let after_second = applied_rows(&pool).await;

    // Identical `installed_on` is the real proof: not merely "the same set of
    // versions", but "nothing ran again".
    assert_eq!(
        after_first, after_second,
        "the second run must not re-apply, re-time or re-checksum anything"
    );
    assert_eq!(
        after_first.len(),
        embedded_versions().len(),
        "no duplicate rows should appear in the tracking table"
    );

    database.destroy(pool).await;
}

/// Apply the embedded migrations in `versions`, in order, without touching
/// `_sqlx_migrations`.
///
/// The point is to stand a database up in a genuinely PRE-migration state so
/// the next migration can be run AT it. Nothing here reimplements a migration:
/// the SQL is the embedded text, so a migration edited tomorrow is the SQL this
/// runs tomorrow. A RANGE rather than a ceiling because these are not
/// idempotent — 0004 replaces 0001's `elements`, so re-running from the start
/// fails on a schema the earlier migration no longer describes.
async fn apply_migrations(pool: &PgPool, versions: std::ops::RangeInclusive<i64>) {
    for migration in MIGRATOR
        .iter()
        .filter(|entry| versions.contains(&entry.version))
    {
        sqlx::raw_sql(&migration.sql)
            .execute(pool)
            .await
            .unwrap_or_else(|error| {
                panic!("migration {} should apply: {error}", migration.version)
            });
    }
}

#[tokio::test]
async fn embedding_chunks_have_the_exact_key_and_defaulted_smallint_column() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let key_columns: Vec<String> = sqlx::query_scalar(
        "SELECT a.attname::text
           FROM pg_index i
          CROSS JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS key(attnum, ordinal)
           JOIN pg_attribute a
             ON a.attrelid = i.indrelid AND a.attnum = key.attnum
          WHERE i.indrelid = 'embeddings_1024'::regclass
            AND i.indisprimary
          ORDER BY key.ordinal",
    )
    .fetch_all(&pool)
    .await
    .expect("the embedding primary key should be inspectable");
    assert_eq!(
        key_columns,
        ["source_hash", "source_kind", "chunk_no", "model_key"],
        "the chunk number is part of vector identity, not row metadata"
    );

    let column: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT format_type(a.atttypid, a.atttypmod),
                a.attnotnull,
                pg_get_expr(d.adbin, d.adrelid)
           FROM pg_attribute a
           LEFT JOIN pg_attrdef d
             ON d.adrelid = a.attrelid AND d.adnum = a.attnum
          WHERE a.attrelid = 'embeddings_1024'::regclass
            AND a.attname = 'chunk_no'",
    )
    .fetch_one(&pool)
    .await
    .expect("the chunk column should be inspectable");
    assert_eq!(
        column,
        ("smallint".to_string(), true, Some("0".to_string()))
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn migration_0022_grandfathers_vectors_without_minting_embed_jobs() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;
    apply_migrations(&pool, 1..=21).await;

    sqlx::query(
        "INSERT INTO embeddings_1024
             (source_hash, source_kind, model_key, vector, truncated)
         VALUES ('grandfathered', 'raw', 'legacy-model',
                 array_fill(0.0::real, ARRAY[1024])::vector, true)",
    )
    .execute(&pool)
    .await
    .expect("the old three-column key should accept a vector");
    let jobs_before: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'embed'")
        .fetch_one(&pool)
        .await
        .expect("count pre-migration embed jobs");

    apply_migrations(&pool, 22..=22).await;

    let grandfathered: (i16, bool) = sqlx::query_as(
        "SELECT chunk_no, truncated
           FROM embeddings_1024
          WHERE source_hash = 'grandfathered'",
    )
    .fetch_one(&pool)
    .await
    .expect("the pre-chunk vector should survive");
    assert_eq!(grandfathered, (0, true));
    let jobs_after: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'embed'")
        .fetch_one(&pool)
        .await
        .expect("count post-migration embed jobs");
    assert_eq!(jobs_after, jobs_before, "the migration must not mint work");

    database.destroy(pool).await;
}

#[tokio::test]
async fn migration_0023_dedupes_revivable_jobs_and_covers_live_depth() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;
    apply_migrations(&pool, 1..=22).await;

    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, terminal)
         VALUES ('scan_file', 'owned-by-pending', '{}'::jsonb, 'pending', false),
                ('scan_file', 'owned-by-pending', '{}'::jsonb, 'failed', false),
                ('scan_file', 'owned-by-pending', '{}'::jsonb, 'failed', false),
                ('embed', 'failed-only', '{}'::jsonb, 'failed', false),
                ('embed', 'failed-only', '{}'::jsonb, 'failed', false)",
    )
    .execute(&pool)
    .await
    .expect("the old predicate permits failed duplicates");

    apply_migrations(&pool, 23..=23).await;

    let owners: Vec<(String, i64)> = sqlx::query_as(
        "SELECT dedupe_key, count(*)
           FROM jobs
          WHERE state IN ('pending', 'running')
             OR (state = 'failed' AND NOT terminal)
          GROUP BY dedupe_key
          ORDER BY dedupe_key",
    )
    .fetch_all(&pool)
    .await
    .expect("read active owners");
    assert_eq!(
        owners,
        [
            ("failed-only".to_string(), 1),
            ("owned-by-pending".to_string(), 1)
        ]
    );
    let retired: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = 'failed' AND terminal")
            .fetch_one(&pool)
            .await
            .expect("count retired duplicates");
    assert_eq!(retired, 3, "duplicates are retained as terminal history");

    let index: String =
        sqlx::query_scalar("SELECT pg_get_indexdef('jobs_live_dedupe_idx'::regclass)")
            .fetch_one(&pool)
            .await
            .expect("inspect active-job index");
    for required in [
        "INCLUDE (kind, state, last_error, terminal)",
        "state = 'failed'",
        "NOT terminal",
    ] {
        assert!(index.contains(required), "{required} missing from {index}");
    }

    let duplicate = sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, terminal)
         VALUES ('embed', 'failed-only', '{}'::jsonb, 'failed', false)",
    )
    .execute(&pool)
    .await;
    assert!(
        duplicate.is_err(),
        "failed non-terminal owner must hold its key"
    );

    let receipt_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_retention_state WHERE singleton")
            .fetch_one(&pool)
            .await
            .expect("read seeded retention receipt");
    assert_eq!(receipt_rows, 1);

    database.destroy(pool).await;
}

/// The migration that had to double as a RECOVERY (Jordan, 2026-08-27).
///
/// A daemon run from a throwaway dev worktree wrote `update:blocked` naming its
/// own `target/debug` path into a shared store. The production daemon restarted
/// the next day on a current binary and went on serving that fossil to every
/// envelope, because update state was keyed to the STORE and there was nothing
/// to re-evaluate it against.
///
/// The fix arrives as a new binary, and a new binary migrates at boot — so the
/// migration is where the fossil dies. No repair verb, no hand-written SQL for
/// a user to find in a doc. What it must NOT do is take the other producers'
/// messages with it: a schema skew is exactly as true after this statement as
/// before it.
#[tokio::test]
async fn migrating_to_per_install_update_state_kills_the_store_keyed_fossils() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;

    apply_migrations(&pool, 1..=11).await;

    // The world as a pre-fix binary left it: one singleton row describing a
    // path that only ever existed in somebody's build tree, and the standing
    // message it produced.
    sqlx::query(
        "UPDATE update_state
            SET installed_version = '0.2.0',
                install_path      = '/tmp/worktree/target/debug/flowspace3',
                blocked_reason    = 'the install directory is not writable'
          WHERE singleton",
    )
    .execute(&pool)
    .await
    .expect("seeding the pre-fix update row");

    for (key, source) in [
        ("update:blocked", "update"),
        ("update:installed:0.2.0", "update"),
        ("schema:ahead:9001", "schema"),
    ] {
        sqlx::query(
            "INSERT INTO user_messages (key, source, severity, text, next_action)
                  VALUES ($1, $2, 'warning', 'seeded', 'do the thing')",
        )
        .bind(key)
        .bind(source)
        .execute(&pool)
        .await
        .expect("seeding a pre-fix message");
    }

    apply_migrations(&pool, 12..=12).await;

    let survivors: Vec<(String, String)> =
        sqlx::query_as("SELECT key, source FROM user_messages ORDER BY key")
            .fetch_all(&pool)
            .await
            .expect("reading the queue back");
    assert_eq!(
        survivors,
        vec![("schema:ahead:9001".to_string(), "schema".to_string())],
        "every store-keyed update message must die, and nobody else's may"
    );

    // The state row goes with them: it is keyed to a store, so there is no
    // honest install to re-home it onto, and one check re-derives all of it.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM update_state")
        .fetch_one(&pool)
        .await
        .expect("counting update rows");
    assert_eq!(rows, 0, "the singleton row must not survive its own schema");

    // And the shape it left behind is the per-install one.
    let key_column: String = sqlx::query_scalar(
        "SELECT a.attname
           FROM pg_index i
           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
          WHERE i.indrelid = 'update_state'::regclass AND i.indisprimary",
    )
    .fetch_one(&pool)
    .await
    .expect("reading the primary key");
    assert_eq!(key_column, "install_path");

    database.destroy(pool).await;
}

#[tokio::test]
async fn migration_0020_keeps_lowest_root_requeues_failures_and_enforces_uniqueness() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;
    apply_migrations(&pool, 1..=19).await;
    let blob = unique_blob();
    let file = |path: &str| {
        Element::new(
            ElementKind::File,
            "rust",
            path,
            path,
            Span::new(1, 1),
            "same bytes",
        )
    };

    upsert_element_tree(&pool, &blob, PARSER_VERSION, &file("src/first.rs"), |_| {
        true
    })
    .await
    .expect("seed first pre-fix root");
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &file("src/second.rs"), |_| {
        true
    })
    .await
    .expect("seed second pre-fix root");
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, attempts, last_error)
         VALUES ('scan_file', 'scan:repair:src/second.rs',
                 jsonb_build_object('blob', $1, 'path', 'src/second.rs'),
                 'failed', 3,
                 'blob has 2 file roots, expected exactly one')",
    )
    .bind(blob.as_str())
    .execute(&pool)
    .await
    .expect("seed affected failed scan");

    apply_migrations(&pool, 20..=20).await;

    let roots: Vec<String> = sqlx::query_scalar(
        "SELECT address FROM elements
          WHERE blob_sha = $1 AND parser_version = $2 AND parent_id IS NULL
          ORDER BY id",
    )
    .bind(blob.as_str())
    .bind(PARSER_VERSION)
    .fetch_all(&pool)
    .await
    .expect("read repaired roots");
    assert_eq!(roots, ["src/first.rs"], "lowest-id root survives");

    let repaired_job: (String, i32, String) = sqlx::query_as(
        "SELECT state, attempts, last_error FROM jobs
          WHERE dedupe_key = 'scan:repair:src/second.rs'",
    )
    .fetch_one(&pool)
    .await
    .expect("read repaired job");
    assert_eq!(repaired_job.0, "pending");
    assert_eq!(repaired_job.1, 0);
    assert!(repaired_job.2.contains("migration 0020"));

    let outcome = upsert_element_tree(&pool, &blob, PARSER_VERSION, &file("src/third.rs"), |_| {
        true
    })
    .await
    .expect("post-migration writer converges");
    assert_eq!(
        outcome,
        ElementTreeWrite::Reused {
            stored_path: "src/first.rs".to_string()
        }
    );

    database.destroy(pool).await;
}

#[tokio::test]
async fn migration_0021_canonicalizes_registered_conversation_anchors_only() {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;
    apply_migrations(&pool, 1..=20).await;

    let repo_id: i64 = sqlx::query_scalar(
        "INSERT INTO repos (identity)
         VALUES ('git:github.com/fs3/anchored')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("seed canonical repository");
    sqlx::query("INSERT INTO worktrees (repo_id, root_path) VALUES ($1, '/srv/anchored')")
        .bind(repo_id)
        .execute(&pool)
        .await
        .expect("seed registered worktree");

    sqlx::query(
        "INSERT INTO conversations (guid, repo_identity, worktree, started_at)
         VALUES
           ('6ba7b810-9dad-11d1-80b4-00c04fd430c8',
            'https://github.com/fs3/anchored.git', '/srv/anchored', now()),
           ('6ba7b810-9dad-11d1-80b4-00c04fd430c9',
            NULL, '/srv/anchored', now()),
           ('6ba7b810-9dad-11d1-80b4-00c04fd430ca',
            'https://github.com/fs3/foreign.git', '/srv/foreign', now())",
    )
    .execute(&pool)
    .await
    .expect("seed pre-fix conversation anchors");

    apply_migrations(&pool, 21..=21).await;

    let anchors: Vec<Option<String>> =
        sqlx::query_scalar("SELECT repo_identity FROM conversations ORDER BY guid")
            .fetch_all(&pool)
            .await
            .expect("read repaired anchors");
    assert_eq!(
        anchors,
        [
            Some("git:github.com/fs3/anchored".to_string()),
            None,
            Some("https://github.com/fs3/foreign.git".to_string()),
        ],
        "registered raw anchors converge while null and foreign pointers survive"
    );

    database.destroy(pool).await;
}
