//! Loopback authentication for every daemon HTTP request.
//!
//! Each boot stages a fresh opaque bearer key beside its destination. Boot
//! binds the listener first, then atomically publishes the staged bytes before
//! the accept loop starts. A failed bind therefore cannot clobber the live
//! daemon's key, and no request can reach a daemon whose key is unpublished.

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
    published_mtime: Option<std::time::SystemTime>,
    owner_port: Option<u16>,
}

impl Auth {
    /// Build authentication state from an already-published key.
    ///
    /// Production uses [`generate`]; this constructor lets isolated test
    /// daemons use their own explicit config directory without mutating the
    /// process-global environment.
    pub fn new(key: impl Into<String>, key_path: PathBuf) -> Self {
        let published_mtime = std::fs::metadata(&key_path)
            .and_then(|metadata| metadata.modified())
            .ok();
        Self {
            key: Arc::from(key.into()),
            key_path: Arc::new(key_path),
            published_mtime,
            owner_port: None,
        }
    }

    /// The file clients must read.
    #[must_use]
    pub fn key_path(&self) -> &Path {
        &self.key_path
    }
}
/// A listener whose socket bind has already succeeded.
///
/// The private field is the publication proof: callers cannot construct this
/// token from an address or a boolean, only from Tokio's bound-listener type.
pub(crate) struct BoundListener(tokio::net::TcpListener);

impl BoundListener {
    pub(crate) fn new(listener: tokio::net::TcpListener) -> Self {
        Self(listener)
    }

    pub(crate) fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.0.local_addr()
    }

    pub(crate) fn into_inner(self) -> tokio::net::TcpListener {
        self.0
    }
}

/// A fresh daemon credential staged beside its eventual destination.
///
/// Holding this value has no externally visible effect: [`publish`](Self::publish)
/// is the only operation that replaces `daemon.key`.
pub(crate) struct StagedAuth {
    key: String,
    key_path: PathBuf,
    temporary: tempfile::NamedTempFile,
}

impl StagedAuth {
    /// Atomically publish the staged key after the listener has been bound.
    ///
    /// Boot must call this before starting the accept loop. That order preserves
    /// both edge invariants: a bind loser leaves the winner's key untouched, and
    /// a bound daemon never answers with a key clients cannot yet read.
    pub(crate) fn publish(self, listener: &BoundListener) -> Result<Auth> {
        self.temporary
            .persist(&self.key_path)
            .map_err(|error| error.error)
            .with_context(|| format!("publishing {} atomically", self.key_path.display()))?;
        let published_mtime = std::fs::metadata(&self.key_path)
            .and_then(|metadata| metadata.modified())
            .with_context(|| {
                format!(
                    "reading {} mtime after publication",
                    self.key_path.display()
                )
            })?;
        let owner_port = listener
            .local_addr()
            .context("reading the bound listener after key publication")?
            .port();

        Ok(Auth {
            key: Arc::from(self.key),
            key_path: Arc::new(self.key_path),
            published_mtime: Some(published_mtime),
            owner_port: Some(owner_port),
        })
    }
}

/// Mint and stage a fresh 256-bit key without publishing it.
///
/// The temporary file is created in the destination directory, restricted to
/// the owner, flushed, and synced. Dropping the result removes only that
/// unpublished temporary file.
pub(crate) fn stage(config_dir: &Path) -> Result<StagedAuth> {
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

    Ok(StagedAuth {
        key,
        key_path,
        temporary,
    })
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
    let key_newer_than_daemon = auth.published_mtime.is_some_and(|published_mtime| {
        std::fs::metadata(auth.key_path.as_ref())
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|current_mtime| current_mtime > published_mtime)
    });
    let fix = if key_newer_than_daemon {
        let owner = auth.owner_port.map_or_else(
            || "the configured port".to_string(),
            |port| format!(":{port}"),
        );
        format!(
            "another flowspace3 daemon overwrote the shared key at {key_path}; restart the daemon \
             that owns {owner}"
        )
    } else {
        format!(
            "read the current key from {key_path} and send it as `Authorization: Bearer <key>`; \
             if the file is missing or stale, restart the fs3 daemon so it republishes the key"
        )
    };
    let envelope = Envelope::<serde_json::Value>::failed(
        "authenticate",
        Failure::new(
            &catalog::DAEMON_UNAUTHORIZED,
            "the request did not present this daemon boot's authentication key",
        )
        .with_fix(&fix)
        .with_detail("key_file", &key_path)
        .with_detail("key_newer_than_daemon", key_newer_than_daemon),
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an ephemeral listener");
        let listener = BoundListener::new(listener);
        let address = listener.local_addr().expect("the bound address");
        let auth = stage(directory.path())
            .expect("staging the isolated daemon key")
            .publish(&listener)
            .expect("publishing the isolated daemon key");
        let key = auth.key.to_string();
        let app = Router::new()
            .route(
                "/health",
                get(|| async { Json(serde_json::json!({ "status": "ok" })) }),
            )
            .layer(axum::middleware::from_fn_with_state(auth, require));
        tokio::spawn(async move {
            axum::serve(listener.into_inner(), app)
                .await
                .expect("serves")
        });
        (format!("http://{address}"), key, directory)
    }

    #[tokio::test]
    async fn staging_is_invisible_and_publish_replaces_with_mode_0600() {
        let directory = tempfile::tempdir().expect("an isolated config directory");
        let key_path = fs3_core::daemon_key_path(directory.path());
        std::fs::write(&key_path, "winner-key").expect("writing the winner's key");

        let staged = stage(directory.path()).expect("staging the replacement key");
        assert_eq!(
            std::fs::read_to_string(&key_path).expect("reading the winner's key"),
            "winner-key",
            "staging before a bind must not disturb the serving daemon"
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a bound listener proves publication is safe");
        let listener = BoundListener::new(listener);
        let auth = staged
            .publish(&listener)
            .expect("publishing after the bind");
        let published = std::fs::read_to_string(auth.key_path()).expect("reading the key");
        assert_eq!(published.len(), 64, "256 bits encoded as lowercase hex");
        assert!(published.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(published, "winner-key");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(auth.key_path())
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
        let failure = envelope.error.as_ref().expect("a failure envelope");
        assert_eq!(failure.code, "FS3-E-DAEMON-UNAUTHORIZED");
        assert_eq!(
            failure.details.get("key_newer_than_daemon"),
            Some(&serde_json::json!(false)),
            "the key published by this boot must not look newer than itself"
        );
        assert!(!failure.fix.contains("overwrote the shared key"));
        assert!(envelope.next_action.as_deref().is_some_and(|next| {
            next.contains(
                &fs3_core::daemon_key_path(directory.path())
                    .display()
                    .to_string(),
            )
        }));
    }

    #[tokio::test]
    async fn rewritten_key_401_names_the_overwrite_and_listener_owner() {
        let (base, original_key, directory) = server().await;
        let key_path = fs3_core::daemon_key_path(directory.path());
        let original_mtime = std::fs::metadata(&key_path)
            .expect("published key metadata")
            .modified()
            .expect("published key mtime");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            std::fs::write(&key_path, "replacement-key").expect("simulating the shared overwrite");
            let replacement_mtime = std::fs::metadata(&key_path)
                .expect("replacement key metadata")
                .modified()
                .expect("replacement key mtime");
            if replacement_mtime > original_mtime {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the fixture filesystem never advanced daemon.key's mtime"
            );
        }
        assert_ne!(
            std::fs::read_to_string(&key_path).expect("reading replacement key"),
            original_key
        );

        let response = reqwest::Client::new()
            .get(format!("{base}/health"))
            .bearer_auth("replacement-key")
            .send()
            .await
            .expect("the original daemon answers");
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        let envelope: Envelope = response.json().await.expect("an envelope");
        let failure = envelope.error.expect("a failure envelope");
        assert_eq!(
            failure.details.get("key_newer_than_daemon"),
            Some(&serde_json::json!(true))
        );
        assert!(
            failure
                .fix
                .contains("another flowspace3 daemon overwrote the shared key")
        );
        let port = base.rsplit(':').next().expect("base URL has a port");
        assert!(
            failure
                .fix
                .contains(&format!("restart the daemon that owns :{port}")),
            "fix did not name the listener owner: {}",
            failure.fix
        );
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
