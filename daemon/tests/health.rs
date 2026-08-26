//! Exemplar: the daemon integration tier.
//!
//! Boots the real router over a real socket with `provider = "fake"` — the
//! offline configuration a fresh machine gets — and asks it the question the
//! CLI asks. No database is required, which is the point of wiring the pool
//! lazily.

use fs3_core::Config;
use fs3_daemon::{AppState, http};

mod support;

#[tokio::test]
async fn health_returns_200_and_status_ok_under_the_fake_provider() {
    let config = Config::from_toml_str(
        r#"
        [embedder]
        provider = "fake"

        [summarizer]
        provider = "fake"
        "#,
    )
    .expect("the offline configuration must parse");

    let state = AppState::from_config(config).expect("the fake arms need nothing to wire");
    let base = support::spawn(http::router(state)).await;

    let response = reqwest::get(format!("{base}/health"))
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

/// The daemon must start and report health with no database reachable — the
/// pool is lazy on purpose, so `flowspace3 ping` can tell "daemon down" from
/// "Postgres down" instead of conflating them.
#[tokio::test]
async fn the_daemon_serves_health_without_a_reachable_database() {
    let config = Config::from_toml_str(
        r#"
        [database]
        url = "postgres://nobody:nobody@127.0.0.1:1/nothing"
        "#,
    )
    .expect("config parses");

    let state = AppState::from_config(config).expect("a lazy pool needs no connection");
    let base = support::spawn(http::router(state)).await;

    let body: serde_json::Value = reqwest::get(format!("{base}/health"))
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

/// The `flowspace3` binary from this workspace's target directory.
///
/// `CARGO_BIN_EXE_*` only covers bins in *this* package, and the CLI lives in
/// another. Under the mandated gate (`cargo test --all`) cargo builds every
/// workspace binary before running any test, so this path is populated.
fn cli_binary() -> std::path::PathBuf {
    let mut directory = std::env::current_exe().expect("the test binary has a path");
    directory.pop(); // the test executable itself
    directory.pop(); // deps/
    let candidate = directory.join(format!("flowspace3{}", std::env::consts::EXE_SUFFIX));
    assert!(
        candidate.is_file(),
        "{} is missing. This test drives the real CLI, so build the workspace \
         first: cargo build --workspace",
        candidate.display()
    );
    candidate
}

/// The end-to-end proof that AC-0005 holds for the binaries that actually ship.
///
/// Every test above builds `Config`, `AppState` and `Router` by hand, so
/// `daemon/src/main.rs` — config discovery, the bind-address guard, the bin
/// target itself — never runs. Replacing main's discovered configuration with
/// defaults would leave them all green. This one starts the real daemon binary
/// against a real `FS3_CONFIG_DIR` and then asks the real `flowspace3` binary
/// how it is, with **no `--daemon-url`**: the answer can only arrive if both
/// binaries discovered and honoured the same config file.
#[tokio::test]
async fn the_real_binaries_agree_through_a_discovered_config() {
    let port = free_port();
    let directory = support::temp_dir("discovered-config");
    std::fs::write(
        directory.join("config.toml"),
        format!(
            "[daemon]\nurl = \"http://127.0.0.1:{port}\"\n\n\
             [embedder]\nprovider = \"fake\"\n\n\
             [summarizer]\nprovider = \"fake\"\n"
        ),
    )
    .expect("writing the config the binaries must discover");

    let mut daemon = Daemon(
        std::process::Command::new(env!("CARGO_BIN_EXE_fs3-daemon"))
            .env(fs3_core::CONFIG_DIR_ENV, &directory)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("the daemon binary should start"),
    );

    // Readiness is observed, never assumed. A port that never opens is a
    // failure with the reason attached, not a hang.
    let health = format!("http://127.0.0.1:{port}/health");
    let mut answered = None;
    for _ in 0..100 {
        if let Ok(exited) = daemon.0.try_wait()
            && let Some(status) = exited
        {
            panic!("the daemon exited before serving {health}: {status}");
        }
        if let Ok(response) = reqwest::get(&health).await
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
    let ping = std::process::Command::new(cli_binary())
        .arg("ping")
        .env(fs3_core::CONFIG_DIR_ENV, &directory)
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
