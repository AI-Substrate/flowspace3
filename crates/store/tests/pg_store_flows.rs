//! The flows the store exists to perform, each on its own throwaway database.
//!
//! Throwaway rather than a unique key in the shared database, because two of
//! these flows are not key-scoped at all: `claim_job` takes the best ready job
//! in the WHOLE table, and `query_embeddings` ranks over every vector in it. A
//! concurrent test's rows would not merely coexist — they would be candidate
//! answers.
//!
//! Each test names the workshop 002 decision it is defending, because that is
//! what a failure here actually means.

mod support;

use fs3_core::{
    Element, ElementKind, Embedder, RepoIdentity, Span, Summarizer, Summary, content_hash,
};
use fs3_store::{
    EMBEDDING_DIMENSIONS, NewEmbedding, PgPool, SearchFilters, SourceKind, StoreError, claim_job,
    complete_job, enqueue_job, existing_embedding_hashes, fail_job, get_smart_content,
    missing_embeddings, missing_enrichment, put_embeddings, put_smart_content, query_embeddings,
    register_worktree, requeue_failed, search_elements, sync_worktree_files, upsert_element_tree,
};
use fs3_testkit::fakes::{FakeEmbedder, FakeSummarizer};
use std::collections::BTreeMap;
use std::time::Duration;
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

const SUMMARIZER: &str = "fake-summarizer@v1";
const EMBEDDER: &str = "fake-embedder@v1";

/// A file element with one child per body, addressed under `path`.
fn file_with(path: &str, bodies: &[&str]) -> Element {
    let children = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| {
            let line = index as u32 * 4 + 3;
            Element::new(
                ElementKind::Function,
                "function_item",
                format!("f{index}"),
                format!("{path}::f{index}"),
                Span::new(line, line + 2),
                *body,
            )
            .with_sibling_order(index as u32)
        })
        .collect();

    Element::new(
        ElementKind::File,
        "rust",
        path.rsplit('/').next().unwrap_or(path),
        path,
        Span::new(1, bodies.len() as u32 * 4 + 4),
        format!("// {path}\n"),
    )
    .with_children(children)
}

/// Only the declarations earn enrichment — the file root is the container, not
/// the content. This stands in for the scanner's injected policy (decision D5).
fn declarations_only(element: &Element) -> bool {
    element.kind != ElementKind::File
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(pool)
        .await
        .expect("counting should succeed")
}

// ── Decision D2: enrichment is content-addressed ────────────────────────────

/// The same function body in two different files is ONE piece of enrichment.
///
/// This is decision D2's entire payoff: the same method on forty branches is
/// summarised once. If this test fails, the system is paying an LLM per copy.
#[tokio::test]
async fn one_body_in_two_blobs_is_one_smart_row_and_one_piece_of_work() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let shared = "pub fn area(&self) -> u32 { self.w * self.h }";
    let alpha = file_with("src/alpha.rs", &[shared]);
    let beta = file_with("src/beta.rs", &[shared]);
    let raw_hash = alpha.children[0].raw_hash().to_string();
    assert_eq!(
        raw_hash,
        beta.children[0].raw_hash(),
        "identical text must hash identically, or nothing below is true"
    );

    upsert_element_tree(
        &pool,
        &unique_blob(),
        PARSER_VERSION,
        &alpha,
        declarations_only,
    )
    .await
    .expect("insert alpha");
    upsert_element_tree(
        &pool,
        &unique_blob(),
        PARSER_VERSION,
        &beta,
        declarations_only,
    )
    .await
    .expect("insert beta");

    // Two element rows for that body, in two blobs.
    assert_eq!(
        count(
            &pool,
            &format!("SELECT count(*) FROM elements WHERE raw_hash = '{raw_hash}'")
        )
        .await,
        2
    );

    // ...but the reconciler sees ONE job. Deduplicating here is what stops the
    // sweep from enqueueing the same LLM call once per branch.
    let missing = missing_enrichment(&pool, SUMMARIZER, 10)
        .await
        .expect("sweep");
    assert_eq!(
        missing.len(),
        1,
        "the same body in two blobs is one piece of work, got {missing:#?}"
    );
    assert_eq!(missing[0].raw_hash, raw_hash);
    assert_eq!(
        missing[0].element.raw_text, shared,
        "the sweep carries the element to summarise, not just its hash: the \
         summariser reads a declaration's kind, name, address and span too"
    );
    assert_eq!(missing[0].element.kind, ElementKind::Function);
    assert_eq!(missing[0].element.address, "src/alpha.rs::f0");

    let summary = FakeSummarizer::default()
        .summarize(&alpha.children[0])
        .await
        .expect("the fake summariser does not fail");
    put_smart_content(&pool, &raw_hash, SUMMARIZER, &summary)
        .await
        .expect("write the summary");

    // One write covered both elements: the sweep is now empty, and the summary
    // is reachable from either copy because neither copy is the key.
    assert!(
        missing_enrichment(&pool, SUMMARIZER, 10)
            .await
            .expect("sweep")
            .is_empty(),
        "a stored summary is what makes an element clean — there is no flag to set"
    );
    assert!(
        !summary.extras.is_empty(),
        "this assertion is only worth making while the fake returns a field \
         outside the typed contract — it is what proves extras are persisted"
    );
    assert_eq!(
        get_smart_content(&pool, beta.children[0].raw_hash(), SUMMARIZER)
            .await
            .expect("read"),
        Some(summary),
        "beta never had its own summary written, and does not need one — and the \
         summary must come back WHOLE, extras included, or the store is dropping \
         the fields the type promises to keep"
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM smart_content").await,
        1,
        "one body, one row — no matter how many places hold it"
    );

    // A model bump is a new key: the old row is untouched (instant rollback)
    // and the work reappears under the new one.
    assert_eq!(
        missing_enrichment(&pool, "fake-summarizer@v2", 10)
            .await
            .expect("sweep")
            .len(),
        1,
        "changing the model must resurface the work without erasing the old answer"
    );

    database.destroy(pool).await;
}

/// The recovery sweep: content with no vector is found in BOTH spaces, and
/// stops being found the moment the vector lands.
///
/// This is what heals an index whose `embed` jobs a GC pass reaped before they
/// ran — a defect that left elements, summaries and an empty queue all looking
/// healthy while the content was absent from every semantic search. Nothing
/// stored recorded the loss, so the backlog has to be derived from the absence
/// of a vector row, exactly as `missing_enrichment` derives from the absence of
/// a summary.
#[tokio::test]
async fn the_vector_sweep_finds_both_spaces_and_forgets_what_lands() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let body = "fn unvectored() { never_bought() }";
    let raw_hash = content_hash(body.as_bytes());
    upsert_element_tree(
        &pool,
        &unique_blob(),
        PARSER_VERSION,
        &file_with("src/a.rs", &[body]),
        declarations_only,
    )
    .await
    .expect("insert");

    let summary_text = "buys nothing";
    put_smart_content(
        &pool,
        &raw_hash,
        SUMMARIZER,
        &Summary {
            text: summary_text.to_string(),
            tags: vec!["unvectored".to_string()],
            extras: BTreeMap::new(),
        },
    )
    .await
    .expect("write the summary");

    let missing = missing_embeddings(&pool, EMBEDDER, 50)
        .await
        .expect("sweep");

    // The declaration's raw text and the summary's text, and NOT the file root:
    // a file element with parsed children does not earn a vector, because its
    // text is the concatenation of texts already indexed one by one.
    let raw: Vec<&str> = missing
        .iter()
        .filter(|item| item.source_kind == SourceKind::Raw)
        .map(|item| item.text.as_str())
        .collect();
    assert_eq!(
        raw,
        vec![body],
        "the file root must not be swept: it would compete with its own parts"
    );

    let smart: Vec<&str> = missing
        .iter()
        .filter(|item| item.source_kind == SourceKind::Smart)
        .map(|item| item.text.as_str())
        .collect();
    assert_eq!(
        smart,
        vec![summary_text],
        "a summary with no vector is unreachable by semantic search — the whole \
         point of having paid for it"
    );

    // Buy the raw vector. Only that one stops being missing.
    let vector = FakeEmbedder {
        dimensions: EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    }
    .embed(&[body.to_string()])
    .await
    .expect("the fake embedder does not fail");
    put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: &raw_hash,
            source_kind: SourceKind::Raw,
            vector: &vector[0],
            truncated: false,
        }],
    )
    .await
    .expect("write the vector");

    let missing = missing_embeddings(&pool, EMBEDDER, 50)
        .await
        .expect("sweep");
    assert_eq!(
        missing.len(),
        1,
        "a stored vector is what makes content clean — there is no flag to set"
    );
    assert_eq!(missing[0].source_kind, SourceKind::Smart);

    // A model bump is a new space, so the work reappears under the new key
    // without erasing the old answer — the same property the summary sweep has.
    assert_eq!(
        missing_embeddings(&pool, "fake-embedder@v2", 50)
            .await
            .expect("sweep")
            .len(),
        2
    );

    database.destroy(pool).await;
}

/// PRD req 36's tag band is enforced by the database too, not only by the type.
#[tokio::test]
async fn the_schema_refuses_a_summary_with_too_many_tags() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let error = put_smart_content(
        &pool,
        &content_hash(b"whatever"),
        SUMMARIZER,
        &Summary {
            text: "six tags is one too many".into(),
            tags: (0..6).map(|n| format!("tag{n}")).collect(),
            ..Summary::default()
        },
    )
    .await
    .expect_err("six tags must be refused");

    let StoreError::Query(sqlx::Error::Database(database_error)) = &error else {
        panic!("expected a database error carrying a SQLSTATE, got: {error}");
    };
    assert_eq!(
        database_error.constraint(),
        Some("smart_content_tag_band"),
        "the tag band must be the constraint that bit: {error}"
    );

    database.destroy(pool).await;
}

/// A provider field outside the typed contract survives PERSISTENCE, not just
/// deserialisation.
///
/// `Summary::extras` exists so that a JSON member the type has never heard of
/// is captured rather than dropped. A store with nowhere to put it moved the
/// drop one layer down — from the wire to the database — where nothing
/// complains and the type's own doc comment is quietly untrue. Found by
/// pij-broad-sawfish integrating plan 003.
#[tokio::test]
async fn a_provider_field_outside_the_typed_contract_survives_the_store() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    // The shapes a real provider actually returns: a number, a string, a bool,
    // a nested object, an array — and a null, which must survive as a stored
    // null rather than becoming an absent key.
    let rich = Summary {
        text: "summarises a parser".into(),
        tags: vec!["parser".into()],
        extras: BTreeMap::from([
            ("complexity".to_string(), serde_json::json!(7)),
            ("risk".to_string(), serde_json::json!("low")),
            ("deprecated".to_string(), serde_json::json!(false)),
            ("cost".to_string(), serde_json::json!({ "tokens": 812 })),
            ("callers".to_string(), serde_json::json!(["a", "b"])),
            ("owner".to_string(), serde_json::Value::Null),
        ]),
    };
    let raw_hash = content_hash(b"fn parse() {}");

    put_smart_content(&pool, &raw_hash, SUMMARIZER, &rich)
        .await
        .expect("write");
    assert_eq!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("read"),
        Some(rich.clone()),
        "every extra must come back exactly as it went in"
    );

    // Re-summarising overwrites extras rather than merging them: a field the
    // new prompt stopped returning must not linger as a ghost of the old one.
    let leaner = Summary {
        text: rich.text.clone(),
        tags: rich.tags.clone(),
        extras: BTreeMap::from([("complexity".to_string(), serde_json::json!(2))]),
    };
    put_smart_content(&pool, &raw_hash, SUMMARIZER, &leaner)
        .await
        .expect("rewrite");
    assert_eq!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("read"),
        Some(leaner),
        "the upsert must replace extras, not accumulate them"
    );

    // The fork, asserted rather than left implicit: `text_hash` addresses the
    // TEXT. Extras are not folded into it, so adding a provider field does not
    // re-key the summary — and therefore does not orphan the vector that
    // resolves through it, or buy a re-embed for a change to something that
    // was never embedded.
    let hashes: Vec<String> = sqlx::query_scalar(
        "SELECT text_hash FROM smart_content WHERE raw_hash = $1 OR raw_hash = $2",
    )
    .bind(&raw_hash)
    .bind(content_hash(b"a different body"))
    .fetch_all(&pool)
    .await
    .expect("read the digests");
    assert_eq!(
        hashes,
        vec![content_hash(rich.text.as_bytes())],
        "text_hash is sha256 of the summary text and nothing else"
    );

    database.destroy(pool).await;
}

// ── Decision D1: the queue IS the dirty-file list ───────────────────────────

/// A re-fire pushes the deadline out instead of adding a row: the debounce.
#[tokio::test]
async fn a_re_fire_collapses_into_the_queued_job_and_pushes_its_deadline_out() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let key = "scan:wt1:src/a.rs";
    enqueue_job(
        &pool,
        "scan_file",
        key,
        &serde_json::json!({ "path": "src/a.rs" }),
        Duration::ZERO,
    )
    .await
    .expect("first enqueue");

    // The same file saved again while the first job is still waiting.
    enqueue_job(
        &pool,
        "scan_file",
        key,
        &serde_json::json!({ "path": "src/a.rs" }),
        Duration::from_secs(60),
    )
    .await
    .expect("second enqueue");

    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs").await,
        1,
        "a second enqueue of a live dedupe_key must not add a row"
    );
    assert_eq!(
        claim_job(&pool, &["scan_file"]).await.expect("claim"),
        None,
        "the re-fire pushed not_before out, so nothing is ready — that is the \
         debounce, and it lives in the column rather than in a timer"
    );

    database.destroy(pool).await;
}

/// The full lifecycle, and the reason the unique index is partial: finishing a
/// job releases its key so the next edit can queue again.
#[tokio::test]
async fn a_job_is_claimed_once_and_completing_it_frees_its_key() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let key = "scan:wt1:src/b.rs";
    let payload = serde_json::json!({ "path": "src/b.rs", "worktree": 1 });
    enqueue_job(&pool, "scan_file", key, &payload, Duration::ZERO)
        .await
        .expect("enqueue");

    let job = claim_job(&pool, &["scan_file"])
        .await
        .expect("claim")
        .expect("a ready job should be claimable");
    assert_eq!(job.dedupe_key, key);
    assert_eq!(
        job.payload, payload,
        "the worker gets its arguments back intact"
    );
    assert_eq!(job.attempts, 1);

    assert_eq!(
        claim_job(&pool, &["scan_file"]).await.expect("claim"),
        None,
        "a running job must not be claimable a second time"
    );
    assert_eq!(
        claim_job(&pool, &["summarize", "embed"])
            .await
            .expect("claim"),
        None,
        "a worker must only take the kinds it asked for"
    );

    complete_job(&pool, job.id).await.expect("complete");

    // The key is free again — the file can be edited and re-queued.
    enqueue_job(&pool, "scan_file", key, &payload, Duration::ZERO)
        .await
        .expect("re-enqueue after completion");
    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs").await,
        2,
        "a finished job is history, not a block on the next edit"
    );

    database.destroy(pool).await;
}

/// A failure is recorded on the row, not swallowed.
#[tokio::test]
async fn failing_a_job_records_why() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    enqueue_job(
        &pool,
        "summarize",
        "summarize:deadbeef",
        &serde_json::json!({}),
        Duration::ZERO,
    )
    .await
    .expect("enqueue");
    let job = claim_job(&pool, &["summarize"])
        .await
        .expect("claim")
        .expect("ready");

    fail_job(&pool, job.id, "provider returned 429", false)
        .await
        .expect("fail");

    let (state, last_error): (String, Option<String>) =
        sqlx::query_as("SELECT state, last_error FROM jobs WHERE id = $1")
            .bind(job.id)
            .fetch_one(&pool)
            .await
            .expect("read the settled row");
    assert_eq!(state, "failed");
    assert_eq!(last_error.as_deref(), Some("provider returned 429"));

    // A 429 is not the job's fault and not a defect in it, so the row stays
    // revivable: `requeue_failed` is allowed to wake it after a fix.
    let terminal: bool = sqlx::query_scalar("SELECT terminal FROM jobs WHERE id = $1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .expect("read the terminal flag");
    assert!(!terminal, "a retryable failure must stay revivable");

    database.destroy(pool).await;
}

/// The boot heal retires only failed embed jobs whose every text is blank.
#[tokio::test]
async fn empty_embed_job_heal_is_terminal_counted_and_refuses_real_text() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let poison_payload = serde_json::json!({
        "identity": "github.com/acme/repo",
        "source": "raw",
        "items": [["not-the-empty-hash", ""], ["also-not-empty-hash", " \t\n\u{2003}"]]
    });
    let real_payload = serde_json::json!({
        "identity": "github.com/acme/repo",
        "source": "raw",
        "items": [["e3b0c44298fc1c149afbf4c8996fb924", "real text"]]
    });
    let poison_id: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, attempts, last_error)
         VALUES ('embed', 'embed:blank', $1, 'failed', 3, 'empty provider input')
         RETURNING id",
    )
    .bind(poison_payload)
    .fetch_one(&pool)
    .await
    .expect("seed blank failed embed job");
    let real_id: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, dedupe_key, payload, state, attempts, last_error)
         VALUES ('embed', 'embed:real', $1, 'failed', 3, 'provider unavailable')
         RETURNING id",
    )
    .bind(real_payload)
    .fetch_one(&pool)
    .await
    .expect("seed real-text failed embed job");

    let retired = fs3_store::jobs::retire_empty_embed_jobs(&pool)
        .await
        .expect("heal empty embed jobs");
    assert_eq!(retired, 1, "the count is the boot receipt");

    let swept = requeue_failed(&pool, &["embed"])
        .await
        .expect("run the real boot sweep");
    assert_eq!(swept, 1, "the real-text control must remain revivable");

    let poison: (String, bool) = sqlx::query_as("SELECT state, terminal FROM jobs WHERE id = $1")
        .bind(poison_id)
        .fetch_one(&pool)
        .await
        .expect("read healed poison job");
    assert_eq!(poison, ("failed".to_string(), true));
    let control: (String, bool) = sqlx::query_as("SELECT state, terminal FROM jobs WHERE id = $1")
        .bind(real_id)
        .fetch_one(&pool)
        .await
        .expect("read real-text control job");
    assert_eq!(control, ("pending".to_string(), false));

    database.destroy(pool).await;
}

// ── Decision D4: FOR UPDATE SKIP LOCKED ─────────────────────────────────────

/// A row another worker holds is STEPPED OVER, not waited on.
///
/// Without `SKIP LOCKED` this test would deadline out rather than fail an
/// assertion: the claim would sit behind the held lock until the transaction
/// below released it. That is precisely the serialisation decision D4 exists to
/// avoid, and the reason an LLM job and an embedding job can run at once.
#[tokio::test]
async fn a_locked_job_is_stepped_over_not_waited_on() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    for name in ["src/one.rs", "src/two.rs"] {
        enqueue_job(
            &pool,
            "scan_file",
            &format!("scan:wt1:{name}"),
            &serde_json::json!({ "path": name }),
            Duration::ZERO,
        )
        .await
        .expect("enqueue");
    }

    // Whichever job `claim_job` would take next — asked for in its own order,
    // so this test does not depend on insertion order happening to match.
    let mut holder = pool.begin().await.expect("begin");
    let held: i64 = sqlx::query_scalar(
        "SELECT id FROM jobs
          WHERE state = 'pending' AND not_before <= now()
          ORDER BY priority DESC, not_before
          FOR UPDATE
          LIMIT 1",
    )
    .fetch_one(&mut *holder)
    .await
    .expect("lock the front of the queue");

    let claimed = claim_job(&pool, &["scan_file"])
        .await
        .expect("claim")
        .expect("the second job is still free");
    assert_ne!(
        claimed.id, held,
        "a locked row must be skipped, never handed to a second worker"
    );

    holder.rollback().await.expect("release the lock");

    // Released, the skipped-over job is claimable again — the lock deferred it,
    // it did not consume it.
    let after = claim_job(&pool, &["scan_file"])
        .await
        .expect("claim")
        .expect("the released job is ready again");
    assert_eq!(after.id, held);

    database.destroy(pool).await;
}

/// Two workers polling at the same instant get two different jobs.
#[tokio::test]
async fn two_claimers_never_take_the_same_job() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    for name in ["src/one.rs", "src/two.rs"] {
        enqueue_job(
            &pool,
            "scan_file",
            &format!("scan:wt1:{name}"),
            &serde_json::json!({ "path": name }),
            Duration::ZERO,
        )
        .await
        .expect("enqueue");
    }

    let (first, second) = tokio::join!(
        claim_job(&pool, &["scan_file"]),
        claim_job(&pool, &["scan_file"])
    );
    let first = first.expect("claim").expect("two jobs are ready");
    let second = second.expect("claim").expect("two jobs are ready");

    assert_ne!(
        first.id, second.id,
        "two concurrent claimers took the same job — the same file would be \
         scanned twice and the same LLM call paid for twice"
    );
    assert_eq!(
        claim_job(&pool, &["scan_file"]).await.expect("claim"),
        None,
        "both jobs are now running, so a third worker finds nothing"
    );

    database.destroy(pool).await;
}

/// One source may own many chunks, while rewriting one chunk remains an upsert.
#[tokio::test]
async fn embedding_chunks_round_trip_and_same_chunk_upserts() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let source_hash = content_hash(b"one source split into overlapping chunks");
    let zero = vec![0.0; EMBEDDING_DIMENSIONS];
    let mut changed = zero.clone();
    changed[0] = 1.0;

    put_embeddings(
        &pool,
        EMBEDDER,
        &[
            NewEmbedding {
                source_hash: &source_hash,
                source_kind: SourceKind::Raw,
                chunk_no: 0,
                vector: &zero,
                truncated: false,
            },
            NewEmbedding {
                source_hash: &source_hash,
                source_kind: SourceKind::Raw,
                chunk_no: 1,
                vector: &zero,
                truncated: false,
            },
        ],
    )
    .await
    .expect("write both chunks atomically");
    put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            source_hash: &source_hash,
            source_kind: SourceKind::Raw,
            chunk_no: 1,
            vector: &changed,
            truncated: false,
        }],
    )
    .await
    .expect("rewrite one chunk");

    let chunks: Vec<(i16, bool)> = sqlx::query_as(
        "SELECT chunk_no, vector::text LIKE '[1,%'
           FROM embeddings_1024
          WHERE source_hash = $1 AND source_kind = 'raw' AND model_key = $2
          ORDER BY chunk_no",
    )
    .bind(&source_hash)
    .bind(EMBEDDER)
    .fetch_all(&pool)
    .await
    .expect("read stored chunks");
    assert_eq!(chunks, [(0, false), (1, true)]);
    assert_eq!(
        existing_embedding_hashes(&pool, EMBEDDER, SourceKind::Raw, &[&source_hash])
            .await
            .expect("run hash-level pre-check"),
        [source_hash].into_iter().collect(),
        "many chunks still collapse to one completed source hash"
    );

    database.destroy(pool).await;
}

// ── Search: HNSW, then the join back to the element ─────────────────────────

/// Nearest first, and a summary hit resolves to the element it describes.
#[tokio::test]
async fn similarity_ranks_nearest_first_and_resolves_back_to_elements() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let bodies = [
        "pub fn parse_markdown(input: &str) -> Document { markdown::parse(input) }",
        "pub fn connect_database(url: &str) -> Pool { postgres::pool(url) }",
        "pub fn render_template(name: &str) -> Html { templates::render(name) }",
    ];
    let file = file_with("src/lib.rs", &bodies);
    upsert_element_tree(
        &pool,
        &unique_blob(),
        PARSER_VERSION,
        &file,
        declarations_only,
    )
    .await
    .expect("insert");

    // The fake is the testkit's, widened to the table's dimension. Its vectors
    // are token-hashed, so cosine ordering carries real signal — which is the
    // only reason a similarity assertion over a fake means anything.
    let embedder = FakeEmbedder {
        dimensions: EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    };
    let texts: Vec<String> = bodies.iter().map(|body| (*body).to_string()).collect();
    let vectors = embedder
        .embed(&texts)
        .await
        .expect("the fake does not fail");

    let raw: Vec<NewEmbedding<'_>> = file
        .children
        .iter()
        .zip(&vectors)
        .map(|(element, vector)| NewEmbedding {
            chunk_no: 0,
            source_hash: element.raw_hash(),
            source_kind: SourceKind::Raw,
            vector,
            truncated: false,
        })
        .collect();
    put_embeddings(&pool, EMBEDDER, &raw)
        .await
        .expect("write vectors");

    // Search for the second body's own text: it must come back first, at
    // distance zero, because the fake is deterministic.
    let hits = query_embeddings(&pool, EMBEDDER, &vectors[1], 3)
        .await
        .expect("query");
    assert_eq!(hits.len(), 3);
    assert_eq!(
        hits[0].element.address,
        file.children[1].address,
        "the exact text must rank first; got {:#?}",
        hits.iter()
            .map(|hit| &hit.element.address)
            .collect::<Vec<_>>()
    );
    assert!(
        hits[0].distance < 1e-6,
        "a vector matched against itself is distance zero, got {}",
        hits[0].distance
    );
    assert!(
        hits.windows(2)
            .all(|pair| pair[0].distance <= pair[1].distance),
        "results must be nearest-first: {:?}",
        hits.iter().map(|hit| hit.distance).collect::<Vec<_>>()
    );
    assert_eq!(hits[0].source_kind, SourceKind::Raw);
    assert_eq!(hits[0].smart, None, "a raw hit has no summary attached");
    assert_eq!(hits[0].element.raw_text, bodies[1]);

    // Now the smart leg. A summary is embedded under the digest of its own
    // text, and `smart_content.text_hash` is the only way back from that vector
    // to the element — the join this schema exists to make possible.
    let third = &file.children[2];
    let summary = FakeSummarizer::default()
        .summarize(third)
        .await
        .expect("the fake does not fail");
    put_smart_content(&pool, third.raw_hash(), SUMMARIZER, &summary)
        .await
        .expect("write the summary");

    let summary_vector = embedder
        .embed(std::slice::from_ref(&summary.text))
        .await
        .expect("the fake does not fail")
        .remove(0);
    put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: &content_hash(summary.text.as_bytes()),
            source_kind: SourceKind::Smart,
            vector: &summary_vector,
            truncated: false,
        }],
    )
    .await
    .expect("write the summary vector");

    let hits = query_embeddings(&pool, EMBEDDER, &summary_vector, 1)
        .await
        .expect("query");
    assert_eq!(hits[0].source_kind, SourceKind::Smart);
    assert_eq!(
        hits[0].element.address, third.address,
        "a summary vector must resolve to the element it describes"
    );
    assert_eq!(
        hits[0].smart,
        Some(summary),
        "and carry the summary itself — WHOLE. The search path builds its own \
         Summary from a joined row, so it is the second place extras can be \
         dropped, and the one where nobody would notice"
    );
    assert_eq!(hits[0].parser_version, PARSER_VERSION);

    database.destroy(pool).await;
}

/// The FILTERED search surface carries extras too — the daemon's `/search` is
/// this function, not `query_embeddings`.
///
/// `search_elements` builds its `Summary` from a different SQL statement with a
/// different lateral join, so "extras are persisted" being true of
/// `get_smart_content` says nothing about it. That gap was the live one: the
/// integrator's surface would have been the one still lying, and nothing in
/// plan 003 reads extras yet, so nobody would have noticed until something did.
#[tokio::test]
async fn the_filtered_search_surface_carries_extras_and_a_live_path() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let blob = unique_blob();
    let file = file_with(
        "src/lib.rs",
        &["pub fn parse(input: &str) -> Document { todo!() }"],
    );
    upsert_element_tree(&pool, &blob, PARSER_VERSION, &file, declarations_only)
        .await
        .expect("insert elements");

    // The ref layer, so the hit can resolve to somewhere it actually lives.
    let identity = RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3")
        .expect("a host and a path make an identity");
    let worktree = register_worktree(&pool, &identity, "/tmp/flowspace3", Some("main"))
        .await
        .expect("register");
    sync_worktree_files(&pool, worktree, &[("src/lib.rs".to_string(), blob.clone())])
        .await
        .expect("sync");

    let element = &file.children[0];
    let summary = FakeSummarizer::default()
        .summarize(element)
        .await
        .expect("the fake does not fail");
    assert!(
        !summary.extras.is_empty(),
        "this test is only worth running while the fake returns a field outside \
         the typed contract"
    );
    put_smart_content(&pool, element.raw_hash(), SUMMARIZER, &summary)
        .await
        .expect("write the summary");

    let embedder = FakeEmbedder {
        dimensions: EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    };
    let vector = embedder
        .embed(std::slice::from_ref(&summary.text))
        .await
        .expect("the fake does not fail")
        .remove(0);
    put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: &content_hash(summary.text.as_bytes()),
            source_kind: SourceKind::Smart,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .expect("write the summary vector");

    let hits = search_elements(
        &pool,
        EMBEDDER,
        &vector,
        &SearchFilters {
            repo: Some(identity.key().to_string()),
            path: Some("src/%".to_string()),
            limit: 1,
            ..SearchFilters::default()
        },
    )
    .await
    .expect("search");

    let hit = hits.first().expect("the summary vector is an exact match");
    assert_eq!(
        hit.similar.smart,
        Some(summary),
        "a filtered search hit must carry the WHOLE summary — this is the path \
         the daemon serves, and it decodes its own row"
    );
    assert_eq!(hit.identity.as_deref(), Some(identity.key()));
    assert_eq!(hit.path.as_deref(), Some("src/lib.rs"));

    database.destroy(pool).await;
}

/// A vector of the wrong width is refused on the caller's terms, before any
/// round trip — the fix is another table (decision D3), not a retry.
#[tokio::test]
async fn a_vector_of_the_wrong_width_is_refused_by_name() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let narrow = vec![0.5f32; 32];
    let error = put_embeddings(
        &pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash: &content_hash(b"anything"),
            source_kind: SourceKind::Raw,
            vector: &narrow,
            truncated: false,
        }],
    )
    .await
    .expect_err("a 32-wide vector does not belong in embeddings_1024");
    assert!(
        matches!(
            error,
            StoreError::Dimensions {
                expected: EMBEDDING_DIMENSIONS,
                actual: 32
            }
        ),
        "{error}"
    );

    let error = query_embeddings(&pool, EMBEDDER, &narrow, 5)
        .await
        .expect_err("nor in a query against it");
    assert!(matches!(error, StoreError::Dimensions { .. }), "{error}");

    database.destroy(pool).await;
}
