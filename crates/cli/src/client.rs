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

use std::time::Duration;

use anyhow::{Context, Result};
use fs3_core::catalog;
use fs3_core::envelope::{Envelope, Failure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    http: reqwest::Client,
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

    /// Build a client for `base_url`.
    ///
    /// # Errors
    /// When the HTTP client cannot be constructed.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Self::SCAN_TIMEOUT)
                .build()
                .context("building the HTTP client")?,
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
        let response = self
            .http
            .get(&url)
            .timeout(Self::TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "fs3 daemon is not reachable at {}: {error}\n{}",
                    self.base_url,
                    crate::DOCTOR_HINT
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
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

    /// Read one address in full.
    pub async fn get(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("get", "/get", query).await
    }

    /// Browse indexed structure.
    pub async fn tree(&self, query: &[(String, String)]) -> Envelope {
        self.get_json("tree", "/tree", query).await
    }

    async fn get_json(&self, command: &str, path: &str, query: &[(String, String)]) -> Envelope {
        let url = format!("{}{path}", self.base_url);
        let response = self.http.get(&url).query(query).send().await;
        self.envelope(command, response).await
    }

    async fn post(&self, command: &str, path: &str, body: &Value) -> Envelope {
        let url = format!("{}{path}", self.base_url);
        let response = self.http.post(&url).json(body).send().await;
        self.envelope(command, response).await
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
