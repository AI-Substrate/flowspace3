//! Exemplar: the daemon integration tier.
//!
//! Boots the real router over a real socket with both ports selecting the
//! built-in `fake` provider instance — the
//! offline configuration a fresh machine gets — and asks it the question the
//! CLI asks. The router needs no database, which is the point of wiring the
//! pool lazily.

use fs3_core::Config;
use fs3_daemon::{AppState, http};

mod support;

#[tokio::test]
async fn health_returns_200_and_status_ok_under_the_fake_provider() {
    let config = Config::from_toml_str(
        r#"
        [embedder]
        active = "fake"

        [summarizer]
        active = "fake"
        "#,
    )
    .expect("the offline configuration must parse");

    let state = AppState::from_config(config).expect("the fake arms need nothing to wire");
    let auth = support::auth("health-fake");
    let base = support::spawn(http::router(state, auth.auth)).await;

    let response = reqwest::Client::new()
        .get(format!("{base}/health"))
        .bearer_auth(&auth.key)
        .send()
        .await
        .expect("the daemon should answer");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("health is JSON");
    assert_eq!(body["status"], "ok");
    assert_eq!(
        body["embedder"], "fake",
        "the composition root selected the fake arm"
    );
    assert_eq!(body["summarizer"], "fake");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

/// The router must keep reporting health with no database reachable — the pool
/// is lazy on purpose, so a database outage does not take the HTTP surface with
/// it and `flowspace3 ping` can tell "daemon down" from "Postgres down".
///
/// This is the *runtime* property. Boot is stricter: `main` migrates the store
/// once and exits nonzero if it cannot, because the daemon is the single writer
/// (see `docs/how/database.md`).
#[tokio::test]
async fn the_router_serves_health_without_a_reachable_database() {
    let config = Config::from_toml_str(
        r#"
        [database]
        url = "postgres://nobody:nobody@127.0.0.1:1/nothing"
        "#,
    )
    .expect("config parses");

    let state = AppState::from_config(config).expect("a lazy pool needs no connection");
    let auth = support::auth("health-no-database");
    let base = support::spawn(http::router(state, auth.auth)).await;

    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("{base}/health"))
        .bearer_auth(&auth.key)
        .send()
        .await
        .expect("the daemon should answer")
        .json()
        .await
        .expect("health is JSON");
    assert_eq!(body["status"], "ok");
}

/// Kills the daemon however this test ends — assertion failure included.
struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// An ephemeral port, released again before the daemon claims it.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("an ephemeral port should be available")
        .local_addr()
        .expect("the socket is bound")
        .port()
}

/// The end-to-end proof that AC-0005 holds for the binaries that actually ship.
///
/// Every test above builds `Config`, `AppState` and `Router` by hand, so
/// `boot.rs` — config discovery, the bind-address guard, the subcommand wiring
/// — never runs. Replacing boot's discovered configuration with defaults would
/// leave them all green. This one starts the real daemon (`flowspace3 daemon`)
/// against a real `FS3_CONFIG_DIR` and then asks the real `flowspace3 ping` how
/// it is, with **no `--daemon-url`**: the answer can only arrive if both
/// invocations discovered and honoured the same config file.
///
/// Since PRD req 51 those are the SAME binary, which is the point — a CLI and a
/// daemon of different vintages can no longer meet.
///
/// # Blast radius
///
/// This test is why `fs3_testkit::spawn` exists. Until 2026-08-27 it spawned
/// the daemon with `FS3_CONFIG_DIR` alone, against the `config.toml` below —
/// which has no `[database]` section, so the child resolved
/// `DatabaseConfig::DEFAULT_URL`. That is the SHIPPED address, which on a
/// developer machine is the real store, and daemon boot MIGRATES before it
/// serves. Migration 0012 reached Jordan's production database that way and
/// took the installed CLI down on schema skew.
///
/// `Scratch` rather than `Unreachable`: boot exits non-zero when it cannot
/// migrate, so a daemon pointed at nothing would fail to exist rather than
/// fail the assertion under test.
#[tokio::test]
async fn the_real_binaries_agree_through_a_discovered_config() {
    let port = free_port();
    let directory = support::temp_dir("discovered-config");
    std::fs::write(
        directory.join("config.toml"),
        format!(
            "[daemon]\nurl = \"http://127.0.0.1:{port}\"\n\n\
             [embedder]\nactive = \"fake\"\n\n\
             [summarizer]\nactive = \"fake\"\n"
        ),
    )
    .expect("writing the config the binaries must discover");

    let mut daemon = Daemon(
        fs3_testkit::sealed(
            &fs3_testkit::flowspace3_binary(),
            &directory,
            fs3_testkit::TestDatabase::Scratch,
        )
        .arg("daemon")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the daemon binary should start"),
    );

    // Readiness is observed, never assumed. The daemon publishes daemon.key
    // before binding, so any open listener must already accept those bytes.
    let health = format!("http://127.0.0.1:{port}/health");
    let client = reqwest::Client::new();
    let mut answered = None;
    for _ in 0..100 {
        if let Ok(exited) = daemon.0.try_wait()
            && let Some(status) = exited
        {
            panic!("the daemon exited before serving {health}: {status}");
        }
        let key = std::fs::read_to_string(fs3_core::daemon_key_path(&directory));
        if let Ok(key) = key
            && let Ok(response) = client.get(&health).bearer_auth(key.trim()).send().await
            && response.status() == 200
        {
            answered = Some(response.json::<serde_json::Value>().await.expect("JSON"));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let body = answered.unwrap_or_else(|| {
        panic!("the real daemon never served {health} — it did not honour FS3_CONFIG_DIR")
    });
    assert_eq!(body["status"], "ok");
    assert_eq!(
        body["embedder"], "fake",
        "the binary's own composition root selected the fake arm"
    );

    // No --daemon-url: the CLI has to find the same port the same way.
    // `ping` opens no pool, so the store it is pointed at is irrelevant to the
    // assertion — which is exactly why it must still be pinned rather than
    // left to default.
    let ping = fs3_testkit::sealed(
        &fs3_testkit::flowspace3_binary(),
        &directory,
        fs3_testkit::TestDatabase::Unreachable,
    )
    .arg("ping")
    .output()
    .expect("the flowspace3 binary should run");

    let stdout = String::from_utf8_lossy(&ping.stdout).to_string();
    let stderr = String::from_utf8_lossy(&ping.stderr).to_string();
    assert!(
        ping.status.success(),
        "flowspace3 ping failed ({}):\n{stdout}{stderr}",
        ping.status
    );
    assert!(
        stdout.contains("healthy"),
        "ping should report health, got: {stdout}"
    );
    assert!(
        stdout.contains(&port.to_string()),
        "ping should name the discovered port {port}, got: {stdout}"
    );
    assert!(
        stdout.contains("embedder: fake"),
        "ping should name the wired provider, got: {stdout}"
    );
}
