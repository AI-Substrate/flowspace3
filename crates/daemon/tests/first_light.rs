//! First light, proven end to end: add → scan → enrich → search.
//!
//! One test binary, one throwaway database per test, the fake provider
//! throughout. What it proves is the plan's whole claim — that the landed parts
//! compose into a working system — and it proves it against the REAL daemon
//! router and the REAL job runner, not a rehearsal of them.
//!
//! # Why a fixture repository and not this one
//!
//! A fixture tree is small enough to assert exactly: every element it produces
//! is named in an expectation, so a regression shows up as a wrong count rather
//! than a plausible one. The live run against a real repository is a separate
//! proof with a different purpose (`assets/first-light-run.md`).
//!
//! # Why real git
//!
//! `fs3_git::blob_id` is git's own hash, and the whole incremental story rests
//! on it. A fixture that faked blob ids would prove the plumbing and skip the
//! claim.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use fs3_core::envelope::Envelope;
use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::wiring::AppState;
use fs3_daemon::{roots, runner};
use serde_json::Value;
use sqlx::Row;

/// A daemon URL nothing is listening on, for tests that exercise doctor's store
/// steps rather than its daemon row. Port 1 is privileged and never serves.
const NO_DAEMON: &str = "http://127.0.0.1:1";

/// A doctor config pointing at `database_url`, with no daemon listening.
fn doctor_config(database_url: &str) -> Config {
    Config {
        database: DatabaseConfig {
            url: database_url.to_string(),
        },
        daemon: fs3_core::DaemonConfig {
            url: NO_DAEMON.to_string(),
            ..fs3_core::DaemonConfig::default()
        },
        ..Config::default()
    }
}

/// A git repository with fs3-shaped content in it.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Build a repository with three files worth indexing, one that discovery
    /// must refuse, and a `.gitignore` that hides a fourth.
    fn create(label: &str) -> Self {
        let root = support::temp_dir(label);

        Self::write(
            &root,
            "src/auth.rs",
            r#"
/// Validate a session token against the store.
///
/// Returns false for an expired token, which is why the caller must not
/// distinguish "expired" from "never existed" in what it tells the user.
pub fn validate_session_token(token: &str, now: u64) -> bool {
    let Some(session) = lookup_session(token) else {
        return false;
    };
    if session.expires_at <= now {
        return false;
    }
    session.revoked_at.is_none()
}

/// Find a session by its opaque token.
fn lookup_session(token: &str) -> Option<Session> {
    SESSIONS.lock().unwrap().get(token).cloned()
}
"#,
        );

        Self::write(
            &root,
            "src/geometry.rs",
            r#"
/// A rectangle in screen space.
pub struct Rect {
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// The area of the rectangle, in square pixels.
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Whether the rectangle covers no pixels at all.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}
"#,
        );

        Self::write(
            &root,
            "README.md",
            "# The fixture\n\n## Sessions\n\nSessions expire and are then invalid.\n\n## Shapes\n\nRectangles have an area.\n",
        );

        // Refused by discovery: a config format (PRD req 43).
        Self::write(&root, "settings.toml", "[a]\nb = 1\n");
        // Hidden from discovery by git.
        Self::write(&root, ".gitignore", "ignored/\n");
        Self::write(&root, "ignored/secret.rs", "fn never_indexed() {}\n");
        // Refused by the standard deny list, and NOT by git: nothing in
        // `.gitignore` mentions it, so its absence can only be the deny list.
        // Real JavaScript, in the directory a fresh clone is fullest of.
        Self::write(
            &root,
            "node_modules/pkg/index.js",
            "module.exports = () => 'somebody else\\'s dependency';\n",
        );

        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "fixture@fs3.test"]);
        git(&root, &["config", "user.name", "fs3 fixture"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "--quiet", "-m", "fixture"]);

        Fixture { root }
    }

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("creating fixture directories");
        std::fs::write(path, contents.trim_start()).expect("writing a fixture file");
    }

    fn path(&self) -> &Path {
        &self.root
    }

    /// Edit one file, changing exactly one function's body.
    fn touch_auth(&self) {
        let path = self.root.join("src/auth.rs");
        let text = std::fs::read_to_string(&path).expect("the fixture file exists");
        std::fs::write(
            &path,
            text.replace(
                "session.revoked_at.is_none()",
                "session.revoked_at.is_none() && true",
            ),
        )
        .expect("rewriting the fixture file");
    }
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .unwrap_or_else(|error| panic!("running git {args:?}: {error}"));
    assert!(status.success(), "git {args:?} failed");
}

/// A throwaway database, and the config that points at it.
struct Stack {
    database: support::FreshDatabase,
    state: AppState,
}

impl Stack {
    async fn create(label: &str) -> Self {
        let database = support::FreshDatabase::create(label).await;
        let state = Self::wire(&database.url(), true).await;
        Self { database, state }
    }

    /// A stack whose database exists but has NOT been migrated — the behind-db
    /// case tk-0108's guard exists for.
    async fn create_unmigrated(label: &str) -> Self {
        let database = support::FreshDatabase::create(label).await;
        let state = Self::wire(&database.url(), false).await;
        Self { database, state }
    }

    async fn wire(url: &str, migrate: bool) -> AppState {
        // `fake` is a legal production value (workshop 001 rule 5), so the whole
        // stack runs offline with no keys — including the vector width, which
        // the composition root pins to the store's. Everything else is the
        // shipped default, so the test exercises the configuration a fresh
        // machine has.
        let config = Config {
            database: DatabaseConfig {
                url: url.to_string(),
            },
            ..Config::default()
        };

        let state = AppState::from_config(config).expect("the fake stack wires");
        if migrate {
            fs3_store::migrate(&state.db)
                .await
                .expect("a fresh database migrates");
        }
        state
    }

    /// Drain the queue until nothing is ready, running the REAL runner.
    async fn drain(&self) -> runner::Drained {
        let mut total = runner::Drained::default();
        // Several passes: a scan enqueues enrichment, and enrichment enqueues
        // the smart vectors a summary earns, so one pass cannot see work that
        // does not exist yet.
        for _ in 0..8 {
            let pass = runner::drain(&self.state, 4).await;
            if pass.total() == 0 {
                break;
            }
            total.completed += pass.completed;
            total.retried += pass.retried;
            total.failed += pass.failed;
        }
        total
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query(sql)
            .fetch_one(&self.state.db)
            .await
            .expect("a count query")
            .try_get::<i64, _>(0)
            .expect("counts are bigints")
    }

    async fn destroy(self) {
        let pool = self.state.db.clone();
        self.database.destroy(pool).await;
    }
}

/// Call the router the daemon actually serves, so the test cannot pass against
/// a shape the binary does not have.
async fn call(state: &AppState, method: &str, path: &str, body: Option<Value>) -> Envelope {
    let auth = support::auth("first-light-call");
    let base = support::spawn(fs3_daemon::router(state.clone(), auth.auth)).await;
    let client = reqwest::Client::new();
    let url = format!("{base}{path}");
    let request = match method {
        "POST" => client.post(&url).json(&body.unwrap_or(Value::Null)),
        _ => client.get(&url),
    }
    .bearer_auth(&auth.key);
    let response = request.send().await.expect("the daemon answers");
    let status = response.status();
    let envelope: Envelope = response
        .json()
        .await
        .expect("every route answers an envelope");
    assert_eq!(
        status.as_u16(),
        envelope.http_status(),
        "the HTTP status must be the one the envelope's code implies (workshop 004 D4)"
    );
    envelope
}

// ---------------------------------------------------------------------------
// ac-0001 + ac-0002 + ac-0003: the whole path

/// The plan's headline claim, in one test: add a real repository through the
/// daemon, let the real runner drain the queue, and ask a real question.
#[tokio::test]
async fn add_scan_enrich_and_search_answer_end_to_end() {
    let fixture = Fixture::create("e2e");
    let stack = Stack::create("e2e").await;

    // --- add ---------------------------------------------------------------
    let added = call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": fixture.path().to_string_lossy() })),
    )
    .await;
    assert!(added.ok, "add failed: {:?}", added.error);
    let data = added.data.expect("a successful add carries data");

    assert_eq!(
        data["files"], 3,
        "discovery accepts the two .rs files and the .md, and nothing else"
    );
    assert_eq!(
        data["enqueued"], 3,
        "a first add is three files nobody has scanned"
    );
    assert_eq!(data["unchanged"], 0);
    assert!(
        data["identity"].as_str().unwrap().starts_with("git:")
            || data["identity"].as_str().unwrap().starts_with("path:"),
        "the identity is prefixed by its source so the two key spaces cannot collide"
    );

    // PRD req 43: a refused file is reported, never a silent gap.
    let skipped = data["skipped"].as_array().expect("a skip ledger");
    assert!(
        skipped
            .iter()
            .any(|row| row["reason"] == "config-format" && row["count"] == 1),
        "settings.toml must be REFUSED and said so, got {skipped:?}"
    );
    assert!(
        !skipped.iter().any(|row| row["reason"] == "gitignored"),
        "a git-ignored FILE is out of scope, not refused: it is in neither file \
         list, and git already answers why (`git check-ignore -v`). This is not \
         the same claim as `nothing unindexed is ever reported` — see the prune \
         ledger below, which names DIRECTORIES fs3 itself refused to walk"
    );

    // The other half of that doctrine, and the reason it had to be split: a
    // denied directory puts nothing in EITHER file list, so without this the
    // only symptom of node_modules/ not being indexed is code missing from a
    // search months later. Named, not counted — eleven directories are the
    // answer, where thousands of files would only be a summary of it.
    let pruned = data["pruned"].as_array().expect("a prune ledger");
    let node_modules = pruned
        .iter()
        .find(|row| row["path"] == "node_modules")
        .unwrap_or_else(|| panic!("node_modules must be NAMED as pruned, got {pruned:?}"));
    assert_eq!(node_modules["reason"], "standard-ignore");
    assert!(
        node_modules["fix"]
            .as_str()
            .expect("a fix is text")
            .contains("standard_ignores"),
        "the fix must name a line that can actually be typed into config",
    );
    assert!(
        !pruned
            .iter()
            .any(|row| row["path"].as_str().is_some_and(|p| p.contains('/'))),
        "the DIRECTORY is named, never its contents — that is what keeps this \
         ledger eleven rows instead of a hundred thousand, got {pruned:?}"
    );

    // --- drain -------------------------------------------------------------
    let drained = stack.drain().await;
    assert_eq!(drained.failed, 0, "no job should fail against the fake");
    assert!(drained.completed >= 3, "at least the three scans ran");

    // --- what landed -------------------------------------------------------
    let elements = stack.count("SELECT count(*) FROM elements").await;
    assert!(
        elements >= 8,
        "three files of real declarations produce a tree, got {elements}"
    );
    let file_roots = stack
        .count("SELECT count(*) FROM elements WHERE kind = 'file' AND parent_id IS NULL")
        .await;
    let summaries = stack.count("SELECT count(*) FROM smart_content").await;
    assert!(summaries > 0, "enrich-marked elements earned summaries");
    let raw_vectors = stack
        .count("SELECT count(*) FROM embeddings_1024 WHERE source_kind = 'raw'")
        .await;
    let smart_vectors = stack
        .count("SELECT count(*) FROM embeddings_1024 WHERE source_kind = 'smart'")
        .await;
    assert_eq!(
        raw_vectors,
        elements - file_roots,
        "every element earns a raw vector EXCEPT a file root its children already \
         cover — that text is the concatenation of texts already indexed one by one, so its \
         vector would compete with every one of its own parts on every query about that file"
    );
    assert_eq!(
        smart_vectors, summaries,
        "every summary earns exactly one smart vector"
    );

    // The store's width is 1024 and the fake's own default is 32; the
    // composition root is what makes them agree, and these rows are the proof.
    assert!(
        raw_vectors > 0,
        "an offline stack must produce real vectors"
    );

    // --- search ------------------------------------------------------------
    //
    // Two questions about two different subjects, each asserted to reach the
    // right FILE. That is the claim worth pinning: the index discriminates
    // between unrelated content. Which element inside the right file ranks
    // first is decided by the embedder's own geometry — with the deterministic
    // fake that is feature hashing, where a short body of matching tokens beats
    // a longer one — so asserting an exact within-file winner would be pinning
    // the fake's implementation rather than fs3's behaviour, and it would have
    // to be re-tuned every time the fake changed. The live Azure run is where
    // within-file ranking quality is judged.
    let found = call(
        &stack.state,
        "GET",
        "/search?q=validate%20an%20expired%20session%20token&limit=5",
        None,
    )
    .await;
    assert!(found.ok, "search failed: {:?}", found.error);
    let results = found.data.expect("search carries data")["results"]
        .as_array()
        .expect("results is a list")
        .clone();
    assert!(!results.is_empty(), "the question must find something");

    let best = &results[0];
    assert_eq!(
        best["path"], "src/auth.rs",
        "a session question must reach the session file, and the hit must resolve through the \
         ref layer to a live path"
    );
    assert!(
        results.iter().any(|hit| hit["address"]
            .as_str()
            .unwrap_or_default()
            .contains("validate_session_token")),
        "the known-relevant element must be among the hits, got {results:?}"
    );
    assert!(
        best["span"][0].as_u64().unwrap() >= 1,
        "spans are 1-based and real"
    );
    assert!(
        best["score"].as_f64().unwrap() > 0.0,
        "score is 1 - distance, so a real hit is positive"
    );
    assert!(
        matches!(best["match_field"].as_str(), Some("raw") | Some("smart")),
        "match_field reports which space won"
    );
    assert!(
        best["kind"].as_str().is_some_and(|kind| kind != "file"),
        "a file root its children cover earns no vector, so it can never be the answer"
    );

    // The discrimination half: an unrelated question must NOT land in auth.rs.
    let geometry = call(
        &stack.state,
        "GET",
        "/search?q=area%20of%20a%20rectangle%20width%20times%20height&limit=3",
        None,
    )
    .await;
    let geometry_results = geometry.data.expect("search carries data")["results"]
        .as_array()
        .expect("results is a list")
        .clone();
    assert_eq!(
        geometry_results[0]["path"], "src/geometry.rs",
        "a geometry question must reach the geometry file — an index that answers every \
         question with the same file is not an index, got {geometry_results:?}"
    );

    stack.destroy().await;
}

/// ac-0003's second half, and the acceptance criterion the whole content-
/// addressed design exists for: re-scanning an unchanged tree costs NOTHING.
#[tokio::test]
async fn a_rescan_of_an_unchanged_tree_enqueues_no_work_at_all() {
    let fixture = Fixture::create("idempotent");
    let stack = Stack::create("idempotent").await;

    let path = fixture.path().to_string_lossy().to_string();
    call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    stack.drain().await;

    let summaries_before = stack.count("SELECT count(*) FROM smart_content").await;
    let vectors_before = stack.count("SELECT count(*) FROM embeddings_1024").await;
    assert!(
        summaries_before > 0 && vectors_before > 0,
        "the first pass paid"
    );

    // --- the same tree, again ----------------------------------------------
    let again = call(
        &stack.state,
        "POST",
        "/scan",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    assert!(again.ok, "rescan failed: {:?}", again.error);
    let data = again.data.expect("a successful scan carries data");

    assert_eq!(
        data["enqueued"], 0,
        "nothing changed, so nothing is queued — this is the acceptance criterion"
    );
    assert_eq!(
        data["unchanged"], 3,
        "every file is recognised as unchanged"
    );
    assert_eq!(data["removed"], 0);

    let drained = stack.drain().await;
    assert_eq!(drained.total(), 0, "there was nothing to drain");
    assert_eq!(
        stack.count("SELECT count(*) FROM smart_content").await,
        summaries_before,
        "no LLM call was paid for a second time"
    );
    assert_eq!(
        stack.count("SELECT count(*) FROM embeddings_1024").await,
        vectors_before,
        "no embedding was paid for a second time"
    );

    stack.destroy().await;
}

/// The other half of idempotence: a real edit MUST cost something, and only for
/// what changed. A test that only proves "nothing re-runs" would pass on a
/// pipeline that had stopped working entirely.
#[tokio::test]
async fn editing_one_file_re_indexes_that_file_and_only_that_file() {
    let fixture = Fixture::create("incremental");
    let stack = Stack::create("incremental").await;

    let path = fixture.path().to_string_lossy().to_string();
    call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    stack.drain().await;
    let elements_before = stack.count("SELECT count(*) FROM elements").await;

    fixture.touch_auth();

    let again = call(
        &stack.state,
        "POST",
        "/scan",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    let data = again.data.expect("a successful scan carries data");
    assert_eq!(data["enqueued"], 1, "exactly the edited file is re-queued");
    assert_eq!(data["unchanged"], 2, "the other two are untouched");

    stack.drain().await;

    // A new blob means new element rows for that file; the untouched files'
    // rows are keyed by blobs that did not change, so they are not rewritten.
    assert!(
        stack.count("SELECT count(*) FROM elements").await > elements_before,
        "the edited file's new bytes are a new blob, and a new tree"
    );

    stack.destroy().await;
}

// ---------------------------------------------------------------------------
// tk-0102: errors are envelopes with codes and fixes

/// A bad path is a 404 with a code and a fix, not a stack trace — and the fix
/// names the trap it is about to fall into.
#[tokio::test]
async fn adding_a_path_that_does_not_exist_answers_a_coded_envelope() {
    let stack = Stack::create("badpath").await;

    let refused = call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": "/definitely/not/a/repository" })),
    )
    .await;

    assert!(!refused.ok);
    let error = refused.error.expect("a failure carries an error");
    assert_eq!(error.code, "FS3-E-SCAN-ROOT-NOT-FOUND");
    assert!(!error.fix.is_empty(), "workshop 004 D3: fix is mandatory");
    assert!(
        error.fix.contains("ABSOLUTE"),
        "the daemon-cwd trap must be named: {}",
        error.fix
    );

    stack.destroy().await;
}

/// `scan` on a path nobody added must not silently register it — that would
/// make a typo look like success.
#[tokio::test]
async fn scanning_an_unregistered_root_refuses_and_points_at_add() {
    let fixture = Fixture::create("unregistered");
    let stack = Stack::create("unregistered").await;

    let refused = call(
        &stack.state,
        "POST",
        "/scan",
        Some(serde_json::json!({ "path": fixture.path().to_string_lossy() })),
    )
    .await;

    assert!(!refused.ok);
    let error = refused.error.expect("a failure carries an error");
    assert_eq!(error.code, "FS3-E-SCAN-ROOT-NOT-REGISTERED");
    assert!(error.fix.contains("flowspace3 add"));
    assert_eq!(
        stack.count("SELECT count(*) FROM worktrees").await,
        0,
        "a refused scan must not have registered anything"
    );

    stack.destroy().await;
}

/// An empty query cannot be ranked against anything, and saying so beats
/// returning the whole index.
#[tokio::test]
async fn an_empty_query_is_refused_with_a_usable_fix() {
    let stack = Stack::create("emptyquery").await;

    let refused = call(&stack.state, "GET", "/search?q=%20", None).await;

    assert!(!refused.ok);
    assert_eq!(refused.http_status(), 400, "the caller's mistake is a 400");
    let error = refused.error.expect("a failure carries an error");
    assert_eq!(error.code, "FS3-E-QUERY-INVALID");
    assert!(error.fix.contains("flowspace3 search"));

    stack.destroy().await;
}

// ---------------------------------------------------------------------------
// tk-0108 / dw-0109: behind db → rejection → doctor → success

/// The full loop the schema discipline exists for: a database that is behind
/// rejects every db-touching command with a code naming doctor; doctor applies
/// what is missing; the same command then succeeds.
#[tokio::test]
async fn a_behind_database_is_rejected_then_repaired_by_doctor_then_works() {
    let fixture = Fixture::create("schema");
    let stack = Stack::create_unmigrated("schema").await;
    let path = fixture.path().to_string_lossy().to_string();

    // --- behind: every db-touching route refuses, by code ------------------
    for (method, route, body) in [
        (
            "POST",
            "/roots",
            Some(serde_json::json!({ "path": path.clone() })),
        ),
        ("GET", "/status", None),
        ("GET", "/search?q=anything", None),
    ] {
        let refused = call(&stack.state, method, route, body).await;
        assert!(!refused.ok, "{route} must refuse against a behind database");
        let error = refused.error.expect("a failure carries an error");
        assert_eq!(
            error.code, "FS3-E-STORE-SCHEMA-STALE",
            "{route} must name the SCHEMA, not a missing column"
        );
        assert!(
            error.fix.contains("flowspace3 doctor"),
            "the fix must name the one command that repairs it: {}",
            error.fix
        );
    }

    // `/health` is independent of schema state, but not of daemon auth.
    let auth = support::auth("behind-schema-health");
    let base = support::spawn(fs3_daemon::router(stack.state.clone(), auth.auth)).await;
    let health = reqwest::Client::new()
        .get(format!("{base}/health"))
        .bearer_auth(&auth.key)
        .send()
        .await
        .expect("health answers")
        .status();
    assert!(health.is_success(), "health must not depend on the schema");

    // --- doctor repairs it -------------------------------------------------
    // No daemon is listening in this test, so doctor reports the stack as
    // degraded — correctly. What is under test is the SCHEMA row.
    let doctor_auth = support::auth("doctor-behind-schema");
    let report = fs3_cli::doctor::run(
        &doctor_config(&stack.database.url()),
        &doctor_auth.config_dir,
    )
    .await;
    assert!(report.ok, "doctor failed: {:?}", report.error);
    let data = report.data.expect("doctor reports its steps");
    assert!(data.healthy);

    let schema_step = data
        .steps
        .iter()
        .find(|step| step.check == "schema")
        .expect("doctor walks the schema step");
    assert_eq!(
        schema_step.outcome, "repaired",
        "doctor must APPLY the migrations, not merely report them missing"
    );
    assert!(
        schema_step
            .action
            .as_deref()
            .unwrap_or_default()
            .contains("applied"),
        "the row must say what it DID: {schema_step:?}"
    );

    // Every earlier step passed, and said so.
    for check in ["engine", "stack", "database"] {
        assert!(
            data.steps.iter().any(|step| step.check == check),
            "doctor must walk {check} before the schema"
        );
    }

    // --- the same command now succeeds -------------------------------------
    let added = call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    assert!(
        added.ok,
        "after doctor, the rejected command must work: {:?}",
        added.error
    );

    stack.destroy().await;
}

/// dw-0109's other half: a database that does not exist at all is created,
/// not merely reported. Doctor is repair-as-it-goes, so the step BEFORE the
/// schema has to fix itself too.
#[tokio::test]
async fn doctor_creates_a_database_that_is_not_there() {
    let name = format!("fs3_doctor_e2e_{:032x}", support::unique_seed());
    let url = support::database_url_named(&name);

    // Nothing exists yet: the guard's own code says so rather than reporting a
    // generic outage.
    let probe = fs3_store::connect_lazy(&url).expect("a lazy pool builds");
    let before = fs3_store::schema_current(&probe).await;
    assert!(
        before.is_err(),
        "a database that does not exist cannot report a schema"
    );
    assert!(
        fs3_store::is_missing_database(&before.unwrap_err()),
        "and the reason must be legible as 'missing database', not 'server down'"
    );
    probe.close().await;

    let doctor_auth = support::auth("doctor-create-database");
    let report = fs3_cli::doctor::run(&doctor_config(&url), &doctor_auth.config_dir).await;
    assert!(report.ok, "doctor failed: {:?}", report.error);
    let data = report.data.expect("doctor reports its steps");

    let database_step = data
        .steps
        .iter()
        .find(|step| step.check == "database")
        .expect("doctor walks the database step");
    assert_eq!(database_step.outcome, "repaired");
    assert!(
        database_step
            .action
            .as_deref()
            .unwrap_or_default()
            .contains("created")
    );

    // And it is genuinely usable afterwards.
    let pool = fs3_store::connect(&url)
        .await
        .expect("the new database answers");
    assert!(
        fs3_store::schema_current(&pool)
            .await
            .expect("the schema reads")
            .is_current(),
        "doctor migrates what it creates — one command, not two"
    );
    pool.close().await;

    support::drop_database(&name).await;
}

// ---------------------------------------------------------------------------
// tk-0103: the retry policy

/// The policy the runner owns, proven through the real queue: a failure the
/// catalog marks NON-retryable is terminal at once, and the row keeps the code
/// so `status` can explain itself.
///
/// A malformed payload is the right specimen: it is the failure most likely to
/// come from a version skew rather than a bug, and no amount of waiting will
/// make it parse. It also has to be a kind the runner CLAIMS — `claim_job`
/// filters by kind, so a job of an unknown kind is never picked up at all and
/// proves nothing about the retry policy.
#[tokio::test]
async fn a_terminal_failure_is_not_retried_and_records_why() {
    let stack = Stack::create("retry").await;

    fs3_store::enqueue_job(
        &stack.state.db,
        roots::SCAN_FILE,
        "scan:broken",
        &serde_json::json!({ "not": "a scan job" }),
        std::time::Duration::ZERO,
    )
    .await
    .expect("enqueue");

    let drained = runner::drain(&stack.state, 1).await;
    assert_eq!(
        drained.failed, 1,
        "a payload that cannot parse fails at once"
    );
    assert_eq!(
        drained.retried, 0,
        "waiting cannot make a malformed payload parse, so retrying is spend with no upside"
    );

    let last = fs3_store::last_failure(&stack.state.db)
        .await
        .expect("the failure is recorded")
        .expect("there is one");
    assert_eq!(last.0, "scan:broken");
    assert!(
        last.1.contains("FS3-E-QUEUE-JOB-FAILED"),
        "the row keeps the CODE, so status can explain itself: {}",
        last.1
    );

    stack.destroy().await;
}

/// A malformed payload is the other terminal case, and the one most likely to
/// be produced by a version skew rather than by a bug.
#[tokio::test]
async fn a_scan_job_for_a_vanished_worktree_completes_rather_than_failing() {
    let stack = Stack::create("vanished").await;

    let job = roots::ScanFileJob {
        worktree_id: 4242,
        identity: "path:/gone".to_string(),
        path: "src/lib.rs".to_string(),
        blob: "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".to_string(),
    };
    fs3_store::enqueue_job(
        &stack.state.db,
        roots::SCAN_FILE,
        &job.dedupe_key(),
        &serde_json::to_value(&job).unwrap(),
        std::time::Duration::ZERO,
    )
    .await
    .expect("enqueue");

    let drained = runner::drain(&stack.state, 1).await;
    assert_eq!(
        drained.completed, 1,
        "a root removed while its job waited is not an error — there is simply nothing to do"
    );
    assert_eq!(drained.failed, 0);

    stack.destroy().await;
}

// ---------------------------------------------------------------------------
// tk-0104: per-repo provider resolution

/// The registry claim, and the regression that matters: a repo naming a
/// different instance must resolve to a DIFFERENT instance.
///
/// The previous version of this test only proved the fallback — an unknown repo
/// gets the default — which a broken `embedder_for` returning the default for
/// EVERYONE would also have passed. Divergence is the property; the fallback is
/// the easy half.
///
/// Identity is asserted by `Arc::ptr_eq` rather than by comparing keys, because
/// two `fake` instances legitimately produce the SAME key (`fake@1024` — the
/// key names the vector space, and both fakes share it). Pointer identity is
/// what "a different instance answered" actually means, and it is what a repo
/// override has to change for a real provider to be reached.
#[tokio::test]
async fn a_repo_override_resolves_to_a_different_instance_than_the_default() {
    let database = support::FreshDatabase::create("perrepo").await;

    const OVERRIDDEN: &str = "git:github.com/AI-Substrate/other";
    let mut config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    // A second instance, and one repo that names it.
    config
        .providers
        .insert("second".to_string(), fs3_core::ProviderInstance::Fake);
    config.repos.insert(
        OVERRIDDEN.to_string(),
        fs3_core::RepoSelection {
            embedder: Some("second".to_string()),
            summarizer: Some("second".to_string()),
            ..Default::default()
        },
    );

    let state = AppState::from_config(config).expect("the two-instance stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");

    let default_embedder = state.embedder_for("");
    let overridden_embedder = state.embedder_for(OVERRIDDEN);
    assert!(
        !std::sync::Arc::ptr_eq(default_embedder, overridden_embedder),
        "the overriding repo must reach a DIFFERENT instance, not the default"
    );
    assert!(
        !std::sync::Arc::ptr_eq(state.summarizer_for(""), state.summarizer_for(OVERRIDDEN)),
        "both ports honour the override independently"
    );

    // The fallback half, still worth pinning: a repo nobody configured gets the
    // active selection rather than failing.
    assert!(
        std::sync::Arc::ptr_eq(
            state.embedder_for(""),
            state.embedder_for("git:example.com/never-configured")
        ),
        "an unknown repo falls back to the active selection"
    );

    // And the key that enrichment rows are written under comes from whatever
    // answered, carrying the vector WIDTH so two spaces can never be compared.
    let key = state.embedder_key("");
    assert!(!key.is_empty(), "the key comes from provider.key()");
    assert!(
        key.contains(&fs3_store::EMBEDDING_DIMENSIONS.to_string()),
        "the embedder key must name the vector space: {key}"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

// ---------------------------------------------------------------------------
// Fault paths (o-prime review 2026-08-26)

/// Finding 1: a worker that dies mid-job leaves its row `running` forever.
///
/// There is no lease and no heartbeat, so nothing else can move it, and
/// `claim_job` only looks at `pending`. The compounding half is worse than the
/// stall: `scan_file` dedupes on `(worktree, path)`, so the wedged row absorbs
/// every future `add` or `scan` of that file — `enqueue_job`'s `ON CONFLICT`
/// bumps the payload and deadline but can never change the state. One SIGKILL
/// during a large index would make those files permanently unindexable, and the
/// scan would keep reporting success.
///
/// A kill-mid-job test would need a second process; this proves the same states
/// by claiming a job and abandoning it, which is exactly what the corpse looks
/// like.
#[tokio::test]
async fn a_job_abandoned_by_a_dead_worker_is_recovered_at_boot() {
    let stack = Stack::create("bootsweep").await;

    let job = roots::ScanFileJob {
        worktree_id: 1,
        identity: "path:/srv/api".to_string(),
        path: "src/lib.rs".to_string(),
        blob: "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".to_string(),
    };
    let payload = serde_json::to_value(&job).unwrap();
    fs3_store::enqueue_job(
        &stack.state.db,
        roots::SCAN_FILE,
        &job.dedupe_key(),
        &payload,
        std::time::Duration::ZERO,
    )
    .await
    .expect("enqueue");

    // Claim it and abandon it — the state a killed worker leaves behind.
    let claimed = fs3_store::claim_job(&stack.state.db, runner::KINDS)
        .await
        .expect("claim")
        .expect("there is a job");
    assert_eq!(claimed.attempts, 1);

    // The wedge: nothing can claim it, and re-enqueueing cannot free it.
    assert!(
        fs3_store::claim_job(&stack.state.db, runner::KINDS)
            .await
            .expect("claim")
            .is_none(),
        "a row left running is invisible to every future claim"
    );
    fs3_store::enqueue_job(
        &stack.state.db,
        roots::SCAN_FILE,
        &job.dedupe_key(),
        &payload,
        std::time::Duration::ZERO,
    )
    .await
    .expect("re-enqueue");
    assert_eq!(
        stack.count("SELECT count(*) FROM jobs").await,
        1,
        "the re-add collapsed into the wedged row rather than making a new one — this is why \
         the stall becomes permanent unindexability"
    );
    assert!(
        fs3_store::claim_job(&stack.state.db, runner::KINDS)
            .await
            .expect("claim")
            .is_none(),
        "and it is STILL unclaimable, so the file can never be scanned again"
    );

    // Boot recovery: exactly what the daemon does before spawning its runner.
    let swept = fs3_store::requeue_running(&stack.state.db)
        .await
        .expect("the sweep runs");
    assert_eq!(swept, 1);

    let recovered = fs3_store::claim_job(&stack.state.db, runner::KINDS)
        .await
        .expect("claim")
        .expect("the job is claimable again");
    assert_eq!(recovered.id, claimed.id, "the same row, not a duplicate");
    assert_eq!(
        recovered.attempts, 2,
        "the attempt is counted, so a job that keeps killing its worker is visible as such \
         rather than retrying forever"
    );

    stack.destroy().await;
}

/// Finding 2: the content-addressed skips must RE-EMIT their downstream work.
///
/// The parse and its enrichment are written in separate transactions, so a
/// crash, an outage or a retry in the window between them leaves elements that
/// no summarize or embed job will ever be enqueued for — paid for, stored, and
/// invisible to search, with nothing reporting a problem. This reproduces that
/// window exactly: the elements exist, the enrichment queue is empty, and the
/// scan job runs again.
#[tokio::test]
async fn a_scan_whose_parse_already_landed_still_enqueues_its_enrichment() {
    let fixture = Fixture::create("reemit");
    let stack = Stack::create("reemit").await;
    let path = fixture.path().to_string_lossy().to_string();

    call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    stack.drain().await;

    let elements = stack.count("SELECT count(*) FROM elements").await;
    assert!(elements > 0, "the first pass parsed the tree");

    // Reproduce the crash window: the parse is durable, everything downstream
    // of it is gone, and nothing remembers that it was owed. `worktree_files`
    // goes too, because that map is what makes `add` decide a file is
    // unchanged — leaving it would mean the scan never re-runs and the test
    // would prove nothing about the skip.
    for table in ["smart_content", "embeddings_1024", "jobs", "worktree_files"] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&stack.state.db)
            .await
            .expect("clearing the enrichment window");
    }

    // Re-run. Every file re-queues, and every scan_file job will take the
    // content-addressed skip, because the elements are already there.
    let again = call(
        &stack.state,
        "POST",
        "/scan",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    assert!(again.ok, "rescan failed: {:?}", again.error);
    assert!(
        again.data.expect("data")["enqueued"].as_u64().unwrap() > 0,
        "the path map was cleared, so the files re-queue"
    );

    stack.drain().await;

    assert_eq!(
        stack.count("SELECT count(*) FROM elements").await,
        elements,
        "the parse was skipped — no element was written a second time"
    );
    assert!(
        stack.count("SELECT count(*) FROM smart_content").await > 0,
        "the SKIP must still enqueue the summaries the stored tree earns, or they are lost \
         forever with nothing reporting it"
    );
    assert!(
        stack.count("SELECT count(*) FROM embeddings_1024").await > 0,
        "and the vectors, or the content is invisible to search"
    );

    stack.destroy().await;
}

/// The other half of finding 2: a summarize job whose summary already exists
/// must still enqueue that summary's vector. Otherwise a retry that lands after
/// the summary was stored — but before its embed was queued — leaves a summary
/// that was paid for and can never be found.
#[tokio::test]
async fn a_summary_that_already_exists_still_gets_its_vector_enqueued() {
    let fixture = Fixture::create("smartreemit");
    let stack = Stack::create("smartreemit").await;
    let path = fixture.path().to_string_lossy().to_string();

    call(
        &stack.state,
        "POST",
        "/roots",
        Some(serde_json::json!({ "path": path })),
    )
    .await;
    stack.drain().await;

    let summaries = stack.count("SELECT count(*) FROM smart_content").await;
    assert!(summaries > 0, "the first pass summarised");

    // The window: summaries durable, their vectors gone.
    sqlx::query("DELETE FROM embeddings_1024 WHERE source_kind = 'smart'")
        .execute(&stack.state.db)
        .await
        .expect("clearing the smart vectors");
    sqlx::query("DELETE FROM jobs")
        .execute(&stack.state.db)
        .await
        .expect("clearing the queue");
    assert_eq!(
        stack
            .count("SELECT count(*) FROM embeddings_1024 WHERE source_kind = 'smart'")
            .await,
        0
    );

    // Re-run one summarize job for a summary that already exists.
    let raw_hash: String = sqlx::query("SELECT raw_hash FROM smart_content LIMIT 1")
        .fetch_one(&stack.state.db)
        .await
        .expect("a summary exists")
        .try_get("raw_hash")
        .expect("raw_hash is text");

    // Built from the real type, not hand-rolled JSON: `Element::raw_hash` is a
    // private DERIVED field that is nonetheless part of the wire format, so a
    // literal that omits it fails to deserialise and the job dies terminally —
    // which would make this test pass for the wrong reason if it asserted only
    // that no second summary appeared.
    let job = serde_json::to_value(fs3_daemon::enrich::SummarizeJob {
        identity: "git:github.com/AI-Substrate/flowspace3".to_string(),
        raw_hash: raw_hash.clone(),
        element: fs3_core::Element::new(
            fs3_core::ElementKind::Function,
            "function_item",
            "f",
            "src/auth.rs::f",
            fs3_core::Span::new(1, 3),
            "fn f() {}",
        ),
    })
    .expect("a summarize job always serialises");
    fs3_store::enqueue_job(
        &stack.state.db,
        "summarize",
        "summarize:reemit",
        &job,
        std::time::Duration::ZERO,
    )
    .await
    .expect("enqueue");

    stack.drain().await;

    assert_eq!(
        stack.count("SELECT count(*) FROM smart_content").await,
        summaries,
        "the summary was skipped — no LLM call was paid for a second time"
    );
    assert!(
        stack
            .count("SELECT count(*) FROM embeddings_1024 WHERE source_kind = 'smart'")
            .await
            > 0,
        "but its vector WAS re-emitted, so the summary is reachable by search again"
    );

    stack.destroy().await;
}
