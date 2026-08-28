//! `status --watch` is the one stdout path that is NDJSON, not an envelope.

use axum::Router;
use axum::body::Body;
use axum::http::{Response, header};
use axum::routing::get;
use std::process::Command;

const STREAM: &str = concat!(
    "{\"stream\":\"fs3.events\",\"v\":1,\"daemon\":\"0.4.0\",\"heartbeat_ms\":15000}\n",
    "{\"v\":1,\"at\":\"2026-08-28T03:11:05.001Z\",\"kind\":\"job_done\",\"job\":\"scan_file\",\"subject\":\"src/lib.rs\",\"ms\":12,\"left\":0}\n",
);

async fn spawn() -> String {
    let app = Router::new().route(
        "/events",
        get(|| async {
            Response::builder()
                .header(header::CONTENT_TYPE, "application/x-ndjson")
                .body(Body::from(STREAM))
                .expect("response")
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let address = listener.local_addr().expect("bound address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serves") });
    format!("http://{address}")
}

#[tokio::test]
async fn json_watch_copies_daemon_ndjson_byte_for_byte() {
    let url = spawn().await;
    let output = Command::new(env!("CARGO_BIN_EXE_flowspace3"))
        .args(["status", "--watch", "--json", "--daemon-url", &url])
        .output()
        .expect("runs flowspace3");

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, STREAM.as_bytes());
    assert!(output.stderr.is_empty());
}

#[tokio::test]
async fn human_watch_is_a_terse_feed_not_an_envelope() {
    let url = spawn().await;
    let output = Command::new(env!("CARGO_BIN_EXE_flowspace3"))
        .args(["status", "--watch", "--human", "--daemon-url", &url])
        .output()
        .expect("runs flowspace3");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.starts_with("watching fs3.events v1"), "{stdout}");
    assert!(stdout.contains("done scan_file src/lib.rs (12ms, 0 left)"), "{stdout}");
    assert!(!stdout.contains("\"ok\""), "watch is never envelope-shaped: {stdout}");
}
