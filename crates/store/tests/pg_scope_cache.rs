//! The scope cache never hides content: a search that reads a cached scope
//! returns exactly what one that collects the scope in full returns, after
//! every kind of write that can grow a scope.

mod support;

use fs3_core::{
    BlobRef, Conversation, ConversationId, Element, ElementKind, RepoIdentity, Span, Summary, Turn,
    TurnRole, TurnSource, content_hash,
};
use fs3_store::{
    EMBEDDING_DIMENSIONS, NewEmbedding, PgPool, ScopeCacheUse, SearchFilters, SearchPage,
    SourceKind, append_turns, normalise_conversation_identities, put_embeddings, put_smart_content,
    register_worktree, search_elements, sync_worktree_files, upsert_conversation,
    upsert_element_tree,
};
use fs3_testkit::FreshDatabase;
use support::{PARSER_VERSION, unique_blob};

const EMBEDDER: &str = "scope-cache-fixture@1024";

/// These tests run one at a time. A cached scope's cutoff is the oldest
/// transaction in flight anywhere in the cluster, so another test's open
/// write would make every delta here larger: still correct, but large enough
/// to read a write a missing stamp should have hidden, and so to prove
/// nothing about that stamp.
static SERIAL: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
const SUMMARIZER: &str = "scope-cache-summary@v1";
const ALPHA_ROOT: &str = "/fixtures/alpha";
const BETA_ROOT: &str = "/fixtures/beta";
/// The parser version searches answer with, one newer than the fixture's.
const REPARSED: &str = "test-parser@2";

/// A vector `y` away from the query along one axis: a smaller `y` is nearer.
fn vector(y: f32) -> Vec<f32> {
    let mut vector = vec![0.0; EMBEDDING_DIMENSIONS];
    vector[0] = 1.0;
    vector[1] = y;
    vector
}

async fn embed(pool: &PgPool, source_hash: &str, source_kind: SourceKind, y: f32) {
    let vector = vector(y);
    put_embeddings(
        pool,
        EMBEDDER,
        &[NewEmbedding {
            chunk_no: 0,
            source_hash,
            source_kind,
            vector: &vector,
            truncated: false,
        }],
    )
    .await
    .expect("store a vector");
}

/// A parsed file blob with one function element and a vector, not yet on
/// any path.
async fn blob(pool: &PgPool, name: &str, y: f32) -> (BlobRef, String) {
    let blob = unique_blob();
    let body = format!("fn {name}() {{ scope_cache_fixture(); }}");
    let child = Element::new(
        ElementKind::Function,
        "scope_cache_fixture",
        name,
        format!("{name}.rs::{name}"),
        Span::new(1, 1),
        &body,
    );
    let raw_hash = child.raw_hash().to_string();
    let root = Element::new(
        ElementKind::File,
        "fixture",
        format!("{name}.rs"),
        format!("{name}.rs"),
        Span::new(1, 1),
        &body,
    )
    .with_children(vec![child]);
    upsert_element_tree(pool, &blob, PARSER_VERSION, &root, |_| false)
        .await
        .expect("store a file blob");
    embed(pool, &raw_hash, SourceKind::Raw, y).await;
    (blob, raw_hash)
}

fn turn(turn_no: u32, body: &str) -> Turn {
    Turn {
        turn_no,
        role: TurnRole::Human,
        source: TurnSource::Peer,
        head_sha: None,
        at: "2026-10-15T00:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::new(),
    }
}

async fn conversation(pool: &PgPool, guid: &str, repo: Option<&RepoIdentity>) -> ConversationId {
    let id = ConversationId::new(guid).expect("a conversation id");
    upsert_conversation(
        pool,
        &Conversation {
            guid: id.clone(),
            repo_identity: repo.map(|repo| repo.key().to_string()),
            // Each repository's own root: `upsert_conversation` anchors a
            // conversation to whichever repository its worktree belongs to.
            worktree: repo.map(|repo| {
                if repo.key().ends_with("alpha") {
                    ALPHA_ROOT.to_string()
                } else {
                    BETA_ROOT.to_string()
                }
            }),
            base_sha: None,
            title: None,
            started_at: "2026-10-15T00:00:00Z".to_string(),
            parent: None,
        },
    )
    .await
    .expect("store a conversation");
    id
}

/// Append one turn and give it a vector; returns its raw hash.
async fn say(pool: &PgPool, id: &ConversationId, turn_no: u32, body: &str, y: f32) -> String {
    append_turns(pool, id, &[turn(turn_no, body)], |_| false)
        .await
        .expect("append a turn");
    let raw_hash = turn(turn_no, body).element(id).raw_hash().to_string();
    embed(pool, &raw_hash, SourceKind::Raw, y).await;
    raw_hash
}

struct Fixture {
    database: FreshDatabase,
    pool: PgPool,
    alpha: RepoIdentity,
    beta: RepoIdentity,
    alpha_worktree: i64,
    alpha_files: Vec<(String, BlobRef)>,
}

impl Fixture {
    async fn new(label: &str) -> Self {
        Self::with_settings(label, &[]).await
    }

    /// A fixture whose database carries `settings` from its first connection.
    async fn with_settings(label: &str, settings: &[&str]) -> Self {
        let database = FreshDatabase::create_from(&fs3_testkit::test_database_url(), label)
            .await
            .expect("create a migrated database");
        if !settings.is_empty() {
            let setup = database.pool().await;
            for setting in settings {
                sqlx::query(&format!(
                    "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I SET {setting}', current_database()); END $$;"
                ))
                .execute(&setup)
                .await
                .expect("apply a database setting");
            }
            setup.close().await;
        }
        let pool = database.pool().await;
        let alpha = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/alpha").unwrap();
        let beta = RepoIdentity::from_remote_parts(Some("github.com"), "fixtures/beta").unwrap();
        let alpha_worktree = register_worktree(&pool, &alpha, ALPHA_ROOT, Some("main"))
            .await
            .unwrap();
        register_worktree(&pool, &beta, BETA_ROOT, Some("main"))
            .await
            .unwrap();
        let mut fixture = Self {
            database,
            pool,
            alpha,
            beta,
            alpha_worktree,
            alpha_files: Vec::new(),
        };
        for (index, name) in ["alpha_one", "alpha_two"].into_iter().enumerate() {
            let (blob, _) = blob(&fixture.pool, name, 0.1 + index as f32 * 0.01).await;
            fixture.alpha_files.push((format!("src/{name}.rs"), blob));
        }
        fixture.sync().await;
        fixture
    }

    async fn sync(&self) {
        sync_worktree_files(&self.pool, self.alpha_worktree, &self.alpha_files)
            .await
            .expect("sync alpha's paths");
    }

    /// The scopes the property is checked over.
    fn scopes(&self) -> Vec<(&'static str, SearchFilters)> {
        let repo = SearchFilters {
            repo: Some(self.alpha.key().to_string()),
            limit: 50,
            // As the daemon always names its own: blobs parsed only under
            // the fixture's older version still answer, and a reparse under
            // this one supersedes them.
            parser_version: Some(REPARSED.to_string()),
            ..SearchFilters::default()
        };
        vec![
            ("repo, every kind", repo.clone()),
            (
                "repo, code",
                SearchFilters {
                    kinds: Some(vec![
                        ElementKind::File,
                        ElementKind::Container,
                        ElementKind::Function,
                    ]),
                    ..repo.clone()
                },
            ),
            (
                "worktree",
                SearchFilters {
                    repo: None,
                    worktree: Some(ALPHA_ROOT.to_string()),
                    ..repo.clone()
                },
            ),
            (
                "repo, turns",
                SearchFilters {
                    kinds: Some(vec![ElementKind::Turn]),
                    ..repo.clone()
                },
            ),
            (
                "path",
                SearchFilters {
                    path: Some("src/%".to_string()),
                    ..repo
                },
            ),
        ]
    }

    async fn page(&self, filters: &SearchFilters, cache: bool) -> SearchPage {
        search_elements(
            &self.pool,
            EMBEDDER,
            &vector(0.0),
            &SearchFilters {
                scope_cache: cache,
                ..filters.clone()
            },
        )
        .await
        .expect("search")
    }

    async fn hits(&self, filters: &SearchFilters, cache: bool) -> (Vec<String>, ScopeCacheUse) {
        let page = search_elements(
            &self.pool,
            EMBEDDER,
            &vector(0.0),
            &SearchFilters {
                scope_cache: cache,
                ..filters.clone()
            },
        )
        .await
        .expect("search");
        let hits = page
            .hits
            .iter()
            .map(|hit| {
                format!(
                    "{} {:?} {:.6}",
                    hit.similar.element.address, hit.path, hit.similar.distance
                )
            })
            .collect();
        (hits, page.scope_cache)
    }

    /// Fill every scope's cache, so the checks after a write read through it.
    async fn warm(&self) {
        for (label, filters) in self.scopes() {
            let (_, used) = self.hits(&filters, true).await;
            assert!(
                matches!(used, ScopeCacheUse::Built | ScopeCacheUse::Hit),
                "{label}: the cache was not filled ({used:?})"
            );
        }
    }

    /// Drop every cached scope and build them again, so the next write is read
    /// through its own delta path alone, not one an earlier write opened.
    async fn rebuild(&self) {
        sqlx::query("DELETE FROM scope_cache")
            .execute(&self.pool)
            .await
            .expect("drop the cached scopes");
        self.warm().await;
    }

    /// Cached == uncached, hit for hit, in every scope, and the cached search
    /// really read the cache.
    async fn assert_parity(&self, after: &str) {
        for (label, filters) in self.scopes() {
            let (uncached, _) = self.hits(&filters, false).await;
            let (cached, used) = self.hits(&filters, true).await;
            assert_eq!(
                used,
                ScopeCacheUse::Hit,
                "{after}, {label}: not read from the cache"
            );
            assert_eq!(
                cached, uncached,
                "{after}, {label}: the cached scope differs"
            );
        }
    }

    async fn destroy(self) {
        self.database.destroy(self.pool).await;
    }
}

#[tokio::test]
async fn a_cached_scope_answers_like_a_collected_one_after_every_kind_of_growth() {
    let _serial = SERIAL.lock().await;
    let mut fixture = Fixture::new("scope-cache-property").await;
    let pool = fixture.pool.clone();
    let talk = conversation(
        &pool,
        "11111111-1111-4111-8111-111111111111",
        Some(&fixture.alpha),
    )
    .await;
    let first_turn = say(
        &pool,
        &talk,
        1,
        "an early turn in alpha's conversation",
        0.2,
    )
    .await;
    fixture.rebuild().await;
    fixture.assert_parity("first build").await;

    // A new file: a new path to a new blob.
    fixture.rebuild().await;
    let (fresh, _) = blob(&pool, "alpha_three", 0.05).await;
    fixture
        .alpha_files
        .push(("src/alpha_three.rs".to_string(), fresh));
    fixture.sync().await;
    fixture.assert_parity("a new file").await;

    // A branch switch onto content the index already holds: an existing path
    // re-pointed at a blob parsed (and stamped) long before the cache.
    let (elsewhere, _) = blob(&pool, "beta_only", 0.03).await;
    sync_worktree_files(
        &pool,
        beta_worktree(&pool).await,
        &[("src/beta_only.rs".to_string(), elsewhere.clone())],
    )
    .await
    .unwrap();
    fixture.rebuild().await;
    fixture.alpha_files[0].1 = elsewhere;
    fixture.sync().await;
    fixture
        .assert_parity("a branch switch onto an existing blob")
        .await;

    // A blob already on a path gains an element (a reparse): no path moved.
    fixture.rebuild().await;
    let reparsed = fixture.alpha_files[1].1.clone();
    let body_text = "fn reparsed() { found_through_its_new_element(); }";
    let child = Element::new(
        ElementKind::Function,
        "scope_cache_fixture",
        "reparsed",
        "alpha_two.rs::reparsed",
        Span::new(2, 2),
        body_text,
    );
    let root = Element::new(
        ElementKind::File,
        "fixture",
        "alpha_two.rs",
        "alpha_two.rs",
        Span::new(1, 2),
        body_text,
    )
    .with_children(vec![child.clone()]);
    let departed_before = departed(&pool).await;
    upsert_element_tree(&pool, &reparsed, REPARSED, &root, |_| false)
        .await
        .unwrap();
    // The blob's two older rows are superseded: four sources, counted.
    assert!(
        departed(&pool).await >= departed_before + 4,
        "a reparse counts the sources it supersedes"
    );
    embed(&pool, child.raw_hash(), SourceKind::Raw, 0.0).await;
    fixture.assert_parity("a blob that gained an element").await;

    // A deleted file: the cached scope is a superset; admission drops it.
    fixture.rebuild().await;
    fixture.alpha_files.pop();
    fixture.sync().await;
    fixture.assert_parity("a deleted file").await;

    // A conversation append.
    fixture.rebuild().await;
    say(
        &pool,
        &talk,
        2,
        "a later turn in alpha's conversation",
        0.04,
    )
    .await;
    fixture.assert_parity("a conversation append").await;

    // A deduped turn: alpha's new conversation says what beta's already did,
    // so its element (and vector) predate the cache.
    let beta_talk = conversation(
        &pool,
        "22222222-2222-4222-8222-222222222222",
        Some(&fixture.beta),
    )
    .await;
    let shared = "the very same words in two repositories";
    say(&pool, &beta_talk, 1, shared, 0.01).await;
    fixture.rebuild().await;
    let echo = conversation(
        &pool,
        "33333333-3333-4333-8333-333333333333",
        Some(&fixture.alpha),
    )
    .await;
    append_turns(&pool, &echo, &[turn(1, shared)], |_| false)
        .await
        .unwrap();
    fixture.assert_parity("a deduped turn").await;

    // Late attribution: a conversation stored with no repository, given one
    // after the cache was built.
    let stray = conversation(&pool, "44444444-4444-4444-8444-444444444444", None).await;
    say(
        &pool,
        &stray,
        1,
        "a turn whose conversation finds its repository later",
        0.02,
    )
    .await;
    fixture.rebuild().await;
    conversation(
        &pool,
        "44444444-4444-4444-8444-444444444444",
        Some(&fixture.alpha),
    )
    .await;
    fixture.assert_parity("late repository attribution").await;

    // Boot normalisation: a conversation filed under an old spelling of
    // alpha's identity is rewritten to the canonical one.
    let legacy = ConversationId::new("66666666-6666-4666-8666-666666666666").unwrap();
    upsert_conversation(
        &pool,
        &Conversation {
            guid: legacy.clone(),
            repo_identity: Some("legacy-alpha".to_string()),
            worktree: None,
            base_sha: None,
            title: None,
            started_at: "2026-10-15T00:00:00Z".to_string(),
            parent: None,
        },
    )
    .await
    .unwrap();
    say(
        &pool,
        &legacy,
        1,
        "a turn filed under an old spelling of alpha",
        0.015,
    )
    .await;
    fixture.rebuild().await;
    let canonical = fixture.alpha.key().to_string();
    normalise_conversation_identities(&pool, |spelling| {
        if spelling == "legacy-alpha" {
            canonical.clone()
        } else {
            spelling.to_string()
        }
    })
    .await
    .unwrap();
    fixture
        .assert_parity("boot normalisation of an identity")
        .await;

    // A late summary for a source the cache already holds.
    fixture.rebuild().await;
    let summary = Summary {
        text: "A summary that arrives after its turn was cached.".to_string(),
        tags: vec![
            "late".to_string(),
            "summary".to_string(),
            "cache".to_string(),
        ],
        ..Summary::default()
    };
    put_smart_content(&pool, &first_turn, SUMMARIZER, &summary)
        .await
        .unwrap();
    embed(
        &pool,
        &content_hash(summary.text.as_bytes()),
        SourceKind::Smart,
        0.0,
    )
    .await;
    fixture.assert_parity("a late summary").await;

    // A restated turn: its blob changes in place, to text only beta held
    // until now, so nothing else puts it in alpha's scope.
    say(&pool, &beta_talk, 2, "words a restatement gives alpha", 0.0).await;
    fixture.rebuild().await;
    sqlx::query(
        "UPDATE turns SET blob_sha = (SELECT blob_sha FROM turns
                                       WHERE conversation_id = $2::uuid AND turn_no = 2)
          WHERE conversation_id = $1::uuid AND turn_no = 2",
    )
    .bind(talk.as_str())
    .bind(beta_talk.as_str())
    .execute(&pool)
    .await
    .unwrap();
    fixture.assert_parity("a restated turn").await;

    fixture.destroy().await;
}

/// Sources counted as departed from every repository's scopes.
async fn departed(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(sum(removed), 0)::bigint FROM scope_shrinks")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn beta_worktree(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT id FROM worktrees WHERE root_path = '/fixtures/beta'")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The standing rule: a search never hides the caller's own session. A turn
/// written a moment ago is found by the next search through a cached scope.
#[tokio::test]
async fn the_callers_fresh_turn_is_found_on_the_next_search() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new("scope-cache-fresh-turn").await;
    let pool = fixture.pool.clone();
    let session = conversation(
        &pool,
        "55555555-5555-4555-8555-555555555555",
        Some(&fixture.alpha),
    )
    .await;
    say(&pool, &session, 1, "what the caller said before", 0.3).await;
    fixture.warm().await;

    say(&pool, &session, 2, "what the caller said a moment ago", 0.0).await;
    let (filters_label, filters) = fixture.scopes().remove(0);
    let (hits, used) = fixture.hits(&filters, true).await;
    assert_eq!(used, ScopeCacheUse::Hit, "{filters_label}");
    assert!(
        hits.first().is_some_and(|hit| hit.contains("#t2")),
        "the fresh turn is the nearest hit: {hits:#?}"
    );

    fixture.destroy().await;
}

/// A write whose transaction began before the cache was built and committed
/// after it is invisible to the build, and still found: the cutoff is no
/// later than the start of any write transaction in flight.
#[tokio::test]
async fn a_write_in_flight_while_the_cache_builds_is_still_found() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new("scope-cache-in-flight").await;
    let pool = fixture.pool.clone();
    let (late, _) = blob(&pool, "in_flight", 0.0).await;

    let mut writer = pool.begin().await.unwrap();
    sqlx::query("UPDATE worktree_files SET blob_sha = $1 WHERE worktree_id = $2 AND path = $3")
        .bind(late.as_str())
        .bind(fixture.alpha_worktree)
        .bind(&fixture.alpha_files[0].0)
        .execute(&mut *writer)
        .await
        .unwrap();
    // Stamped, but not committed: the build cannot see it.
    fixture.warm().await;
    writer.commit().await.unwrap();

    fixture
        .assert_parity("a write committed after the build")
        .await;
    let (hits, _) = fixture.hits(&fixture.scopes().remove(0).1, true).await;
    assert!(
        hits.first().is_some_and(|hit| hit.contains("in_flight")),
        "the in-flight write is the nearest hit: {hits:#?}"
    );

    fixture.destroy().await;
}

/// `search.scope_cache = false` collects every scope in full and writes no
/// cache.
#[tokio::test]
async fn the_kill_switch_collects_in_full_and_caches_nothing() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new("scope-cache-off").await;
    let (_, filters) = fixture.scopes().remove(0);
    let (_, used) = fixture.hits(&filters, false).await;
    assert_eq!(used, ScopeCacheUse::Bypassed);
    let cached: i64 = sqlx::query_scalar("SELECT count(*) FROM scope_cache")
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(cached, 0);
    fixture.destroy().await;
}

/// A transaction that began before the build but wrote nothing until after
/// it took its snapshot: its first write's xid is stamped as it is written,
/// and is past the build's cutoff.
#[tokio::test]
async fn a_first_write_after_the_build_began_is_still_found() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new("scope-cache-first-write").await;
    let pool = fixture.pool.clone();
    let (late, _) = blob(&pool, "first_write", 0.0).await;

    let mut writer = pool.begin().await.unwrap();
    sqlx::query("SELECT 1").execute(&mut *writer).await.unwrap();
    fixture.warm().await;
    sqlx::query("UPDATE worktree_files SET blob_sha = $1 WHERE worktree_id = $2 AND path = $3")
        .bind(late.as_str())
        .bind(fixture.alpha_worktree)
        .bind(&fixture.alpha_files[0].0)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.commit().await.unwrap();

    fixture
        .assert_parity("a first write after the build began")
        .await;
    let (hits, _) = fixture.hits(&fixture.scopes().remove(0).1, true).await;
    assert!(
        hits.first().is_some_and(|hit| hit.contains("first_write")),
        "the late write is the nearest hit: {hits:#?}"
    );

    fixture.destroy().await;
}

/// A cached scope can still carry sources that have left it, so its count can
/// sit over an exact-ranking limit the live scope is under. It must rank the
/// way the live scope would: exactly.
#[tokio::test]
async fn a_cached_scope_over_the_limit_only_by_departed_sources_still_ranks_exactly() {
    let _serial = SERIAL.lock().await;
    // alpha's code scope holds two sources (one per file): the limit is two.
    let mut fixture =
        Fixture::with_settings("scope-cache-near-limit", &["fs3.exact_scope_limit = 2"]).await;
    let pool = fixture.pool.clone();
    let (label, code) = fixture.scopes().remove(1);
    assert_eq!(label, "repo, code");
    fixture.warm().await;

    // One file leaves (still cached, admission drops it) and another arrives:
    // the live scope holds two, the cached one three.
    fixture.alpha_files.pop();
    let (fresh, _) = blob(&pool, "alpha_four", 0.0).await;
    fixture
        .alpha_files
        .push(("src/alpha_four.rs".to_string(), fresh));
    fixture.sync().await;

    let uncached = fixture.page(&code, false).await;
    let cached = fixture.page(&code, true).await;
    assert_eq!(cached.scope_cache, ScopeCacheUse::Hit);
    assert!(uncached.exact_scan, "the live scope is within the limit");
    assert!(
        cached.exact_scan,
        "a count over the limit only by departed sources ranks exactly"
    );
    assert_eq!(
        cached
            .hits
            .iter()
            .map(|hit| hit.similar.element.address.clone())
            .collect::<Vec<_>>(),
        uncached
            .hits
            .iter()
            .map(|hit| hit.similar.element.address.clone())
            .collect::<Vec<_>>()
    );

    fixture.destroy().await;
}

/// Departures are counted as they happen; once a cached scope could carry
/// SCOPE_SHRINK_REBUILD departed sources it is rebuilt, before it serves
/// another search.
#[tokio::test]
async fn a_scope_that_lost_more_than_the_bound_is_rebuilt_first() {
    let _serial = SERIAL.lock().await;
    let mut fixture = Fixture::new("scope-cache-shrink").await;
    let pool = fixture.pool.clone();

    // One file of a thousand declarations: removing it departs over 2,000
    // sources (two per element).
    let big = unique_blob();
    let children = (0..1_000)
        .map(|index| {
            Element::new(
                ElementKind::Function,
                "scope_cache_fixture",
                format!("big_{index}"),
                format!("big.rs::big_{index}"),
                Span::new(index + 1, index + 1),
                format!("fn big_{index}() {{}}"),
            )
        })
        .collect::<Vec<_>>();
    let root = Element::new(
        ElementKind::File,
        "fixture",
        "big.rs",
        "big.rs",
        Span::new(1, 1_000),
        "the big file",
    )
    .with_children(children);
    upsert_element_tree(&pool, &big, PARSER_VERSION, &root, |_| false)
        .await
        .unwrap();
    fixture.alpha_files.push(("src/big.rs".to_string(), big));
    fixture.sync().await;
    fixture.rebuild().await;

    // A small departure is carried.
    let small = fixture.alpha_files.remove(0);
    fixture.sync().await;
    let (_, used) = fixture.hits(&fixture.scopes().remove(0).1, true).await;
    assert_eq!(
        used,
        ScopeCacheUse::Hit,
        "a few departed sources are carried"
    );

    // The big one is not. Built under a minute ago, the scope is collected
    // in full rather than rebuilt (no rebuild storm under churn) ...
    fixture.alpha_files.pop();
    fixture.sync().await;
    let (filters_label, every_kind) = fixture.scopes().remove(0);
    let (cooling, used) = fixture.hits(&every_kind, true).await;
    assert_eq!(used, ScopeCacheUse::Cooling, "{filters_label}");
    let (collected, _) = fixture.hits(&every_kind, false).await;
    assert_eq!(
        cooling, collected,
        "a cooling scope answers like a collected one"
    );

    // ... and once the cooldown has passed, it is rebuilt.
    sqlx::query("UPDATE scope_cache SET built_at = built_at - make_interval(secs => $1)")
        .bind(fs3_store::SCOPE_REBUILD_COOLDOWN.as_secs_f64() + 1.0)
        .execute(&pool)
        .await
        .unwrap();
    let (_, used) = fixture.hits(&every_kind, true).await;
    assert_eq!(
        used,
        ScopeCacheUse::Built,
        "over the bound and past the cooldown, the scope is rebuilt"
    );
    // Every key watching alpha saw the same departures: each is rebuilt on
    // its next search, then reads like a collected scope.
    fixture.alpha_files.insert(0, small);
    fixture.sync().await;
    sqlx::query("UPDATE scope_cache SET built_at = built_at - make_interval(secs => $1)")
        .bind(fs3_store::SCOPE_REBUILD_COOLDOWN.as_secs_f64() + 1.0)
        .execute(&pool)
        .await
        .unwrap();
    fixture.warm().await;
    fixture.assert_parity("after the rebuild").await;

    fixture.destroy().await;
}
