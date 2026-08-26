//! The HTTP client. One endpoint so far: `GET /health`.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::DOCTOR_HINT;

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

    /// Build a client for `base_url`.
    ///
    /// # Errors
    /// When the HTTP client cannot be constructed.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Self::TIMEOUT)
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
        let response = self.http.get(&url).send().await.map_err(|error| {
            anyhow!(
                "fs3 daemon is not reachable at {}: {error}\n{DOCTOR_HINT}",
                self.base_url
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "fs3 daemon at {} answered {status}: {}\n{DOCTOR_HINT}",
                self.base_url,
                body.trim()
            ));
        }

        response
            .json::<HealthReport>()
            .await
            .with_context(|| format!("{url} did not return a health report"))
    }
}
