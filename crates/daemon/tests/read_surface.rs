//! The read surface, end to end: `get`, `tree`, and D6's cwd scoping.
//!
//! Every test drives the REAL router and the REAL runner against a real git
//! fixture, because the claims here are about what is actually stored — that a
//! whole file comes back whole, that one address can legitimately match two
//! elements, and that a search run inside a checkout is about THAT repository.
//!
//! The fixture is deliberately tiny: `struct Rect` and `impl Rect` share one
//! address by design (workshop 002), which is the shape that makes ambiguity a
//! first-class answer rather than a bug.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use fs3_core::envelope::Envelope;
use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::wiring::AppState;
use fs3_daemon::{router, runner};
use fs3_testkit::fakes::{FakeEmbedder, FakeSummarizer};
use serde_json::Value;

/// A git repository with two files worth indexing.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn create(label: &str, remote: Option<&str>) -> Self {
        let root = support::temp_dir(label);

        // The content carries its label, and that is load-bearing rather than
        // decorative: enrichment and vectors are keyed by the HASH of the text
        // (workshop 002 D2), so two fixtures with byte-identical files share
        // one vector and one representative path. A test about two
        // repositories would then be a test about one.
        write(
            &root,
            "src/geometry.rs",
            &format!(
                r#"
/// A rectangle in screen space, as {label} draws it.
pub struct Rect {{
    pub width: f64,
    pub height: f64,
}}

impl Rect {{
    /// The area of the rectangle, in square pixels ({label}).
    pub fn area(&self) -> f64 {{
        self.width * self.height
    }}
}}
"#
            ),
        );
        write(
            &root,
            "README.md",
            &format!("# Fixture {label}\n\n## Shapes\n\nRectangles have an area.\n"),
        );

        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "fixture@fs3.test"]);
        git(&root, &["config", "user.name", "fs3 fixture"]);
        if let Some(url) = remote {
            git(&root, &["remote", "add", "origin", url]);
        }
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "--quiet", "-m", "fixture"]);

        Fixture { root }
    }

    fn path(&self) -> String {
        std::fs::canonicalize(&self.root)
            .expect("the fixture exists")
            .to_string_lossy()
            .to_string()
    }
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("a file has a parent"))
        .expect("creating fixture directories");
    std::fs::write(path, contents.trim_start()).expect("writing a fixture file");
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .unwrap_or_else(|error| panic!("running git {args:?}: {error}"));
    assert!(status.success(), "git {args:?} failed");
}

/// A throwaway database with the fake providers wired, so the whole thing runs
/// offline.
struct Stack {
    database: support::FreshDatabase,
    state: AppState,
}

impl Stack {
    async fn create(label: &str) -> Self {
        let database = support::FreshDatabase::create(label).await;
        let config = Config {
            database: DatabaseConfig {
                url: database.url(),
            },
            ..Config::default()
        };
        let mut state = AppState::from_config(config).expect("the fake stack wires");
        fs3_store::migrate(&state.db).await.expect("migrates");
        state.embedder = Arc::new(FakeEmbedder {
            dimensions: fs3_store::EMBEDDING_DIMENSIONS,
            ..FakeEmbedder::default()
        });
        state.summarizer = Arc::new(FakeSummarizer::default());
        Stack { database, state }
    }

    /// Register a root and drain everything it queued.
    async fn index(&self, path: &str) {
        let envelope = self
            .call(
                "POST",
                "/roots",
                Some(serde_json::json!({ "path": path })),
                &[],
            )
            .await;
        assert!(envelope.ok, "adding {path} failed: {:?}", envelope.error);

        for _ in 0..8 {
            if runner::drain(&self.state, 4).await.total() == 0 {
                break;
            }
        }
    }

    /// Store the exact tree produced by the pure ddoc parser, without relying
    /// on the composition-owned scan dispatch.
    async fn index_ddoc(&self, path: &str, source: &str) -> (String, fs3_core::BlobRef) {
        let tree = fs3_parsers::scan_ddoc(Path::new(path), source.as_bytes(), None)
            .expect("the ddoc fixture parses");
        let address = tree
            .root
            .iter()
            .find(|element| element.kind == fs3_core::ElementKind::Row)
            .expect("the fixture contains a row")
            .address
            .clone();
        let identity = fs3_core::RepoIdentity::from_path(Path::new("/srv/read-ddoc"));
        let worktree =
            fs3_store::register_worktree(&self.state.db, &identity, "/srv/read-ddoc", Some("main"))
                .await
                .expect("registering ddoc fixture root");
        fs3_store::sync_worktree_files(
            &self.state.db,
            worktree,
            &[(path.to_string(), tree.blob.clone())],
        )
        .await
        .expect("mapping ddoc fixture path");
        fs3_store::upsert_element_tree(
            &self.state.db,
            &tree.blob,
            fs3_daemon::scan::PARSER_VERSION,
            &tree.root,
            |_| false,
        )
        .await
        .expect("storing parsed ddoc tree");
        (address, tree.blob)
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        query: &[(&str, &str)],
    ) -> Envelope {
        let auth = support::auth("read-surface-call");
        let base = support::spawn(router(self.state.clone(), auth.auth)).await;
        let client = reqwest::Client::new();
        let url = format!("{base}{path}");
        let request = match method {
            "POST" => client.post(&url).json(&body.unwrap_or(Value::Null)),
            _ => client.get(&url).query(query),
        }
        .bearer_auth(&auth.key);
        let response = request.send().await.expect("the daemon answers");
        let status = response.status();
        let envelope: Envelope = response.json().await.expect("an envelope");
        assert_eq!(
            status.as_u16(),
            envelope.http_status(),
            "the HTTP status must be the one the envelope's code implies (workshop 004 D4)"
        );
        envelope
    }

    async fn get(&self, query: &[(&str, &str)]) -> Envelope {
        self.call("GET", "/get", None, query).await
    }

    async fn refs(&self, query: &[(&str, &str)]) -> Envelope {
        self.call("GET", "/refs", None, query).await
    }

    async fn tree(&self, query: &[(&str, &str)]) -> Envelope {
        self.call("GET", "/tree", None, query).await
    }

    async fn search(&self, query: &[(&str, &str)]) -> Envelope {
        self.call("GET", "/search", None, query).await
    }

    async fn identity(&self) -> String {
        fs3_store::repo_identities(&self.state.db)
            .await
            .expect("identities")
            .first()
            .cloned()
            .expect("the fixture registered a repository")
    }

    async fn destroy(self) {
        let pool = self.state.db.clone();
        self.database.destroy(pool).await;
    }
}

fn data(envelope: &Envelope) -> &Value {
    envelope
        .data
        .as_ref()
        .unwrap_or_else(|| panic!("expected data, got {:?}", envelope.error))
}

fn code(envelope: &Envelope) -> String {
    envelope
        .error
        .as_ref()
        .map(|failure| failure.code.clone())
        .unwrap_or_else(|| panic!("expected a failure, got {:?}", envelope.data))
}

// ---------------------------------------------------------------------------
// get

/// The headline: an address from a search hit reads back as real content, with
/// the structure around it — which is what replaces shelling out to `cat`.
#[tokio::test]
async fn get_reads_one_element_with_its_children_and_parents() {
    let stack = Stack::create("read_get_element").await;
    let fixture = Fixture::create("read-get-element", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let address = format!("el:{identity}/src/geometry.rs::Rect::area");
    let envelope = stack.get(&[("address", address.as_str())]).await;
    let element = data(&envelope);

    assert_eq!(element["name"], "area");
    assert_eq!(element["kind"], "function");
    assert_eq!(element["path"], "src/geometry.rs");
    assert!(
        element["raw_text"]
            .as_str()
            .expect("raw text")
            .contains("self.width * self.height"),
        "the body must be the element's real source: {element:#?}"
    );

    // The parent chain is what makes a fetched element navigable back up.
    let parents: Vec<&str> = element["parents"]
        .as_array()
        .expect("a parent chain")
        .iter()
        .map(|parent| parent["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(parents, vec!["geometry.rs", "Rect"]);

    assert!(
        envelope.next_action.is_some(),
        "every envelope steers (PRD req 44)"
    );
    stack.destroy().await;
}

#[tokio::test]
async fn get_by_dd_address_resolves_the_same_row_the_parser_produced() {
    let stack = Stack::create("read_get_ddoc_row").await;
    let (address, _) = stack
        .index_ddoc(
            "docs/plan.dd.json",
            r#"{
                "dd": {"schema": "builder/plan"},
                "sections": [{
                    "name": "acceptance_criteria",
                    "value": [{
                        "id": "ac-0001",
                        "claim": "Agents can resolve this row",
                        "state": "unchecked"
                    }]
                }]
            }"#,
        )
        .await;
    assert_eq!(address, "docs/plan.dd.json#acceptance_criteria/ac-0001");

    let envelope = stack.get(&[("address", address.as_str())]).await;
    let row = data(&envelope);
    assert_eq!(row["address"], address);
    assert_eq!(row["kind"], "row");
    assert_eq!(row["name"], "ac-0001");
    assert!(
        row["raw_text"]
            .as_str()
            .expect("row text")
            .contains("Agents can resolve this row")
    );
    stack.destroy().await;
}

#[tokio::test]
async fn refs_with_no_rows_is_a_successful_empty_answer() {
    let stack = Stack::create("refs_empty").await;
    let _ = stack
        .index_ddoc(
            "docs/plan.dd.json",
            r#"{
                "dd": {"schema": "builder/plan"},
                "sections": [{"name": "acceptance_criteria", "value": [
                    {"id": "ac-0001", "claim": "No file edge yet", "state": "unchecked"}
                ]}]
            }"#,
        )
        .await;

    let envelope = stack.refs(&[("path", "src/lib.rs")]).await;
    assert!(
        envelope.ok,
        "empty inverse lookup is not an error: {:?}",
        envelope.error
    );
    assert_eq!(data(&envelope)["results"], serde_json::json!([]));
    assert!(
        envelope
            .next_action
            .as_deref()
            .is_some_and(|next| next.contains("successful empty answer"))
    );
    stack.destroy().await;
}

#[tokio::test]
async fn refs_returns_the_source_rows_pasteable_dd_address() {
    let stack = Stack::create("refs_cited").await;
    let (address, blob) = stack
        .index_ddoc(
            "docs/plan.dd.json",
            r#"{
                "dd": {"schema": "builder/plan"},
                "sections": [{"name": "acceptance_criteria", "value": [
                    {"id": "ac-0001", "claim": "Covers source", "state": "unchecked"}
                ]}]
            }"#,
        )
        .await;
    fs3_store::replace_file_refs(
        &stack.state.db,
        &blob,
        fs3_daemon::scan::PARSER_VERSION,
        &[fs3_store::DdocFileRef {
            element_id: 0,
            address: address.clone(),
            path: "src/lib.rs".to_string(),
            rel: "ref".to_string(),
            location: "$.sections[0].value[0].source".to_string(),
        }],
    )
    .await
    .expect("attach file ref");

    let envelope = stack.refs(&[("path", "src/lib.rs")]).await;
    let results = data(&envelope)["results"].as_array().expect("ref results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["address"], address);
    assert_eq!(results[0]["path"], "src/lib.rs");
    assert_eq!(results[0]["rel"], "ref");
    stack.destroy().await;
}

/// C3, proven rather than asserted in prose: a whole-file address returns the
/// file WHOLE, byte for byte with what was indexed — not a reconstruction from
/// the elements, which would silently drop everything between declarations.
#[tokio::test]
async fn get_on_a_file_address_returns_the_file_as_indexed() {
    let stack = Stack::create("read_get_file").await;
    let fixture = Fixture::create("read-get-file", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let address = format!("el:{identity}/src/geometry.rs");
    let envelope = stack.get(&[("address", address.as_str())]).await;
    let element = data(&envelope);

    let on_disk = std::fs::read_to_string(Path::new(&fixture.path()).join("src/geometry.rs"))
        .expect("the fixture file");
    assert_eq!(element["kind"], "file");
    assert_eq!(
        element["raw_text"].as_str().expect("raw text"),
        on_disk,
        "a whole-file get must be the whole file"
    );
    stack.destroy().await;
}

/// Workshop 002's design consequence, met head on: `struct Rect` and
/// `impl Rect` are two elements at ONE address. Answering with either one
/// silently would be a coin flip presented as a fact.
#[tokio::test]
async fn one_address_two_elements_is_ambiguous_and_span_picks_one() {
    let stack = Stack::create("read_get_ambiguous").await;
    let fixture = Fixture::create("read-get-ambiguous", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let address = format!("el:{identity}/src/geometry.rs::Rect");
    let envelope = stack.get(&[("address", address.as_str())]).await;

    assert_eq!(code(&envelope), "FS3-E-QUERY-INVALID-AMBIGUOUS");
    let failure = envelope.error.as_ref().expect("a failure");
    let candidates = failure.details["candidates"]
        .as_array()
        .expect("the candidates are structured, not only prose");
    assert_eq!(candidates.len(), 2, "struct Rect and impl Rect");
    assert!(
        failure.fix.contains("--span"),
        "and the fix must name the lever that resolves it: {}",
        failure.fix
    );

    // The lever works: the second declaration starts where the first does not.
    let line = candidates
        .iter()
        .filter_map(|candidate| candidate.as_str())
        .find_map(|candidate| {
            candidate
                .rsplit_once("--span ")
                .map(|(_, span)| span.trim_end_matches(')').to_string())
        })
        .expect("a candidate names its span");

    let picked = stack
        .get(&[("address", address.as_str()), ("span", line.as_str())])
        .await;
    assert!(picked.ok, "--span resolves it: {:?}", picked.error);
    assert_eq!(
        data(&picked)["span"][0].as_u64(),
        line.parse::<u64>().ok(),
        "the element that came back is the one --span named"
    );
    stack.destroy().await;
}

/// An unknown address must say what IS there — a 404 that only says "no" makes
/// the caller guess, and guessing is what search exists to stop.
#[tokio::test]
async fn an_unknown_element_names_what_the_file_does_hold() {
    let stack = Stack::create("read_get_unknown").await;
    let fixture = Fixture::create("read-get-unknown", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let address = format!("el:{identity}/src/geometry.rs::Circle");
    let envelope = stack.get(&[("address", address.as_str())]).await;

    assert_eq!(code(&envelope), "FS3-E-QUERY-NOT-FOUND");
    let failure = envelope.error.as_ref().expect("a failure");
    let found = failure.details["found_here"]
        .as_array()
        .expect("what was found nearby");
    assert!(
        found
            .iter()
            .filter_map(Value::as_str)
            .any(|entry| entry.contains("Rect")),
        "the nearby names must be real ones: {found:?}"
    );
    stack.destroy().await;
}

/// An unknown PATH is the other half, and the message must not pretend the
/// file was never indexed when the truth is that the scope excluded it.
#[tokio::test]
async fn an_unknown_path_is_a_not_found_with_neighbours() {
    let stack = Stack::create("read_get_unknown_path").await;
    let fixture = Fixture::create("read-get-unknown-path", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let address = format!("el:{identity}/src/nothing.rs");
    let envelope = stack.get(&[("address", address.as_str())]).await;

    assert_eq!(code(&envelope), "FS3-E-QUERY-NOT-FOUND");
    let nearby = envelope.error.as_ref().expect("a failure").details["nearby"]
        .as_array()
        .expect("neighbours");
    assert!(
        nearby
            .iter()
            .filter_map(Value::as_str)
            .any(|path| path == "src/geometry.rs"),
        "the sibling that DOES exist must be named: {nearby:?}"
    );
    stack.destroy().await;
}

/// The dispatch arm the conversations plan reserved is now FILLED, so a `conv:`
/// address reaches the conversation store rather than a 501. A guid nothing
/// answers to is an ordinary not-found — the same answer an unknown element
/// address gets, because it is the same kind of mistake.
#[tokio::test]
async fn an_unknown_conversation_is_a_not_found_with_a_way_forward() {
    let stack = Stack::create("read_get_conv").await;

    let envelope = stack
        .get(&[("address", "conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8")])
        .await;

    assert_eq!(code(&envelope), "FS3-E-QUERY-NOT-FOUND");
    assert_eq!(envelope.http_status(), 404);
    let failure = envelope.error.as_ref().expect("a failure");
    assert!(
        failure.details.contains_key("guid"),
        "the address is understood well enough to name its parts: {failure:?}"
    );
    assert!(
        failure.fix.contains("conversation list"),
        "and the fix names how to find one that exists: {failure:?}"
    );
    stack.destroy().await;
}

/// A guid that is not a uuid is a different mistake from one that is simply not
/// indexed, and it gets the address-shaped code rather than not-found.
#[tokio::test]
async fn a_malformed_conversation_guid_is_an_address_error() {
    let stack = Stack::create("read_get_badconv").await;

    let envelope = stack
        .get(&[("address", "conv:2f1c-not-a-real-conversation")])
        .await;

    assert_eq!(code(&envelope), "FS3-E-QUERY-INVALID-ADDRESS");
    assert_eq!(envelope.http_status(), 400);
    stack.destroy().await;
}

/// Something that is not an address at all is a different failure from an
/// address that does not resolve — confusing the two sends the caller to fix
/// the wrong thing.
#[tokio::test]
async fn a_non_address_is_refused_as_invalid_rather_than_missing() {
    let stack = Stack::create("read_get_nonsense").await;

    let envelope = stack.get(&[("address", "crates/store/src/lib.rs")]).await;

    assert_eq!(code(&envelope), "FS3-E-QUERY-INVALID-ADDRESS");
    assert_eq!(envelope.http_status(), 400);
    stack.destroy().await;
}

// ---------------------------------------------------------------------------
// tree

/// Structure browse over a repository: directories are derived from the paths
/// that are actually indexed, so what you see is what fs3 can answer about.
#[tokio::test]
async fn tree_on_a_repository_lists_its_directories_and_files() {
    let stack = Stack::create("read_tree_repo").await;
    let fixture = Fixture::create("read-tree-repo", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let envelope = stack.tree(&[("repo", identity.as_str())]).await;
    let result = data(&envelope);

    assert_eq!(result["kind"], "repository");
    let names: Vec<&str> = result["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"src"), "the source directory: {names:?}");
    assert!(names.contains(&"README.md"), "and a file: {names:?}");
    assert_eq!(result["total"], 2, "two indexed files in the fixture");
    stack.destroy().await;
}

/// Structure browse over one file: its declarations, addressed, so the next
/// step is a `get` rather than a guess.
#[tokio::test]
async fn tree_on_a_file_lists_its_declarations() {
    let stack = Stack::create("read_tree_file").await;
    let fixture = Fixture::create("read-tree-file", None);
    stack.index(&fixture.path()).await;
    let identity = stack.identity().await;

    let target = format!("el:{identity}/src/geometry.rs");
    let envelope = stack.tree(&[("address", target.as_str())]).await;
    let result = data(&envelope);

    assert_eq!(result["kind"], "file");
    let entries = result["entries"].as_array().expect("entries");
    assert!(
        entries.iter().any(|entry| entry["name"] == "Rect"),
        "the declarations in the file: {entries:#?}"
    );
    assert!(
        entries.iter().all(|entry| entry["address"]
            .as_str()
            .is_some_and(|a| a.starts_with("el:"))),
        "and every row carries the address to get it with"
    );
    stack.destroy().await;
}

/// With nothing named and nowhere to stand, `tree` is the index itself — the
/// orientation move for an agent that has just arrived.
#[tokio::test]
async fn tree_with_no_target_lists_the_indexed_repositories() {
    let stack = Stack::create("read_tree_index").await;
    let fixture = Fixture::create("read-tree-index", None);
    stack.index(&fixture.path()).await;

    let envelope = stack.tree(&[]).await;
    let result = data(&envelope);

    assert_eq!(result["kind"], "index");
    assert_eq!(result["total"], 1);
    assert_eq!(result["entries"][0]["kind"], "repository");
    assert_eq!(result["entries"][0]["files"], 2);
    stack.destroy().await;
}

// ---------------------------------------------------------------------------
// D6: what a bare search is about

/// The scoping claim: standing inside a registered root, a bare search is
/// about THAT repository — and the envelope says so rather than leaving it to
/// be inferred from which results turned up.
#[tokio::test]
async fn a_search_inside_a_registered_root_scopes_to_that_repository() {
    let stack = Stack::create("read_scope_cwd").await;
    let mine = Fixture::create("read-scope-mine", None);
    let other = Fixture::create("read-scope-other", None);
    stack.index(&mine.path()).await;
    stack.index(&other.path()).await;

    let here = mine.path();
    let envelope = stack
        .search(&[("q", "the area of a rectangle"), ("cwd", here.as_str())])
        .await;

    let scope = &envelope.meta.as_ref().expect("meta carries the scope")["scope"];
    assert_eq!(scope["source"], "cwd", "scoped by where the caller stands");
    let repo = scope["repo"].as_str().expect("a repository");

    for hit in data(&envelope)["results"].as_array().expect("results") {
        assert_eq!(
            hit["repo"].as_str(),
            Some(repo),
            "a scoped search must not answer from another repository: {hit:#?}"
        );
    }
    assert!(
        scope["warnings"].is_null(),
        "and standing in a registered root is the healthy case, with nothing to warn about"
    );
    stack.destroy().await;
}

/// `--repo all` is the widen lever, and it must actually widen.
#[tokio::test]
async fn repo_all_widens_back_to_every_repository() {
    let stack = Stack::create("read_scope_all").await;
    let mine = Fixture::create("read-scope-all-mine", None);
    let other = Fixture::create("read-scope-all-other", None);
    stack.index(&mine.path()).await;
    stack.index(&other.path()).await;

    let here = mine.path();
    let envelope = stack
        .search(&[
            ("q", "the area of a rectangle"),
            ("cwd", here.as_str()),
            ("repo", "all"),
            ("limit", "20"),
        ])
        .await;

    let scope = &envelope.meta.as_ref().expect("meta")["scope"];
    assert_eq!(scope["source"], "all");
    assert!(scope["repo"].is_null());

    let repos: std::collections::BTreeSet<String> = data(&envelope)["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(|hit| hit["repo"].as_str().map(ToString::to_string))
        .collect();
    assert!(
        repos.len() > 1,
        "widening must reach both fixtures, got {repos:?}"
    );
    stack.destroy().await;
}

/// The confusion this packet was written for: a search run from a directory
/// fs3 has never been told about used to answer, in full confidence, from an
/// unrelated repository — with nothing saying the current one was absent.
#[tokio::test]
async fn a_search_from_an_unindexed_directory_says_so() {
    let stack = Stack::create("read_scope_unindexed").await;
    let indexed = Fixture::create("read-scope-indexed", None);
    stack.index(&indexed.path()).await;

    let elsewhere = Fixture::create("read-scope-elsewhere", None);
    let here = elsewhere.path();
    let envelope = stack
        .search(&[("q", "the area of a rectangle"), ("cwd", here.as_str())])
        .await;

    let scope = &envelope.meta.as_ref().expect("meta")["scope"];
    assert_eq!(scope["source"], "all", "nothing better was known");
    let warning = scope["warnings"][0]
        .as_str()
        .expect("the miss must be named, not silent");
    assert!(
        warning.contains("flowspace3 add"),
        "and it must name the command that fixes it: {warning}"
    );
    assert!(
        envelope
            .next_action
            .as_deref()
            .expect("a steer")
            .contains("add"),
        "the warning must reach a consumer that reads only next_action"
    );
    stack.destroy().await;
}

/// The other miss, and the subtler one: the caller IS in a checkout of an
/// indexed repository — just not the checkout that was registered. Scoping to
/// the repository is what they meant; saying whose bytes answered is what
/// keeps it honest.
#[tokio::test]
async fn a_second_checkout_of_an_indexed_repo_scopes_but_says_whose_content_answered() {
    let stack = Stack::create("read_scope_sibling").await;
    let remote = "https://github.com/fixture/read-scope.git";
    let registered = Fixture::create("read-scope-registered", Some(remote));
    let sibling = Fixture::create("read-scope-sibling", Some(remote));
    stack.index(&registered.path()).await;

    let here = sibling.path();
    let envelope = stack
        .search(&[("q", "the area of a rectangle"), ("cwd", here.as_str())])
        .await;

    let scope = &envelope.meta.as_ref().expect("meta")["scope"];
    assert_eq!(
        scope["source"], "cwd",
        "the repository is indexed, so scoping to it is right"
    );
    assert_eq!(scope["repo"], "git:github.com/fixture/read-scope");
    let warning = scope["warnings"][0]
        .as_str()
        .expect("an unregistered checkout is worth saying out loud");
    assert!(
        warning.contains(&registered.path()),
        "and it must name the checkout that DID answer: {warning}"
    );
    stack.destroy().await;
}

/// Naming a repository that is not indexed must not look like an empty index.
#[tokio::test]
async fn an_unknown_repo_filter_is_warned_about_rather_than_answered_silently() {
    let stack = Stack::create("read_scope_unknown_repo").await;
    let fixture = Fixture::create("read-scope-unknown-repo", None);
    stack.index(&fixture.path()).await;

    let envelope = stack
        .search(&[
            ("q", "the area of a rectangle"),
            ("repo", "git:github.com/nobody/nothing"),
        ])
        .await;

    let scope = &envelope.meta.as_ref().expect("meta")["scope"];
    assert_eq!(scope["source"], "flag");
    let warning = scope["warnings"][0].as_str().expect("a warning");
    assert!(
        warning.contains("no repository with identity") && warning.contains("flowspace3 status"),
        "a filter naming nothing must say so, and say how to see what IS there: {warning}"
    );
    stack.destroy().await;
}

/// `get` is scoped too: a repo-less address resolves where the caller stands,
/// which is what makes an address copied out of a log usable from a checkout.
#[tokio::test]
async fn a_repo_less_address_resolves_in_the_repository_the_caller_is_in() {
    let stack = Stack::create("read_get_scoped").await;
    let fixture = Fixture::create("read-get-scoped", None);
    stack.index(&fixture.path()).await;

    let here = fixture.path();
    let envelope = stack
        .get(&[
            ("address", "el:src/geometry.rs::Rect::area"),
            ("cwd", here.as_str()),
        ])
        .await;

    assert!(envelope.ok, "expected a hit: {:?}", envelope.error);
    assert_eq!(data(&envelope)["name"], "area");
    stack.destroy().await;
}

/// `get` resolves a repo-less address wherever it can — including from a
/// directory fs3 has never indexed. When it does, the answer came from
/// somebody else's repository, and that has to reach a consumer reading only
/// `data` and `next_action`: a warning that lives in `meta` alone is a warning
/// an agent never sees.
#[tokio::test]
async fn a_get_from_an_unindexed_directory_leads_its_steer_with_the_warning() {
    let stack = Stack::create("read_get_unscoped_steer").await;
    let indexed = Fixture::create("read-get-steer-indexed", None);
    stack.index(&indexed.path()).await;

    let elsewhere = Fixture::create("read-get-steer-elsewhere", None);
    let here = elsewhere.path();
    let envelope = stack
        .get(&[
            ("address", "el:src/geometry.rs::Rect::area"),
            ("cwd", here.as_str()),
        ])
        .await;

    assert!(
        envelope.ok,
        "the address still resolves: {:?}",
        envelope.error
    );

    let warning = envelope.meta.as_ref().expect("meta carries the scope")["scope"]["warnings"][0]
        .as_str()
        .expect("an unindexed directory is worth saying out loud");
    let steer = envelope.next_action.as_deref().expect("a steer");
    assert!(
        steer.starts_with(warning),
        "the warning must LEAD the steer, the way search and tree do:\n  steer: {steer}\n  \
         warning: {warning}"
    );
    stack.destroy().await;
}
