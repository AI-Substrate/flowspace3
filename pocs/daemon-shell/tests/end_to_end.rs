//! End-to-end: real HTTP, real OS watcher, real files.
//!
//! The unit tests in `core` prove the debounce ALGEBRA in microseconds. This
//! file proves the parts that only a real filesystem can: that `notify` reports
//! what we think it reports, that canonicalisation lines events up with roots,
//! and that the shell wires the two together.
//!
//! Debounce here is deliberately short (400ms, not the 10s default) so the
//! suite stays fast; the 10s default is proved by hand in `LEARNINGS.md`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use daemon_shell::Supervisor;
use serde_json::Value;

/// Boot a server on an ephemeral port and hand back its address.
async fn start(debounce_ms: u64) -> (SocketAddr, Arc<Supervisor>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut tx = Some(tx);
        let _ = daemon_shell::serve(
            "127.0.0.1:0".parse().expect("literal address"),
            Duration::from_millis(debounce_ms),
            move |bound, supervisor| {
                let _ = tx
                    .take()
                    .expect("bind callback runs once")
                    .send((bound, supervisor));
            },
        )
        .await;
    });
    rx.await.expect("server bound")
}

/// Poll `GET /dirty` until it reports AT LEAST `expected` paths, or give up.
///
/// Polling rather than sleeping a fixed time: the assertion is "it settles".
///
/// At-least, not exactly, because whether the OS also reports the PARENT
/// DIRECTORY of a written file is not deterministic — the same 100-file burst
/// produced 101 entries under `/var/folders` and 100 under a repo-local path
/// on the same machine. Tests that pin an exact count on that are flaky by
/// construction; the exact claims are made afterwards, on the paths.
async fn wait_for_dirty(
    client: &reqwest::Client,
    address: SocketAddr,
    expected: usize,
    within: Duration,
) -> Value {
    let deadline = std::time::Instant::now() + within;
    let mut last = Value::Null;
    while std::time::Instant::now() < deadline {
        last = read_dirty(client, address).await;
        if last["count"].as_u64().expect("count") >= expected as u64 {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("dirty set never reached {expected} entries; last was {last}");
}

async fn read_dirty(client: &reqwest::Client, address: SocketAddr) -> Value {
    client
        .get(format!("http://{address}/dirty"))
        .send()
        .await
        .expect("GET /dirty")
        .json()
        .await
        .expect("dirty json")
}

fn paths_of(report: &Value) -> Vec<PathBuf> {
    report["dirty"]
        .as_array()
        .expect("dirty array")
        .iter()
        .map(|entry| PathBuf::from(entry["path"].as_str().expect("path string")))
        .collect()
}

/// `tempfile` hands back `/var/folders/...` on macOS, which is a symlink into
/// `/private/var`. Events arrive under the resolved path, so the expectation
/// has to be resolved too — the same reason the supervisor canonicalises.
fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().expect("resolving temp dir")
}

#[tokio::test]
async fn health_and_status_answer_before_anything_is_watched() {
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    let health: Value = client
        .get(format!("http://{address}/health"))
        .send()
        .await
        .expect("GET /health")
        .json()
        .await
        .expect("health json");
    assert_eq!(health["status"], "ok");

    let status: Value = client
        .get(format!("http://{address}/status"))
        .send()
        .await
        .expect("GET /status")
        .json()
        .await
        .expect("status json");
    assert_eq!(status["debounce_ms"], 400);
    assert_eq!(status["roots"].as_array().expect("roots").len(), 0);
    assert_eq!(status["total_dirty"], 0);
}

#[tokio::test]
async fn a_written_file_becomes_dirty_after_the_window() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    let added = client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");
    assert_eq!(added.status(), reqwest::StatusCode::CREATED);
    assert_eq!(
        PathBuf::from(
            added.json::<Value>().await.expect("watch json")["root"]
                .as_str()
                .expect("root string")
        ),
        root,
        "the daemon reports the canonical root it will actually attribute events to"
    );

    std::fs::write(root.join("hello.txt"), "one").expect("write");

    let report = wait_for_dirty(&client, address, 1, Duration::from_secs(10)).await;
    assert_eq!(paths_of(&report), vec![root.join("hello.txt")]);
    assert_eq!(report["pending"], 0);
}

#[tokio::test]
async fn a_hundred_file_burst_coalesces_and_settles_together() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");

    let burst = root.join("burst");
    std::fs::create_dir(&burst).expect("mkdir");
    // Deliberate pause before writing INTO the new directory. On Linux,
    // `RecursiveMode::Recursive` is implemented by walking the tree and adding
    // one inotify watch per directory, so a directory created a moment ago is
    // not yet watched. Without this pause the first files written into it are
    // never reported — proved, and pinned down, by the race test below.
    tokio::time::sleep(Duration::from_millis(500)).await;

    for index in 0..100 {
        std::fs::write(burst.join(format!("f{index:03}.txt")), "x").expect("write");
    }

    // Every one of the 100 files must arrive. The parent directory MAY also
    // arrive — see `wait_for_dirty` — so the count is a floor and the claim
    // that matters is made per path.
    let report = wait_for_dirty(&client, address, 100, Duration::from_secs(15)).await;
    let paths = paths_of(&report);
    for index in 0..100 {
        let expected = burst.join(format!("f{index:03}.txt"));
        assert!(
            paths.contains(&expected),
            "{} never went dirty",
            expected.display()
        );
    }
    assert!(
        paths.iter().all(|p| p.starts_with(&burst)),
        "nothing outside the burst directory leaked in: {paths:?}"
    );

    let events: u64 = report["dirty"]
        .as_array()
        .expect("dirty array")
        .iter()
        .map(|entry| entry["events"].as_u64().expect("events"))
        .sum();
    assert!(
        events >= 100,
        "every file produced at least one raw event ({events} folded into {} entries)",
        paths.len()
    );
}

/// The recursive-watch race, stated as a contract instead of a comment.
///
/// Writing into a directory the instant it is created loses per-file events on
/// Linux (inotify has not been told about the new directory yet) and keeps them
/// on macOS (one FSEvents stream already covers the subtree). Neither is a bug
/// to fix in the watcher — it is the backend.
///
/// What IS guaranteed on both is the recovery signal: the new DIRECTORY goes
/// dirty. So the real daemon's rule has to be "a dirty directory means re-list
/// that directory", not "a dirty path means re-read that file". This test
/// asserts only the portable part.
#[tokio::test]
async fn a_directory_created_and_written_in_one_breath_still_reports_the_directory() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");

    let fresh = root.join("fresh");
    std::fs::create_dir(&fresh).expect("mkdir");
    for index in 0..20 {
        std::fs::write(fresh.join(format!("r{index:02}.txt")), "x").expect("write");
    }

    let report = wait_for_dirty(&client, address, 1, Duration::from_secs(15)).await;
    let paths = paths_of(&report);
    assert!(
        paths.contains(&fresh),
        "the new directory itself must be reported on every backend; got {paths:?}"
    );
}

#[tokio::test]
async fn writes_inside_ignored_directories_never_go_dirty() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");

    for noisy in [".git", "target", "node_modules"] {
        let directory = root.join(noisy);
        std::fs::create_dir(&directory).expect("mkdir");
        std::fs::write(directory.join("noise.bin"), "noise").expect("write");
    }
    std::fs::write(root.join("real.rs"), "fn main() {}").expect("write");

    // Only `real.rs` may appear. If an ignored path leaked in, the count would
    // overshoot and this would fail on the timeout with the offender named.
    let report = wait_for_dirty(&client, address, 1, Duration::from_secs(10)).await;
    assert_eq!(paths_of(&report), vec![root.join("real.rs")]);

    let status: Value = client
        .get(format!("http://{address}/status"))
        .send()
        .await
        .expect("GET /status")
        .json()
        .await
        .expect("status json");
    assert!(
        status["roots"][0]["ignored_events"]
            .as_u64()
            .expect("ignored_events")
            > 0,
        "the ignored writes were seen by the OS and dropped by us, not invisible"
    );
}

#[tokio::test]
async fn get_dirty_is_idempotent_and_delete_is_the_acknowledgement() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");
    std::fs::write(root.join("a.txt"), "a").expect("write");

    let first = wait_for_dirty(&client, address, 1, Duration::from_secs(10)).await;
    assert_eq!(paths_of(&first), vec![root.join("a.txt")]);
    // Read twice: a GET that consumed its own answer would make this zero.
    let second = read_dirty(&client, address).await;
    assert_eq!(second["count"], 1);

    let drained: Value = client
        .delete(format!("http://{address}/dirty"))
        .send()
        .await
        .expect("DELETE /dirty")
        .json()
        .await
        .expect("drain json");
    assert_eq!(drained["drained"], 1);

    assert_eq!(
        read_dirty(&client, address).await["count"],
        0,
        "DELETE is the acknowledgement, and it is what empties the set"
    );
}

#[tokio::test]
async fn overlapping_roots_are_refused_with_the_root_they_collide_with() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    let nested = root.join("inner");
    std::fs::create_dir(&nested).expect("mkdir");
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");

    let conflict = client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": nested }))
        .send()
        .await
        .expect("POST /watch nested");
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
    let body: Value = conflict.json().await.expect("conflict json");
    assert_eq!(body["conflict"], "covered_by");
    assert_eq!(PathBuf::from(body["with"].as_str().expect("with")), root);

    let duplicate = client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch duplicate");
    assert_eq!(duplicate.status(), reqwest::StatusCode::CONFLICT);
    assert_eq!(
        duplicate.json::<Value>().await.expect("json")["conflict"],
        "duplicate"
    );
}

#[tokio::test]
async fn watching_a_path_that_is_not_a_directory_is_the_callers_fault() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let file = scratch.path().join("not-a-dir.txt");
    std::fs::write(&file, "x").expect("write");
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    let refused = client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": file }))
        .send()
        .await
        .expect("POST /watch file");
    assert_eq!(refused.status(), reqwest::StatusCode::BAD_REQUEST);

    let missing = client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": scratch.path().join("nope") }))
        .send()
        .await
        .expect("POST /watch missing");
    assert_eq!(missing.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(
        missing.json::<Value>().await.expect("json")["error"]
            .as_str()
            .expect("error string")
            .contains("resolving"),
        "the error names the layer that refused"
    );
}

#[tokio::test]
async fn unwatching_stops_events_and_discards_that_roots_work() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let root = resolved(scratch.path());
    // 3s window: long enough that the file is still PENDING when the root is
    // removed, which is the removal race worth proving.
    let (address, _supervisor) = start(3_000).await;
    let client = reqwest::Client::new();

    client
        .post(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("POST /watch");
    std::fs::write(root.join("inflight.txt"), "x").expect("write");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let removed = client
        .delete(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("DELETE /watch");
    assert_eq!(removed.status(), reqwest::StatusCode::OK);

    // Well past the window the pending file would have settled in.
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    assert_eq!(
        read_dirty(&client, address).await["count"],
        0,
        "an unwatched root must not hand out work after removal"
    );

    let again = client
        .delete(format!("http://{address}/watch"))
        .json(&serde_json::json!({ "path": root }))
        .send()
        .await
        .expect("DELETE /watch again");
    assert_eq!(again.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_root_can_be_added_at_runtime_after_another_is_removed() {
    let first = tempfile::tempdir().expect("temp dir");
    let second = tempfile::tempdir().expect("temp dir");
    let (address, _supervisor) = start(400).await;
    let client = reqwest::Client::new();

    for directory in [first.path(), second.path()] {
        let response = client
            .post(format!("http://{address}/watch"))
            .json(&serde_json::json!({ "path": resolved(directory) }))
            .send()
            .await
            .expect("POST /watch");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::CREATED,
            "disjoint roots coexist"
        );
    }

    std::fs::write(second.path().join("b.txt"), "b").expect("write");
    let report = wait_for_dirty(&client, address, 1, Duration::from_secs(10)).await;
    assert_eq!(
        paths_of(&report),
        vec![resolved(second.path()).join("b.txt")]
    );

    let status: Value = client
        .get(format!("http://{address}/status"))
        .send()
        .await
        .expect("GET /status")
        .json()
        .await
        .expect("status json");
    assert_eq!(status["roots"].as_array().expect("roots").len(), 2);
}
