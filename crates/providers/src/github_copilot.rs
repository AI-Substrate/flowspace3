//! GitHub Copilot authentication and OpenAI-shaped model adapters.
//!
//! The wire contract was measured against OMP 17.4.0 on 2026-08-29. A GitHub
//! OAuth token is sent directly as a bearer to the Copilot API endpoint returned
//! by `GET https://api.github.com/copilot_internal/user`. The older guessed
//! `/copilot_internal/v2/token` exchange returns 403 and is not part of this
//! adapter. Copilot accepts the OpenAI chat and embeddings shapes directly.
//!
//! Credential precedence is explicit: `COPILOT_GITHUB_TOKEN` (including a value
//! loaded from flowspace3's `secrets.env`), GitHub Copilot host files, then OMP's
//! SQLite OAuth row. OMP's database is opened immutable/read-only and is never
//! written, migrated, or locked.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use fs3_core::{
    ChatMessage, ChatProvider, ChatTurn, Element, Embedder, Error, Result, Summarizer, Summary,
    ToolSchema,
};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};

use crate::{
    OpenAiCompatChatClient, OpenAiCompatConfig, OpenAiCompatEmbedder, OpenAiCompatSummarizer,
};

pub const TOKEN_ENV: &str = "COPILOT_GITHUB_TOKEN";
pub const DEFAULT_BASE_URL: &str = "https://api.githubcopilot.com";
pub const COPILOT_USER_AGENT: &str = "opencode/1.3.15";
pub const COPILOT_API_VERSION: &str = "2026-06-01";
const GITHUB_API: &str = "https://api.github.com";
const GITHUB_CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";

#[derive(Clone)]
pub struct GitHubCopilotCredential {
    token: String,
    api_endpoint: String,
    expires_at_ms: Option<u64>,
    source: CredentialSource,
}

impl std::fmt::Debug for GitHubCopilotCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubCopilotCredential")
            .field("token", &"<redacted>")
            .field("api_endpoint", &self.api_endpoint)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    Environment,
    GitHubCopilotFile,
    Omp,
    DeviceFlow,
}

impl CredentialSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Environment => "environment/secrets.env",
            Self::GitHubCopilotFile => "GitHub Copilot credential file",
            Self::Omp => "OMP credential store (read-only)",
            Self::DeviceFlow => "flowspace3 login",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginState {
    LoggedIn,
    Expired,
    NotLoggedIn,
}

impl GitHubCopilotCredential {
    pub fn discover() -> Result<Self> {
        let raw = std::env::var(TOKEN_ENV).ok();
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            Error::Provider(format!(
                "github-copilot: no credential found and HOME is unset; run `flowspace3 login github-copilot` or set {TOKEN_ENV}"
            ))
        })?;
        discover_from(raw.as_deref(), &home)
    }

    /// Build from bearer bytes and an already discovered Copilot API endpoint.
    /// The token remains redacted from `Debug` and every error surface.
    pub fn from_token(token: impl Into<String>, api_endpoint: impl Into<String>) -> Result<Self> {
        let token = token.into();
        let api_endpoint = api_endpoint.into();
        if token.trim().is_empty() {
            return Err(Error::Provider("github-copilot: token is empty".into()));
        }
        if !api_endpoint.starts_with("http://") && !api_endpoint.starts_with("https://") {
            return Err(Error::Provider(
                "github-copilot: API endpoint must be an http(s) URL".into(),
            ));
        }
        Ok(Self {
            token,
            api_endpoint: api_endpoint.trim_end_matches('/').to_string(),
            expires_at_ms: None,
            source: CredentialSource::Environment,
        })
    }

    pub fn login_state() -> LoginState {
        match Self::discover() {
            Ok(credential) if credential.is_expired() => LoginState::Expired,
            Ok(_) => LoginState::LoggedIn,
            Err(_) => LoginState::NotLoggedIn,
        }
    }

    pub fn source(&self) -> CredentialSource {
        self.source
    }

    pub fn api_endpoint(&self) -> &str {
        &self.api_endpoint
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at_ms
            .is_some_and(|expires| expires <= now_ms())
    }

    pub fn secret_env_value(&self) -> Result<String> {
        serde_json::to_string(&StoredCredential {
            token: &self.token,
            api_endpoint: &self.api_endpoint,
            expires_at_ms: self.expires_at_ms,
        })
        .map_err(|error| {
            Error::Provider(format!("github-copilot: cannot encode credential: {error}"))
        })
    }

    fn openai_config(
        &self,
        model: &str,
        dimensions: Option<usize>,
        max_tokens: Option<usize>,
    ) -> OpenAiCompatConfig {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(COPILOT_USER_AGENT));
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static(COPILOT_API_VERSION),
        );
        headers.insert(
            "openai-intent",
            HeaderValue::from_static("conversation-edits"),
        );
        headers.insert("x-initiator", HeaderValue::from_static("user"));

        let mut config = OpenAiCompatConfig::new(&self.api_endpoint)
            .with_model(model)
            .with_bearer_token(self.token.clone())
            .with_default_headers(headers)
            .with_max_completion_tokens();
        if let Some(dimensions) = dimensions {
            config = config.with_dimensions(dimensions);
        }
        if let Some(max_tokens) = max_tokens {
            config = config.with_max_tokens(max_tokens);
        }
        config
    }
}

#[derive(Serialize)]
struct StoredCredential<'a> {
    token: &'a str,
    #[serde(rename = "apiEndpoint")]
    api_endpoint: &'a str,
    #[serde(rename = "expires", skip_serializing_if = "Option::is_none")]
    expires_at_ms: Option<u64>,
}

#[derive(Deserialize)]
struct CredentialValue {
    token: Option<String>,
    access: Option<String>,
    oauth_token: Option<String>,
    #[serde(rename = "apiEndpoint")]
    api_endpoint: Option<String>,
    #[serde(rename = "expires")]
    expires_at_ms: Option<u64>,
}

fn discover_from(raw_env: Option<&str>, home: &Path) -> Result<GitHubCopilotCredential> {
    if let Some(raw) = raw_env.filter(|raw| !raw.trim().is_empty()) {
        return parse_credential(raw, CredentialSource::Environment);
    }

    for name in ["hosts.json", "apps.json"] {
        let path = home.join(".config/github-copilot").join(name);
        if let Some(credential) = credential_from_json_file(&path)? {
            return Ok(credential);
        }
    }

    if let Some(credential) = credential_from_omp(home)? {
        return Ok(credential);
    }

    Err(Error::Provider(format!(
        "github-copilot: not logged in; run `flowspace3 login github-copilot` (or set {TOKEN_ENV})"
    )))
}

fn parse_credential(raw: &str, source: CredentialSource) -> Result<GitHubCopilotCredential> {
    let parsed = serde_json::from_str::<CredentialValue>(raw).ok();
    let token = parsed
        .as_ref()
        .and_then(|value| {
            value
                .token
                .clone()
                .or_else(|| value.access.clone())
                .or_else(|| value.oauth_token.clone())
        })
        .unwrap_or_else(|| raw.trim().to_string());
    if token.is_empty() {
        return Err(Error::Provider(format!(
            "github-copilot: {} contains an empty token",
            source.label()
        )));
    }
    let api_endpoint = parsed
        .as_ref()
        .and_then(|value| value.api_endpoint.clone())
        .filter(|value| value.starts_with("https://"))
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Ok(GitHubCopilotCredential {
        token,
        api_endpoint,
        expires_at_ms: parsed.and_then(|value| value.expires_at_ms),
        source,
    })
}

fn credential_from_json_file(path: &Path) -> Result<Option<GitHubCopilotCredential>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::Provider(format!(
                "github-copilot: cannot read {}: {error}",
                path.display()
            )));
        }
    };
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        Error::Provider(format!(
            "github-copilot: cannot parse {}: {error}",
            path.display()
        ))
    })?;
    let candidate = value
        .as_object()
        .and_then(|root| root.get("github.com").or_else(|| root.values().next()))
        .unwrap_or(&value);
    let Some(object) = candidate.as_object() else {
        return Ok(None);
    };
    let token = ["oauth_token", "token", "access_token"]
        .iter()
        .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str));
    let Some(token) = token else { return Ok(None) };
    parse_credential(token, CredentialSource::GitHubCopilotFile).map(Some)
}

fn credential_from_omp(home: &Path) -> Result<Option<GitHubCopilotCredential>> {
    let path = home.join(".omp/agent/agent.db");
    if !path.exists() {
        return Ok(None);
    }
    let uri = format!("file:{}?mode=ro&immutable=1", path.display());
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = rusqlite::Connection::open_with_flags(uri, flags).map_err(|error| {
        Error::Provider(format!(
            "github-copilot: cannot read OMP credential store {}: {error}",
            path.display()
        ))
    })?;
    let raw = connection
        .query_row(
            "SELECT data FROM auth_credentials WHERE provider = 'github-copilot' AND credential_type = 'oauth' AND disabled_cause IS NULL ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| Error::Provider(format!("github-copilot: cannot query OMP credential store {} read-only: {error}", path.display())))?;
    raw.map(|raw| parse_credential(&raw, CredentialSource::Omp))
        .transpose()
}

use rusqlite::OptionalExtension as _;

#[derive(Clone, Debug)]
pub struct GitHubCopilotConfig {
    pub model: String,
    pub dimensions: Option<usize>,
    pub max_tokens: Option<usize>,
    credential: GitHubCopilotCredential,
}

impl GitHubCopilotConfig {
    pub fn discover(
        model: impl Into<String>,
        dimensions: Option<usize>,
        max_tokens: Option<usize>,
    ) -> Result<Self> {
        let credential = GitHubCopilotCredential::discover()?;
        if credential.is_expired() {
            return Err(Error::Provider("github-copilot: stored credential is expired; run `flowspace3 login github-copilot`".into()));
        }
        Ok(Self {
            model: model.into(),
            dimensions,
            max_tokens,
            credential,
        })
    }

    pub fn from_credential(
        model: impl Into<String>,
        dimensions: Option<usize>,
        max_tokens: Option<usize>,
        credential: GitHubCopilotCredential,
    ) -> Self {
        Self {
            model: model.into(),
            dimensions,
            max_tokens,
            credential,
        }
    }

    fn wire(&self) -> OpenAiCompatConfig {
        self.credential
            .openai_config(&self.model, self.dimensions, self.max_tokens)
    }
}

#[derive(Debug)]
pub struct GitHubCopilotEmbedder(OpenAiCompatEmbedder);
impl GitHubCopilotEmbedder {
    pub fn new(config: GitHubCopilotConfig) -> Self {
        Self(OpenAiCompatEmbedder::new(config.wire()))
    }
}
#[async_trait]
impl Embedder for GitHubCopilotEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.0.embed(texts).await
    }
    fn key(&self) -> String {
        self.0.key()
    }
    fn concurrency_ceiling(&self) -> usize {
        self.0.concurrency_ceiling()
    }
    fn max_input_tokens(&self) -> usize {
        self.0.max_input_tokens()
    }
}

#[derive(Debug)]
pub struct GitHubCopilotSummarizer(OpenAiCompatSummarizer);
impl GitHubCopilotSummarizer {
    pub fn new(config: GitHubCopilotConfig) -> Self {
        Self(OpenAiCompatSummarizer::configured(config.wire()))
    }
}
#[async_trait]
impl Summarizer for GitHubCopilotSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        self.0.summarize(element).await
    }
    fn key(&self) -> String {
        self.0.key()
    }
    fn concurrency_ceiling(&self) -> usize {
        self.0.concurrency_ceiling()
    }
    fn max_input_tokens(&self) -> usize {
        self.0.max_input_tokens()
    }
}

#[derive(Debug)]
pub struct GitHubCopilotChatClient(OpenAiCompatChatClient);
impl GitHubCopilotChatClient {
    pub fn new(config: GitHubCopilotConfig) -> Self {
        Self(OpenAiCompatChatClient::new(config.wire()))
    }
}
#[async_trait]
impl ChatProvider for GitHubCopilotChatClient {
    async fn turn(&self, messages: &[ChatMessage], tools: &[ToolSchema]) -> Result<ChatTurn> {
        self.0.turn(messages, tools).await
    }
    fn key(&self) -> String {
        self.0.key()
    }
    fn max_input_tokens(&self) -> usize {
        self.0.max_input_tokens()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GitHubCopilotModel {
    pub id: String,
    #[serde(default, skip_serializing)]
    supported_endpoints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GitHubCopilotModelList {
    pub models: Vec<GitHubCopilotModel>,
    pub omitted_non_chat: usize,
    pub filter: &'static str,
}

pub async fn list_models(credential: &GitHubCopilotCredential) -> Result<GitHubCopilotModelList> {
    #[derive(Deserialize)]
    struct Response {
        data: Vec<GitHubCopilotModel>,
    }
    let url = format!("{}/models", credential.api_endpoint.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&credential.token)
        .header(USER_AGENT, COPILOT_USER_AGENT)
        .header("x-github-api-version", COPILOT_API_VERSION)
        .send()
        .await
        .map_err(|error| Error::Provider(format!("github-copilot: GET {url}: {error}")))?;
    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "github-copilot: GET {url}: {}; run `flowspace3 login github-copilot` if the credential was revoked",
            response.status()
        )));
    }
    let mut models = response
        .json::<Response>()
        .await
        .map_err(|error| {
            Error::Provider(format!(
                "github-copilot: GET {url}: unreadable model list: {error}"
            ))
        })?
        .data;
    let total = models.len();
    models.retain(|model| {
        model
            .supported_endpoints
            .iter()
            .any(|endpoint| endpoint == "/chat/completions")
    });
    let omitted_non_chat = total - models.len();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(GitHubCopilotModelList {
        models,
        omitted_non_chat,
        filter: "/chat/completions",
    })
}

#[derive(Clone)]
pub struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    expires_in: u64,
}
impl std::fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCode")
            .field("device_code", &"<redacted>")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("interval", &self.interval)
            .field("expires_in", &self.expires_in)
            .finish()
    }
}
impl DeviceCode {
    pub fn user_code(&self) -> &str {
        &self.user_code
    }
    pub fn verification_uri(&self) -> &str {
        &self.verification_uri
    }
}

pub async fn start_device_login() -> Result<DeviceCode> {
    #[derive(Deserialize)]
    struct Response {
        device_code: String,
        user_code: String,
        verification_uri: String,
        interval: u64,
        expires_in: u64,
    }
    let response = reqwest::Client::new()
        .post("https://github.com/login/device/code")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, COPILOT_USER_AGENT)
        .json(&serde_json::json!({"client_id": GITHUB_CLIENT_ID, "scope": "read:user"}))
        .send()
        .await
        .map_err(|error| {
            Error::Provider(format!("github-copilot: starting device login: {error}"))
        })?;
    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "github-copilot: starting device login: {}",
            response.status()
        )));
    }
    let response = response.json::<Response>().await.map_err(|error| {
        Error::Provider(format!(
            "github-copilot: invalid device-code response: {error}"
        ))
    })?;
    Ok(DeviceCode {
        device_code: response.device_code,
        user_code: response.user_code,
        verification_uri: response.verification_uri,
        interval: response.interval,
        expires_in: response.expires_in,
    })
}

pub async fn finish_device_login(device: DeviceCode) -> Result<GitHubCopilotCredential> {
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = Duration::from_secs(device.interval.max(1));
    while Instant::now() < deadline {
        tokio::time::sleep(interval).await;
        let value = client.post("https://github.com/login/oauth/access_token")
            .header(ACCEPT, "application/json").header(USER_AGENT, COPILOT_USER_AGENT)
            .json(&serde_json::json!({"client_id": GITHUB_CLIENT_ID, "device_code": device.device_code, "grant_type": "urn:ietf:params:oauth:grant-type:device_code"}))
            .send().await.map_err(|error| Error::Provider(format!("github-copilot: polling device login: {error}")))?
            .json::<serde_json::Value>().await.map_err(|error| Error::Provider(format!("github-copilot: invalid device-login response: {error}")))?;
        if let Some(token) = value
            .get("access_token")
            .and_then(serde_json::Value::as_str)
        {
            let endpoint = discover_endpoint(&client, token).await?;
            return Ok(GitHubCopilotCredential {
                token: token.to_string(),
                api_endpoint: endpoint,
                expires_at_ms: None,
                source: CredentialSource::DeviceFlow,
            });
        }
        match value.get("error").and_then(serde_json::Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += Duration::from_secs(5),
            Some(error) => {
                return Err(Error::Provider(format!(
                    "github-copilot: device login failed: {error}"
                )));
            }
            None => {
                return Err(Error::Provider(
                    "github-copilot: device login returned neither a token nor an error".into(),
                ));
            }
        }
    }
    Err(Error::Provider(
        "github-copilot: device login timed out; run `flowspace3 login github-copilot` again"
            .into(),
    ))
}

async fn discover_endpoint(client: &reqwest::Client, token: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct User {
        endpoints: Endpoints,
    }
    #[derive(Deserialize)]
    struct Endpoints {
        api: String,
    }
    let url = format!("{GITHUB_API}/copilot_internal/user");
    let response = client
        .get(&url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, COPILOT_USER_AGENT)
        .header("authorization", format!("token {token}"))
        .send()
        .await
        .map_err(|error| {
            Error::Provider(format!("github-copilot: discovering API endpoint: {error}"))
        })?;
    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "github-copilot: GitHub rejected the login while discovering the Copilot endpoint: {}",
            response.status()
        )));
    }
    let user = response.json::<User>().await.map_err(|error| {
        Error::Provider(format!(
            "github-copilot: invalid endpoint discovery response: {error}"
        ))
    })?;
    if !user.endpoints.api.starts_with("https://") {
        return Err(Error::Provider(
            "github-copilot: GitHub returned a non-HTTPS Copilot endpoint".into(),
        ));
    }
    Ok(user.endpoints.api.trim_end_matches('/').to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        std::env::temp_dir().join(format!(
            "fs3-copilot-credential-{}-{}",
            std::process::id(),
            now_ms()
        ))
    }

    #[test]
    fn environment_precedes_a_github_copilot_file() {
        let home = home();
        let dir = home.join(".config/github-copilot");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("hosts.json"),
            r#"{"github.com":{"oauth_token":"file-token"}}"#,
        )
        .unwrap();

        let credential = discover_from(
            Some(r#"{"token":"env-token","apiEndpoint":"https://copilot.example"}"#),
            &home,
        )
        .unwrap();
        assert_eq!(credential.source(), CredentialSource::Environment);
        assert_eq!(credential.api_endpoint(), "https://copilot.example");
        assert!(!format!("{credential:?}").contains("env-token"));
        std::fs::remove_dir_all(home).ok();
    }

    #[test]
    fn github_copilot_file_is_used_before_the_omp_store() {
        let home = home();
        let dir = home.join(".config/github-copilot");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("apps.json"),
            r#"{"github.com":{"oauth_token":"file-token"}}"#,
        )
        .unwrap();
        let credential = discover_from(None, &home).unwrap();
        assert_eq!(credential.source(), CredentialSource::GitHubCopilotFile);
        assert_eq!(credential.api_endpoint(), DEFAULT_BASE_URL);
        std::fs::remove_dir_all(home).ok();
    }

    #[test]
    fn stored_expiry_is_reported_without_printing_the_token() {
        let credential = parse_credential(
            r#"{"access":"never-print-this","expires":1,"apiEndpoint":"https://copilot.example"}"#,
            CredentialSource::Omp,
        )
        .unwrap();
        assert!(credential.is_expired());
        let debug = format!("{credential:?}");
        assert!(!debug.contains("never-print-this"));
        assert!(debug.contains("<redacted>"));
    }
}
