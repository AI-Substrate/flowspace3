//! The HTTP client. One method per daemon verb, all of them envelope-shaped.
//!
//! The CLI never interprets a payload: it asks, and it prints what came back.
//! That is what makes `flowspace3` a thin client in the sense PRD req 33 means
//! — a newer daemon returning a richer `data` needs no CLI change, because the
//! CLI's job ends at the envelope.
//!
//! # Unreachable is a verb-shaped answer, not an exception
//!
//! A daemon that does not answer is turned into an ERROR ENVELOPE here, with
//! `FS3-E-DAEMON-UNAVAILABLE` and the doctor pointer. The alternative — a raw
//! transport error — would be the one failure in fs3 that does not carry a
//! code, which is exactly the hole workshop 004's registry exists to close. A
//! script parsing `flowspace3` output should never need a second shape for
//! "the daemon is down".

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use fs3_core::catalog;
use fs3_core::envelope::{Envelope, Failure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The only way [`DaemonClient`] can build an HTTP request.
///
/// Keeping the raw reqwest client behind this private boundary makes
/// authentication a property of request construction, not a convention each
/// verb has to remember. The key is still read for every request so daemon
/// restarts and key rotation do not require rebuilding the CLI client.
mod authenticated {
    use std::path::PathBuf;
    use std::time::Duration;

    use anyhow::{Context, Result};
    use fs3_core::catalog;
    use fs3_core::envelope::Failure;
    use reqwest::{Method, RequestBuilder};

    #[derive(Debug, Clone)]
    pub(super) struct Client {
        inner: reqwest::Client,
        key_path: PathBuf,
    }

    impl Client {
        pub(super) fn new(key_path: PathBuf, timeout: Duration) -> Result<Self> {
            Ok(Self {
                inner: reqwest::Client::builder()
                    .timeout(timeout)
                    .build()
                    .context("building the HTTP client")?,
                key_path,
            })
        }

        pub(super) fn request(
            &self,
            method: Method,
            url: &str,
        ) -> std::result::Result<RequestBuilder, Failure> {
            let key = std::fs::read_to_string(&self.key_path).map_err(|error| {
                self.credential_failure(format!(
                    "cannot read the daemon authentication key at {}: {error}",
                    self.key_path.display()
                ))
            })?;
            let key = key.trim();
            if key.is_empty() {
                return Err(self.credential_failure(format!(
                    "the daemon authentication key at {} is empty",
                    self.key_path.display()
                )));
            }
            Ok(self.inner.request(method, url).bearer_auth(key))
        }

        fn credential_failure(&self, message: String) -> Failure {
            Failure::new(&catalog::DAEMON_UNAUTHORIZED, message)
                .with_fix(format!(
                    "restart the fs3 daemon so it publishes a current key at {}, then retry",
                    self.key_path.display()
                ))
                .with_detail("key_file", self.key_path.display().to_string())
        }
    }
}

/// The daemon's health response. Extra fields are tolerated so a newer daemon
/// does not break an older CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthReport {
    /// `"ok"` when the daemon is serving.
    pub status: String,
    /// The daemon's version.
    #[serde(default)]
    pub version: String,
    /// Which embedder arm the daemon wired.
    #[serde(default)]
    pub embedder: String,
    /// Which summarizer arm the daemon wired.
    #[serde(default)]
    pub summarizer: String,
}

impl HealthReport {
    /// Whether the daemon reported itself healthy.
    pub fn is_healthy(&self) -> bool {
        self.status == "ok"
    }
}

/// A client bound to one daemon URL.
#[derive(Debug, Clone)]
pub struct DaemonClient {
    http: authenticated::Client,
    base_url: String,
}

impl DaemonClient {
    /// How long to wait before declaring the daemon unreachable. Short: the CLI
    /// fails fast and points at doctor rather than hanging (PRD req 37).
    pub const TIMEOUT: Duration = Duration::from_secs(3);

    /// How long a SCAN may take before the CLI gives up.
    ///
    /// Longer than [`Self::TIMEOUT`] because `add` walks and hashes a whole
    /// repository before it answers, and three seconds is a normal walk rather
    /// than a hung daemon. The queue drains asynchronously, so this covers the
    /// walk only.
    pub const SCAN_TIMEOUT: Duration = Duration::from_secs(300);

    /// How long an agentic question may keep making model and tool round trips.
    ///
    /// This overrides the client-wide scan ceiling: a healthy agent loop can
    /// legitimately run longer than a repository walk, and must not look like
    /// an unreachable daemon while it is still producing an answer.
    pub const ASK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

    /// Build a client for `base_url`, reading this installation's daemon key
    /// from `config_dir` immediately before every request.
    ///
    /// # Errors
    /// When the HTTP client cannot be constructed.
    pub fn new(base_url: impl Into<String>, config_dir: &Path) -> Result<Self> {
        Ok(Self {
            http: authenticated::Client::new(
                fs3_core::daemon_key_path(config_dir),
                Self::SCAN_TIMEOUT,
            )?,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    /// The URL this client talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Ask the daemon how it is.
    ///
    /// # Errors
    /// An unreachable daemon fails fast, and the message names
    /// [`crate::DOCTOR_HINT`] — the CLI never starts infrastructure itself.
    pub async fn health(&self) -> Result<HealthReport> {
        let url = format!("{}/health", self.base_url);
        let request = self
            .http
            .request(reqwest::Method::GET, &url)
            .map(|request| request.timeout(Self::TIMEOUT))
            .map_err(|failure| anyhow::anyhow!(failure.render()))?;
        let response = request.send().await.map_err(|error| {
            anyhow::anyhow!(
                "fs3 daemon is not reachable at {}: {error}\n{}",
                self.base_url,
                crate::DOCTOR_HINT
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(envelope) = serde_json::from_str::<Envelope>(&body)
                && let Some(failure) = envelope.error
            {
                return Err(anyhow::anyhow!(failure.render()));
            }
            return Err(anyhow::anyhow!(
                "fs3 daemon at {} answered {status}: {}\n{}",
                self.base_url,
                body.trim(),
                crate::DOCTOR_HINT
            ));
        }

        response
            .json::<HealthReport>()
            .await
            .with_context(|| format!("{url} did not return a health report"))
    }

    /// Register a root.
    pub async fn add(&self, path: &str) -> Envelope {
        self.post("add", "/roots", &serde_json::json!({ "path": path }))
            .await
    }

    /// Re-scan a registered root.
    pub async fn scan(&self, path: &str) -> Envelope {
        self.post("scan", "/scan", &serde_json::json!({ "path": path }))
            .await
    }

    /// Unregister a root and kill its queued scans (PRD req 57).
    pub async fn remove(&self, path: &str) -> Envelope {
        self.post("remove", "/remove", &serde_json::json!({ "path": path }))
            .await
    }

    /// Reclaim rows nothing references, now.
    ///
    /// The same engine the daemon runs on its own cadence — this is the
    /// force-it-now entry point, like `doctor upgrade` beside auto-update.
    pub async fn gc(&self) -> Envelope {
        self.post("gc", "/gc", &serde_json::json!({})).await
    }

    /// Store a conversation header and a batch of turns.
    ///
    /// Append-friendly: the daemon is idempotent on `(conversation_id,
    /// turn_no)`, so posting a batch that overlaps what is already stored is
    /// safe and free rather than a duplicate.
    pub async fn conversation_import(&self, body: &Value) -> Envelope {
        self.post("conversation import", "/conversations", body)
            .await
    }

    /// Pull a conversation out of a native agent session store.
    pub async fn conversation_ingest(&self, body: &Value) -> Envelope {
        self.post("conversation ingest", "/conversations/ingest", body)
            .await
    }

    /// List indexed conversations.
    pub async fn conversation_list(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("conversation list", "/conversations", query)
            .await
    }

    /// Forget one conversation and its turns.
    pub async fn conversation_remove(&self, guid: &str) -> Envelope {
        self.post(
            "conversation remove",
            "/conversations/remove",
            &serde_json::json!({ "guid": guid }),
        )
        .await
    }

    /// Read roots and queue depth.
    pub async fn status(&self) -> Envelope {
        self.get_json("status", "/status", &[]).await
    }

    /// Ask a question.
    pub async fn search(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("search", "/search", query).await
    }

    /// Run one agentic question with its own long-lived request budget.
    pub async fn ask(&self, params: &[(String, String)]) -> Envelope {
        let url = format!("{}/ask", self.base_url);
        let body = Value::Object(
            params
                .iter()
                .map(|(name, value)| (name.clone(), Value::String(value.clone())))
                .collect(),
        );
        // The authenticated request boundary owns the key; this verb only
        // supplies its longer timeout and JSON body.
        self.send(
            "ask",
            self.http
                .request(reqwest::Method::POST, &url)
                .map(|request| request.timeout(Self::ASK_TIMEOUT).json(&body)),
        )
        .await
    }

    /// Read deterministic-document rows that reference one source path.
    pub async fn refs(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("refs", "/refs", query).await
    }

    /// Read one address in full.
    pub async fn get(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("get", "/get", query).await
    }

    /// Browse indexed structure.
    pub async fn tree(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("tree", "/tree", query).await
    }

    /// Open the daemon's event stream, authenticated.
    ///
    /// The one door to `GET /events` for every consumer that needs it — the
    /// `add` progress meter, `status --watch`, and the TUI's activity pane.
    /// It exists because the key handling above is private and must stay that
    /// way: three units reaching for `reqwest` directly would be three places
    /// that can forget the `Authorization` header, and the failure mode is a
    /// silent 401 that looks exactly like "the daemon has nothing to say"
    /// (found by u-r, 2026-08-28, before it shipped).
    ///
    /// Returns the raw response rather than parsed events on purpose: this is a
    /// STREAM, and the caller reads it line by line for as long as it wants to.
    /// The wire is `fs3_core::events` — a `Hello` line, then one `Event` per
    /// line — and `docs/services/event-stream.md` is its contract.
    ///
    /// No timeout is applied beyond the connect: a healthy stream is idle most
    /// of the time, and the heartbeat is how a consumer tells idle from dead.
    ///
    /// # Errors
    /// A missing or unreadable key, or a daemon that does not answer. Both come
    /// back as a [`Failure`] carrying its own catalog code and fix, so a caller
    /// renders them like any other failure instead of inventing prose.
    pub async fn events(
        &self,
        heartbeat_ms: Option<u64>,
    ) -> std::result::Result<reqwest::Response, Failure> {
        let url = format!("{}/events", self.base_url);
        let mut request = self.http.request(reqwest::Method::GET, &url)?;
        if let Some(interval) = heartbeat_ms {
            request = request.query(&[("heartbeat_ms", interval.to_string())]);
        }

        let response = request.send().await.map_err(|error| {
            Failure::new(
                &catalog::DAEMON_UNAVAILABLE,
                format!("cannot open the event stream at {url}: {error}"),
            )
            .with_detail("daemon_url", self.base_url.clone())
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(envelope) = serde_json::from_str::<Envelope>(&body)
                && let Some(failure) = envelope.error
            {
                return Err(failure);
            }
            return Err(Failure::new(
                &catalog::DAEMON_UNAVAILABLE,
                format!("the event stream at {url} answered {status}"),
            )
            .with_detail("status", status.as_u16()));
        }

        Ok(response)
    }

    async fn get_json(&self, command: &str, path: &str, query: &[(String, String)]) -> Envelope {
        let url = format!("{}{path}", self.base_url);
        self.send(
            command,
            self.http
                .request(reqwest::Method::GET, &url)
                .map(|request| request.query(query)),
        )
        .await
    }

    async fn post(&self, command: &str, path: &str, body: &Value) -> Envelope {
        let url = format!("{}{path}", self.base_url);
        self.send(
            command,
            self.http
                .request(reqwest::Method::POST, &url)
                .map(|request| request.json(body)),
        )
        .await
    }

    async fn send(
        &self,
        command: &str,
        request: std::result::Result<reqwest::RequestBuilder, Failure>,
    ) -> Envelope {
        let request = match request {
            Ok(request) => request,
            Err(failure) => {
                let next = failure.fix.clone();
                return Envelope::failed(command, failure).with_next_action(next);
            }
        };
        self.envelope(command, request.send().await).await
    }

    /// Turn a transport outcome into an envelope, whatever happened.
    ///
    /// Three cases collapse into one shape here: the daemon answered an
    /// envelope (return it, error or not); the daemon answered something else
    /// (a proxy, a wrong port, a panic page); or it did not answer at all. Only
    /// the first is the daemon's own words — the other two are turned into
    /// codes rather than passed through as prose, because a caller that has to
    /// tell "no route to host" from `{"ok":false}` by parsing is a caller that
    /// will get it wrong.
    async fn envelope(
        &self,
        command: &str,
        response: reqwest::Result<reqwest::Response>,
    ) -> Envelope {
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                return Envelope::failed(
                    command,
                    Failure::new(
                        &catalog::DAEMON_UNAVAILABLE,
                        format!("cannot reach the fs3 daemon at {}: {error}", self.base_url),
                    )
                    .with_detail("daemon_url", self.base_url.clone()),
                );
            }
        };

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        match serde_json::from_str::<Envelope>(&body) {
            Ok(envelope) => envelope,
            Err(_) => Envelope::failed(
                command,
                Failure::new(
                    &catalog::DAEMON_UNAVAILABLE,
                    format!(
                        "the daemon at {} answered {status} with something that is not an fs3 \
                         envelope",
                        self.base_url
                    ),
                )
                .with_fix(
                    "check that daemon.url points at fs3 and not another service on that port \
                     (`flowspace3 config show`), then `flowspace3 doctor`",
                )
                .with_detail("status", status.as_u16())
                .with_detail("body", body.chars().take(200).collect::<String>()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::authenticated;

    const TEST_KEY: &str = "client-auth-contract-key";

    /// Authentication belongs to the request-construction boundary, not to a
    /// hand-maintained list of verbs. `DaemonClient` cannot access the raw
    /// reqwest client, so existing and future verbs all inherit this contract.
    #[tokio::test]
    async fn every_daemon_verb_inherits_authorization_from_the_request_boundary() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
        let address = listener.local_addr().expect("the socket is bound");
        let probe = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the client should connect");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .expect("setting the probe read timeout");

            const MAX_HEADER_BYTES: usize = 16 * 1024;
            let mut request = Vec::with_capacity(2048);
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                assert!(
                    request.len() < MAX_HEADER_BYTES,
                    "the client sent more than {MAX_HEADER_BYTES} bytes without ending its HTTP headers"
                );
                let mut chunk = [0_u8; 1024];
                let remaining = MAX_HEADER_BYTES - request.len();
                let read_limit = remaining.min(chunk.len());
                let read = stream
                    .read(&mut chunk[..read_limit])
                    .expect("the client should finish its HTTP headers within three seconds");
                assert_ne!(
                    read, 0,
                    "the client closed the connection before ending its HTTP headers"
                );
                request.extend_from_slice(&chunk[..read]);
            }
            let request = String::from_utf8(request).expect("HTTP headers are valid UTF-8");
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("the probe should answer");
            request
        });

        let config = tempfile::tempdir().expect("a temp config directory");
        let key_path = fs3_core::daemon_key_path(config.path());
        std::fs::write(&key_path, TEST_KEY).expect("writing the isolated daemon key");
        let client = authenticated::Client::new(key_path, std::time::Duration::from_secs(3))
            .expect("an HTTP client");
        client
            .request(reqwest::Method::POST, &format!("http://{address}/contract"))
            .expect("an authenticated request")
            .send()
            .await
            .expect("the probe should answer");

        let request = probe.join().expect("the probe thread should finish");
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {TEST_KEY}"))),
            "the authenticated request boundary dropped the daemon key; every DaemonClient verb \
             uses this boundary, so all verbs would fail with FS3-E-DAEMON-UNAUTHORIZED. \
             Recorded request:\n{request}"
        );
    }

    /// The network probe proves the boundary adds the key; this structural
    /// half proves no verb quietly routes around that boundary. It discovers
    /// functions from the source, so adding a verb cannot require updating a
    /// hand-maintained test list.
    #[test]
    fn a_verb_that_bypasses_the_authenticated_boundary_is_named() {
        let source = include_str!("client.rs");
        let (prefix, implementation) = source
            .split_once("impl DaemonClient {")
            .expect("the DaemonClient implementation");
        let implementation = implementation
            .split_once("#[cfg(test)]")
            .expect("the client tests follow the implementation")
            .0;
        let implementation_line = prefix.lines().count() + 1;
        let mut function = "DaemonClient implementation";
        let mut bypasses = Vec::new();

        for (offset, line) in implementation.lines().enumerate() {
            let signature = line.trim_start();
            if let Some(rest) = signature
                .strip_prefix("pub async fn ")
                .or_else(|| signature.strip_prefix("async fn "))
            {
                function = rest.split('(').next().unwrap_or(function);
            }
            let raw_reqwest = line.contains("reqwest::Client") || line.contains("reqwest::get(");
            let skips_send = line.contains("self.envelope(") && function != "send";
            if raw_reqwest || skips_send || line.contains(".inner") {
                let bypass = format!("{function} (client.rs:{})", implementation_line + offset);
                if bypasses.last() != Some(&bypass) {
                    bypasses.push(bypass);
                }
            }
        }

        assert!(
            bypasses.is_empty(),
            "daemon verb(s) bypassed the authenticated request boundary: {}. Every request must \
             carry the current bearer key or the daemon refuses it with \
             FS3-E-DAEMON-UNAUTHORIZED",
            bypasses.join(", ")
        );
    }
}
