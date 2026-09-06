//! Spend is defended by actual job counts, not inferred from cursor positions.
//! Same-size identity rotation uses the existing Unix reader identity contract.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn job_counts(pool: &fs3_store::PgPool) -> BTreeMap<String, i64> {
    let mut counts = BTreeMap::new();
    for row in fs3_store::queue_depth_history(pool).await.unwrap() {
        if matches!(row.kind.as_str(), "ingest_session" | "summarize" | "embed") {
            *counts.entry(row.kind).or_default() += row.depth;
        }
    }
    counts
}

fn receipt(status: &Value) -> Option<&Value> {
    status["data"]["conversations"]["harnesses"]
        .as_array()?
        .iter()
        .find(|row| row["harness"] == "claude")?
        .get("newest_ingest")
        .filter(|value| !value.is_null())
}

async fn wait_for_idle(
    client: &reqwest::Client,
    base: &str,
    config: &Path,
    pool: &fs3_store::PgPool,
    expected: impl Fn(&Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        assert!(
            Instant::now() < deadline,
            "expected receipt/idle state not observed: {}",
            std::fs::read_to_string(config.join("daemon.log")).unwrap_or_default()
        );
        if let Ok(key) = std::fs::read_to_string(fs3_core::daemon_key_path(config))
            && let Ok(response) = client
                .get(format!("{base}/status"))
                .bearer_auth(key.trim())
                .send()
                .await
            && let Ok(status) = response.json::<Value>().await
            && status["ok"] == true
            && expected(&status)
            && fs3_store::queue_depth(pool).await.unwrap().is_empty()
        {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn record(folder: &Path, ordinal: &str, text: &str) -> String {
    format!(
        "{}\n",
        json!({"type":"user","uuid":ordinal,"cwd":folder,
        "timestamp":"2026-09-06T00:00:00Z","message":{"role":"user","content":text}})
    )
}

#[tokio::test]
async fn unchanged_and_rescanned_sessions_create_no_new_llm_jobs_and_append_spends_once() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path();
    let database =
        fs3_testkit::FreshDatabase::create_from(&fs3_testkit::test_database_url(), "convo-spend")
            .await
            .unwrap();
    let pool = fs3_store::connect(&database.url()).await.unwrap();
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", reservation.local_addr().unwrap());
    let mut config = fs3_core::Config::default();
    config.database.url = database.url();
    config.daemon.url = base.clone();
    config.daemon.log_dir = config_dir.join("logs").to_string_lossy().into_owned();
    config.indexing.conversation_poll_ticks = 1;
    config.indexing.turn_summary_min_bytes = 1;
    config.update.auto = false;
    std::fs::write(
        config_dir.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let mut command = fs3_testkit::sealed(
        Path::new(env!("CARGO_BIN_EXE_flowspace3")),
        config_dir,
        fs3_testkit::TestDatabase::FromConfigFile,
    );
    let home = config_dir.join("home");
    let folder = home.join("workspace");
    std::fs::create_dir_all(&folder).unwrap();
    let session = "018-spend-session";
    let source = home
        .join(".claude/projects")
        .join(fs3_daemon::convo_ingest::workspace_slug(
            fs3_core::Harness::Claude,
            &folder,
            &home,
        ))
        .join(format!("{session}.jsonl"));
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    let original = record(
        &folder,
        "turn-one",
        "First distinct user turn earns a summary and its raw vector.",
    ) + &record(
        &folder,
        "turn-two",
        "Second distinct turn has different text and a different summary address.",
    );
    std::fs::write(&source, &original).unwrap();
    let log_path = config_dir.join("daemon.log");
    let log = std::fs::File::create(&log_path).unwrap();
    drop(reservation);
    let daemon = Daemon(
        command
            .arg("daemon")
            .current_dir(config_dir)
            .env("HOME", &home)
            .env("RUST_LOG", "fs3_daemon=debug")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    // a. Fully ingested: two summary jobs, one batched raw embed + two smart embeds.
    let initial = wait_for_idle(&client, &base, config_dir, &pool, |status| {
        receipt(status).is_some_and(|report| report["turns_new"] == 2)
    })
    .await;
    let baseline = job_counts(&pool).await;
    assert_eq!(
        baseline,
        BTreeMap::from([
            ("ingest_session".to_owned(), 1),
            ("summarize".to_owned(), 2),
            ("embed".to_owned(), 3)
        ])
    );
    assert_eq!(receipt(&initial).unwrap()["records_read"], 2);
    let initial_poll = initial["data"]["conversations"]["last_poll_at"].clone();

    // b. A genuine subsequent poll, with the file unchanged, spends nothing.
    wait_for_idle(&client, &base, config_dir, &pool, |status| {
        status["data"]["conversations"]["last_poll_at"] != initial_poll
    })
    .await;
    assert_eq!(
        job_counts(&pool).await,
        baseline,
        "unchanged poll must create zero ingest/summarize/embed jobs"
    );

    // c. Replace identity without changing any content. The existing reader rescans.
    let replacement = source.with_extension("replacement");
    std::fs::copy(&source, &replacement).unwrap();
    std::fs::rename(&replacement, &source).unwrap();
    let rescanned = wait_for_idle(&client, &base, config_dir, &pool, |status| {
        receipt(status).is_some_and(|report| report["rescanned"] == true)
    })
    .await;
    let after_rescan = job_counts(&pool).await;
    assert_eq!(
        after_rescan["summarize"], baseline["summarize"],
        "ledger dedupe must prevent NEW summarize jobs on rescan"
    );
    assert_eq!(
        after_rescan["embed"], baseline["embed"],
        "ledger dedupe must prevent NEW embed jobs on rescan"
    );
    assert_eq!(
        after_rescan["ingest_session"],
        baseline["ingest_session"] + 1
    );
    let rescan = receipt(&rescanned).unwrap();
    assert_eq!(rescan["records_read"], 2);
    assert_eq!(rescan["turns_new"], 0);
    assert_eq!(rescan["deduped"], 2);
    assert_eq!(rescan["summarized"], 0);

    // d. Exactly one new turn: one summary and its raw + smart embed jobs.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&source)
        .unwrap()
        .write_all(
            record(
                &folder,
                "turn-three",
                "Only this appended third turn may create new enrichment work.",
            )
            .as_bytes(),
        )
        .unwrap();
    let appended = wait_for_idle(&client, &base, config_dir, &pool, |status| {
        receipt(status)
            .is_some_and(|report| report["turns_new"] == 1 && report["rescanned"] == false)
    })
    .await;
    let after_append = job_counts(&pool).await;
    assert_eq!(
        after_append["ingest_session"],
        after_rescan["ingest_session"] + 1
    );
    assert_eq!(after_append["summarize"], after_rescan["summarize"] + 1);
    assert_eq!(after_append["embed"], after_rescan["embed"] + 2);
    let append = receipt(&appended).unwrap();
    assert_eq!(append["records_read"], 1);
    assert_eq!(append["deduped"], 0);
    assert_eq!(append["summarized"], 1);

    let address =
        fs3_daemon::convo_ingest::conversation_guid(fs3_core::Harness::Claude, session).address();
    let logs = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<_> = logs
        .lines()
        .filter(|line| line.contains("ingested session file"))
        .collect();
    assert_eq!(
        lines.len(),
        3,
        "one info receipt per actual session-file ingest, no line for unchanged poll: {logs}"
    );
    assert!(lines.iter().all(|line| line.contains("INFO")
        && line.contains(&format!("address={address}"))
        && line.contains(&format!("subject={address}"))));
    assert!(lines.iter().any(|line| line.contains("records_read=2")
        && line.contains("turns_new=0")
        && line.contains("deduped=2")
        && line.contains("summarized=0")
        && line.contains("rescanned=true")));
    assert!(logs.lines().any(|line| line.contains("done")
        && line.contains("kind=ingest_session")
        && line.contains(&format!("subject={address}"))));
    assert!(logs.lines().any(|line| line.contains("DEBUG")
        && line.contains("polled native conversations")
        && line.contains("enqueued=0")
        && line.contains("behind=0")
        && line.contains("skipped=0")));
    assert!(!logs.lines().any(|line| line.contains("INFO")
        && line.contains("polled native conversations")
        && line.contains("enqueued=0")
        && line.contains("behind=0")));
    println!(
        "spend counts: initial={baseline:?}; rescan={after_rescan:?}; append={after_append:?}"
    );
    drop(daemon);
    let restarted = Daemon(command.spawn().unwrap());
    wait_for_idle(&client, &base, config_dir, &pool, |status| {
        receipt(status).is_none() && status["data"]["conversations"]["last_poll_at"].is_string()
    })
    .await;
    assert_eq!(
        job_counts(&pool).await,
        after_append,
        "restart cannot invent a receipt or re-index already ingested files"
    );
    drop(restarted);
    database.destroy(pool).await;
}
