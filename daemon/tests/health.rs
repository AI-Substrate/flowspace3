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
