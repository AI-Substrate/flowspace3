//! Serve a router on an ephemeral port and hand back its base URL.

use axum::Router;

/// Bind `127.0.0.1:0`, serve `router` on a background task, return its base URL.
#[allow(dead_code)]
pub async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral port should be available");
    let address = listener.local_addr().expect("the socket is bound");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server runs");
    });

    format!("http://{address}")
}

/// A fresh directory under the system temp dir. Unique per call, so tests that
/// run in parallel never share one.
#[allow(dead_code)]
pub fn temp_dir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let path = std::env::temp_dir().join(format!(
        "fs3-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).expect("creating a temp config directory");
    path
}
