//! Loopback authentication for every daemon HTTP request.
//!
//! The daemon mints one opaque bearer key per boot and atomically publishes it
//! in the resolved config directory before the listener is bound. The CLI reads
//! that file for each request, so a daemon restart rotates credentials without
//! leaving a long-lived client stuck on stale state.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use fs3_core::catalog;
use fs3_core::envelope::{Envelope, Failure};

/// Authentication state shared by the router middleware.
#[derive(Clone)]
pub struct Auth {
    key: Arc<str>,
    key_path: Arc<PathBuf>,
}

impl Auth {
    /// Build authentication state from an already-published key.
    ///
    /// Production uses [`generate`]; this constructor lets isolated test
    /// daemons use their own explicit config directory without mutating the
    /// process-global environment.
    pub fn new(key: impl Into<String>, key_path: PathBuf) -> Self {
        Self {
            key: Arc::from(key.into()),
            key_path: Arc::new(key_path),
        }
    }

    /// The file clients must read.
    #[must_use]
    pub fn key_path(&self) -> &Path {
        &self.key_path
    }
}

/// Mint and atomically publish a fresh 256-bit key for this daemon boot.
///
/// The temporary file is created in the destination directory, restricted to
/// the owner, flushed, synced, and renamed over the prior key. The listener is
/// bound only after this function returns.
pub fn generate(config_dir: &Path) -> Result<Auth> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating config directory {}", config_dir.display()))?;

    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).context("generating the daemon authentication key")?;
    let mut key = String::with_capacity(random.len() * 2);
    for byte in random {
        use std::fmt::Write as _;
        write!(&mut key, "{byte:02x}").expect("writing to a String cannot fail");
    }

    let key_path = fs3_core::daemon_key_path(config_dir);
    let mut temporary = tempfile::NamedTempFile::new_in(config_dir)
        .with_context(|| format!("creating a temporary key beside {}", key_path.display()))?;
    restrict_to_owner(temporary.as_file(), &key_path)?;
    temporary
        .write_all(key.as_bytes())
        .with_context(|| format!("writing {}", key_path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("syncing {}", key_path.display()))?;
    temporary
        .persist(&key_path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing {} atomically", key_path.display()))?;

    Ok(Auth::new(key, key_path))
}

#[cfg(unix)]
fn restrict_to_owner(file: &std::fs::File, path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting {} to mode 0600", path.display()))
}

#[cfg(not(unix))]
fn restrict_to_owner(_file: &std::fs::File, _path: &Path) -> Result<()> {
    Ok(())
}

/// Reject every request that does not carry this boot's bearer key.
pub async fn require(State(auth): State<Auth>, request: Request<Body>, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    if presented == Some(auth.key.as_ref()) {
        return next.run(request).await;
    }

    let key_path = auth.key_path.display().to_string();
    let fix = format!(
        "read the current key from {key_path} and send it as `Authorization: Bearer <key>`; \
         if the file is missing or stale, restart the fs3 daemon so it republishes the key"
    );
    let envelope = Envelope::<serde_json::Value>::failed(
        "authenticate",
        Failure::new(
            &catalog::DAEMON_UNAUTHORIZED,
            "the request did not present this daemon boot's authentication key",
        )
        .with_fix(&fix)
        .with_detail("key_file", &key_path),
    )
    .with_next_action(fix);

    (axum::http::StatusCode::UNAUTHORIZED, Json(envelope)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;

    async fn server() -> (String, String, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("an isolated config directory");
        let auth = generate(directory.path()).expect("publishing the isolated daemon key");
        let key = auth.key.to_string();
        let app = Router::new()
            .route(
                "/health",
                get(|| async { Json(serde_json::json!({ "status": "ok" })) }),
            )
            .layer(axum::middleware::from_fn_with_state(auth, require));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an ephemeral listener");
        let address = listener.local_addr().expect("the bound address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serves") });
        (format!("http://{address}"), key, directory)
    }

    #[test]
    fn generation_publishes_a_fresh_mode_0600_key() {
        let directory = tempfile::tempdir().expect("an isolated config directory");
        let first = generate(directory.path()).expect("the first key");
        let first_bytes = std::fs::read_to_string(first.key_path()).expect("reading the key");
        assert_eq!(first_bytes.len(), 64, "256 bits encoded as lowercase hex");
        assert!(first_bytes.bytes().all(|byte| byte.is_ascii_hexdigit()));

        let second = generate(directory.path()).expect("the rotated key");
        let second_bytes = std::fs::read_to_string(second.key_path()).expect("reading the key");
        assert_ne!(
            first_bytes, second_bytes,
            "each boot rotates the credential"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(second.key_path())
                .expect("key metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[tokio::test]
    async fn missing_key_is_a_401_envelope_naming_the_key_file() {
        let (base, _key, directory) = server().await;
        let response = reqwest::get(format!("{base}/health"))
            .await
            .expect("the daemon answers");
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        let envelope: Envelope = response.json().await.expect("an envelope");
        assert_eq!(
            envelope.error.as_ref().map(|error| error.code.as_str()),
            Some("FS3-E-DAEMON-UNAUTHORIZED")
        );
        assert!(envelope.next_action.as_deref().is_some_and(|next| {
            next.contains(
                &fs3_core::daemon_key_path(directory.path())
                    .display()
                    .to_string(),
            )
        }));
    }

    #[tokio::test]
    async fn wrong_key_is_rejected() {
        let (base, _key, _directory) = server().await;
        let response = reqwest::Client::new()
            .get(format!("{base}/health"))
            .bearer_auth("wrong")
            .send()
            .await
            .expect("the daemon answers");
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn current_key_reaches_the_route() {
        let (base, key, _directory) = server().await;
        let response = reqwest::Client::new()
            .get(format!("{base}/health"))
            .bearer_auth(key)
            .send()
            .await
            .expect("the daemon answers");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.text().await.expect("response bytes"),
            r#"{"status":"ok"}"#,
            "authentication must not alter successful response bytes"
        );
    }
}
