//! `flowspace3 ask` — the daemon half of the agentic query verb.
//!
//! The loop itself is [`fs3_core::ask`], which is pure and knows nothing about
//! HTTP, Postgres or Azure. This module supplies the two things it cannot have:
//! a chat provider (from the composition root) and a [`ToolBox`] that actually
//! reads the index.
//!
//! ## The tools run IN PROCESS
//!
//! `search` and `get` here are direct calls to [`crate::search::search`] and
//! [`crate::read::get`] — the same functions the HTTP handlers call. The
//! prototype shelled out to the `flowspace3` binary for every tool call, which
//! paid a process spawn, a second Entra token acquisition and a JSON
//! round trip per step. Nothing about that was necessary once the loop lives
//! beside the query surface.
//!
//! ## Scope is stated, never assumed
//!
//! Search is scoped by the caller's working directory: standing inside a
//! repository silently narrows results to it. For a human that is a convenience
//! — you get local answers first. For an agent it is a trap, because a scoped
//! zero looks exactly like a global absence, and an agent told "no results"
//! will conclude the thing does not exist and say so with confidence. That is
//! the grounding bar failing through the back door.
//!
//! So every tool result here NAMES its scope, and an empty scoped result says
//! explicitly that other repositories were not searched and how to widen. Ruled
//! binding by o-prime on 2026-08-28 after the trap was hit while dogfooding.

use async_trait::async_trait;
use fs3_core::{
    Address, AgentAnswer, AgentBounds, ConversationId, Result as CoreResult, ToolBox, ToolOutcome,
    ToolSchema, agent::StopReason, catalog, envelope::Failure, views::read::GetPayload,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::scope::{Scope, ScopeSource};
use crate::search::SearchRequest;
use crate::wiring::AppState;

/// What a caller asks of `POST /ask`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct AskRequest {
    /// The question, in prose.
    pub question: String,
    /// The caller's working directory, which is what scoping reads. The daemon
    /// has one of its own and it is never the caller's, so it must be sent.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Restrict to one repository identity, or `all` to search every one.
    #[serde(default)]
    pub repo: Option<String>,
    /// Restrict code and document retrieval to paths matching this glob.
    #[serde(default)]
    pub path: Option<String>,
    /// Content source: `code`, `doc`, `conversation`, or `all` (the default).
    #[serde(default)]
    pub source: Option<String>,
    /// Pin every retrieval and citation to one indexed transcript.
    #[serde(default)]
    pub conversation: Option<String>,
}

/// One tool call, as it happened.
///
/// The trace is the verb's evidence: it is how a reader checks that the answer
/// came from the index rather than from the model's memory, and it is what the
/// evaluation suite asserts against. It is therefore part of the contract, not
/// debug output.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskTraceEntry {
    /// Which turn, counting from 1.
    pub iteration: u32,
    /// The tool the model asked for — recorded as asked, even when no such tool
    /// exists, because an invented tool name is a finding.
    pub tool: String,
    /// The arguments it sent, verbatim.
    pub arguments: String,
    /// Whether the call came back an error the model had to recover from. A run
    /// containing a recovered error is a healthy run, not a failed one.
    pub failed: bool,
    /// Whether the call yielded material from the index.
    ///
    /// Not the same as `!failed`: a search that ran correctly and matched
    /// nothing is `failed: false, evidence: false`. An evaluator wants this
    /// distinction — "the tools worked and found nothing" and "the tools
    /// broke" score differently.
    pub evidence: bool,
    /// Addresses whose summaries this search surfaced to the model.
    ///
    /// Empty for non-search calls, failed searches and searches with no hits.
    /// This is deliberately weaker provenance than [`AskReport::citations`]:
    /// surfaced here means offered as a summary, not read in full.
    pub search_hits: Vec<String>,
    /// Size of the result handed back, after truncation.
    pub result_chars: usize,
}

/// One transcript's measured corpus boundary.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskConversationCoverage {
    /// The canonical full guid selected from the caller's short/full/address form.
    pub guid: String,
    /// Always one: explicit so consumers do not mistake this for repo coverage.
    pub count: u8,
    /// Turns stored in that transcript when the request began.
    pub turns: i64,
}

/// One immutable path boundary and its measured file-backed corpus.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskPathCoverage {
    /// The caller's glob, verbatim.
    pub glob: String,
    /// Live elements reachable through matching paths and the selected source.
    pub elements: i64,
    /// Why conversations are absent when the caller selected every source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_exclusion: Option<String>,
}

/// The content corpus the loop was actually allowed to inspect.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskCorpusCoverage {
    /// Effective source axis after applying a transcript pin.
    pub source: String,
    /// Present only when retrieval was pinned to exactly one conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<AskConversationCoverage>,
    /// Present only when the caller supplied `--path`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<AskPathCoverage>,
}

/// The finite probe behind an answer, stated separately from its prose.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskCoverage {
    /// Model turns consumed by this run.
    pub iterations_used: u32,
    /// Maximum model turns the run was allowed to consume.
    pub iteration_limit: u32,
    /// The top-k requested by each valid search call, in call order.
    pub retrieval_top_k: Vec<i64>,
    /// The corpus boundary applied to every search and read.
    pub corpus: AskCorpusCoverage,
    /// Always false: bounded nearest-neighbour retrieval cannot prove completeness.
    pub exhaustive: bool,
}

/// Evidence retained when ask stops before synthesizing an answer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskPartialEvidence {
    /// Explicit classification: these facts support investigation, not an answer.
    pub label: String,
    /// Addresses the loop read in full before it stopped.
    pub citations: Vec<String>,
    /// One measured summary for every completed model iteration.
    pub findings: Vec<String>,
}

/// What `POST /ask` answers with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskReport {
    /// The question as asked, echoed so a stored report is self-describing.
    pub question: String,
    /// The grounded answer. `None` means the loop hit a bound before
    /// answering — the caller must say so rather than present something else
    /// as the answer.
    pub answer: Option<String>,
    /// Provider failure text for a run that stopped after gathering evidence.
    /// Never used as answer prose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    /// Every address the loop actually READ, in order, deduplicated.
    ///
    /// Deliberately what the loop read, not what the model claimed at the end:
    /// the model's own citation list is prose and can be wrong, while this is
    /// measured. The evaluator resolves each of these to score groundedness.
    pub citations: Vec<String>,
    /// Every tool call.
    pub trace: Vec<AskTraceEntry>,
    /// The measured bounds of this probe; never a claim of exhaustive coverage.
    pub coverage: AskCoverage,
    /// Turns taken.
    pub iterations: u32,
    /// Tokens spent, or `null` when the provider reported nothing.
    ///
    /// Null is not zero. A provider that reports no usage has said it does not
    /// know; publishing `0` would be a number nobody measured, and a budget
    /// assertion against it would pass while measuring nothing.
    pub tokens_used: Option<u64>,
    /// Why the run ended: `answered`, `max_iterations`, `token_budget`, or
    /// `provider_failure`.
    pub stopped: String,
    /// Which chat model answered.
    pub model: String,
    /// Whether the answer rests on evidence the loop actually read.
    ///
    /// `false` means the model answered without successfully reading anything.
    /// The loop pushes back once before permitting that, so a `false` here is a
    /// model that insisted. Carried as a FIELD rather than as prose in
    /// `next_action` because an evaluator cannot assert on a warning.
    pub grounded: bool,
}

impl AskReport {
    /// Preserve useful reads without presenting them as a synthesized answer.
    #[must_use]
    pub fn partial_evidence(&self) -> AskPartialEvidence {
        let findings = (1..=self.coverage.iterations_used)
            .map(|iteration| {
                let calls: Vec<_> = self
                    .trace
                    .iter()
                    .filter(|entry| entry.iteration == iteration)
                    .collect();
                if calls.is_empty() {
                    return format!("iteration {iteration}: no tool result was completed");
                }
                let evidence = calls.iter().filter(|entry| entry.evidence).count();
                let failed = calls.iter().filter(|entry| entry.failed).count();
                format!(
                    "iteration {iteration}: {} tool call(s), {evidence} returned evidence, {failed} failed",
                    calls.len()
                )
            })
            .collect();

        AskPartialEvidence {
            label: "partial evidence — no answer was synthesized".to_string(),
            citations: self.citations.clone(),
            findings,
        }
    }
}

/// Validated request filters, resolved before the first model turn.
pub struct AskCorpus {
    source: Option<String>,
    conversation: Option<fs3_store::ConversationSummary>,
    path: Option<AskPathCoverage>,
}

impl AskCorpus {
    fn coverage(&self) -> AskCorpusCoverage {
        AskCorpusCoverage {
            source: if self.conversation.is_some() {
                "conversation".to_string()
            } else {
                self.source.clone().unwrap_or_else(|| "all".to_string())
            },
            conversation: self
                .conversation
                .as_ref()
                .map(|conversation| AskConversationCoverage {
                    guid: conversation.guid.as_str().to_string(),
                    count: 1,
                    turns: conversation.turns,
                }),
            path: self.path.clone(),
        }
    }
}

/// Validate source and resolve a conversation selector without invoking chat.
pub async fn resolve_corpus(
    state: &AppState,
    request: &AskRequest,
    scope: &Scope,
) -> Result<AskCorpus, Failure> {
    crate::search::source_filter(request.source.as_deref())?;
    if request.path.is_some()
        && (request.conversation.is_some()
            || matches!(request.source.as_deref(), Some("conversation")))
    {
        return Err(Failure::new(
            &catalog::QUERY_INVALID,
            "--path cannot scope conversations because conversation turns carry no file path",
        )
        .with_fix(
            "use `--conversation <guid>` to pin one transcript or `--repo <identity>` to scope conversations by repository",
        ));
    }
    if request.conversation.is_some() && matches!(request.source.as_deref(), Some("code" | "doc")) {
        return Err(Failure::new(
            &catalog::QUERY_INVALID,
            "--conversation conflicts with --source code or doc",
        )
        .with_fix("use `--source conversation` or `--source all` with `--conversation`"));
    }
    let conversation = match request.conversation.as_deref() {
        Some(selector) => {
            Some(crate::conversations::resolve_selector(state, selector, scope).await?)
        }
        None => None,
    };
    let path = match request.path.as_deref() {
        Some(glob) => {
            let pattern = crate::search::glob_to_like(glob);
            let kinds = crate::search::path_source_filter(request.source.as_deref())?;
            let probe = fs3_store::path_filter_probe(
                &state.db,
                scope.repo.as_deref(),
                scope.worktree.as_deref(),
                &pattern,
                Some(&kinds),
            )
            .await
            .map_err(crate::runner::fail)?;
            if !probe.matches {
                let reason = crate::search::path_unmatched_reason(glob, probe.clone())
                    .unwrap_or_else(|| crate::search::EmptyBecause {
                        reason: "path_unmatched",
                        detail: format!(
                            "the --path filter {glob:?} matches zero indexed paths in this scope; no indexed file paths are available to answer from"
                        ),
                        hint: Some(
                            "index the intended checkout with `flowspace3 add <path>`, or correct --repo/--path to a scope that contains files"
                                .to_string(),
                        ),
                    });
                let fix = reason
                    .hint
                    .clone()
                    .unwrap_or_else(|| "correct the --path filter".to_string());
                return Err(Failure::new(&catalog::QUERY_INVALID, reason.detail.clone())
                    .with_fix(fix)
                    .with_detail("empty_because", reason));
            }
            Some(AskPathCoverage {
                glob: glob.to_string(),
                elements: probe.matching_elements,
                conversation_exclusion: matches!(request.source.as_deref(), None | Some("all"))
                    .then(|| {
                        "conversations carry no file path, so --path excludes them".to_string()
                    }),
            })
        }
        None => None,
    };
    Ok(AskCorpus {
        source: request.source.clone(),
        conversation,
        path,
    })
}

/// The tools the loop may call, bound to one request's scope.
pub struct IndexTools<'a> {
    state: &'a AppState,
    scope: Scope,
    /// Caller-selected source, immutable across model-issued searches.
    source: Option<String>,
    /// Canonical conversation pin, immutable across searches and reads.
    conversation: Option<ConversationId>,
    /// Caller-selected path glob, immutable across searches and reads.
    path: Option<String>,
    /// Addresses actually read, in call order — the citation record.
    read: std::sync::Mutex<Vec<String>>,
    /// Top-k used by each valid search call, in call order.
    search_limits: std::sync::Mutex<Vec<i64>>,
}

impl<'a> IndexTools<'a> {
    /// Bind the toolbox to the state and the scope this request resolved to.
    pub fn new(state: &'a AppState, scope: Scope) -> Self {
        Self::with_filters(state, scope, None, None, None)
    }

    fn with_corpus(state: &'a AppState, mut scope: Scope, corpus: &AskCorpus) -> Self {
        if corpus.conversation.is_some() && scope.source != ScopeSource::Flag {
            scope.repo = None;
            scope.worktree = None;
        }
        Self::with_filters(
            state,
            scope,
            corpus.source.clone(),
            corpus
                .conversation
                .as_ref()
                .map(|conversation| conversation.guid.clone()),
            corpus.path.as_ref().map(|path| path.glob.clone()),
        )
    }

    fn with_filters(
        state: &'a AppState,
        scope: Scope,
        source: Option<String>,
        conversation: Option<ConversationId>,
        path: Option<String>,
    ) -> Self {
        Self {
            state,
            scope,
            source,
            conversation,
            path,
            read: std::sync::Mutex::new(Vec::new()),
            search_limits: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The addresses `get` was successfully called on, deduplicated in order.
    pub fn citations(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for address in self.read.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            if !seen.contains(address) {
                seen.push(address.clone());
            }
        }
        seen
    }

    /// The top-k used by each valid search call, in call order.
    pub fn search_limits(&self) -> Vec<i64> {
        self.search_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// How the immutable scope should be described to the model, every time.
    fn scope_line(&self, scope: &Scope) -> String {
        let path = self
            .path
            .as_ref()
            .map(|path| format!("; paths matching `{path}` only"))
            .unwrap_or_default();
        if let Some(conversation) = &self.conversation {
            let repository = if scope.source == ScopeSource::Flag {
                scope.repo.as_ref().map_or_else(
                    || "every indexed repository".to_string(),
                    |repo| format!("repository `{repo}` only"),
                )
            } else {
                "every indexed repository".to_string()
            };
            return format!(
                "scope: {repository}; corpus: one conversation `conv:{}` only",
                conversation.as_str()
            );
        }
        let repository = match &scope.repo {
            Some(repo) => format!("repository `{repo}` only"),
            None => "every indexed repository".to_string(),
        };
        match self.source.as_deref() {
            Some(source) => format!("scope: {repository}{path}; source: {source} only"),
            None => {
                format!("scope: {repository}{path}; source: model-selectable within all sources")
            }
        }
    }

    async fn run_search(&self, arguments: &Value) -> CoreResult<ToolOutcome> {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| provider_error("search needs a string `query`"))?;

        // The model's `repo` argument has to RE-RESOLVE the scope, not just
        // ride along on the request. `search` filters and picks its model key
        // from the Scope alone, so passing `repo` through `SearchRequest` and
        // reusing the request's original scope would leave `repo="all"` a
        // silent no-op — the tool would promise widening in its description,
        // report a scope it had not applied, and the model would conclude the
        // subject does not exist anywhere. Offering a widening that does not
        // widen is worse than offering none: it manufactures false confidence.
        let asked = arguments.get("repo").and_then(Value::as_str);
        let scope = match asked {
            Some(repo) => {
                crate::scope::resolve(self.state, Some(repo), self.scope.cwd.as_deref()).await
            }
            None => self.scope.clone(),
        };

        let request = SearchRequest {
            q: query.to_string(),
            repo: asked.map(str::to_string).or_else(|| scope.repo.clone()),
            cwd: scope.cwd.clone(),
            path: self.path.clone().or_else(|| {
                arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            source: if self.conversation.is_some() {
                Some("conversation".to_string())
            } else if self.path.is_some() {
                Some(self.source.clone().unwrap_or_else(|| "all".to_string()))
            } else {
                self.source.clone().or_else(|| {
                    arguments
                        .get("source")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
            },
            min_score: None,
            limit: Some({
                let limit = arguments
                    .get("limit")
                    .and_then(Value::as_i64)
                    .unwrap_or(6)
                    .clamp(1, 15);
                self.search_limits
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(limit);
                limit
            }),
            ..SearchRequest::default()
        };

        let results = match &self.conversation {
            Some(conversation) => {
                crate::search::search_in_conversation(self.state, &request, &scope, conversation)
                    .await
            }
            None => crate::search::search(self.state, &request, &scope).await,
        }
        .map_err(|failure| provider_error(&format!("[{}] {}", failure.code, failure.message)))?;

        if let Some(reason) = &results.empty_because
            && reason.reason == "path_unmatched"
        {
            let hint = reason
                .hint
                .as_deref()
                .unwrap_or("correct the --path filter");
            return Ok(ToolOutcome::nothing(format!(
                "PATH FILTER UNMATCHED ({}).\n{}\n{}\nDo NOT conclude that the requested \
                 code is absent. Correct the path filter and search again.",
                self.scope_line(&scope),
                reason.detail,
                hint
            )));
        }

        if results.results.is_empty() {
            // The scope-trap guard. A bare "no results" invites the model to
            // conclude the thing does not exist; naming the scope and the way
            // out keeps a scoped miss from being read as a global absence.
            // NOT evidence. The call succeeded and the index answered
            // "nothing here" — counting that as grounding would let a model
            // search once, match nothing, and invent an answer that the report
            // then vouches for.
            let guidance = if self.conversation.is_some() {
                "This does NOT mean the subject is absent from the repository — only that nothing \
                 matched in this ONE pinned conversation. Try a shorter, differently worded query, \
                 or remove --conversation if the question is not transcript-specific."
            } else {
                "This does NOT mean the subject does not exist — only that nothing matched IN THIS \
                 SCOPE. If the answer might live in another repository, call search again with \
                 repo=\"all\". Otherwise try a shorter, differently worded query."
            };
            return Ok(ToolOutcome::nothing(format!(
                "NO HITS ({}).\n{guidance}",
                self.scope_line(&scope)
            )));
        }

        let mut rendered = format!(
            "{} — {} hits\n",
            self.scope_line(&scope),
            results.results.len()
        );
        let mut addresses = Vec::with_capacity(results.results.len());
        for hit in &results.results {
            let gist = hit
                .smart
                .as_deref()
                .unwrap_or(hit.snippet.as_str())
                .replace('\n', " ");
            rendered.push_str(&format!(
                "- address: {}\n  path: {} (score {:.2})\n  gist: {gist}\n",
                hit.address,
                hit.path.as_deref().unwrap_or("(unknown path)"),
                hit.score
            ));
            addresses.push(hit.address.clone());
        }
        Ok(ToolOutcome::evidence_with_references(rendered, addresses))
    }

    fn guard_address(&self, address: &str) -> CoreResult<()> {
        let parsed = Address::parse(address)
            .map_err(|error| provider_error(&format!("get address is invalid: {error}")))?;
        if self.path.is_some() && matches!(&parsed, Address::Conversation(_)) {
            return Err(provider_error(
                "get is outside the caller's immutable --path scope; conversations carry no file path",
            ));
        }
        if let Some(conversation) = &self.conversation {
            return match parsed {
                Address::Conversation(candidate)
                    if candidate.guid == conversation.as_str() && candidate.turn.is_some() =>
                {
                    Ok(())
                }
                _ => Err(provider_error(&format!(
                    "get is outside the pinned conversation `conv:{}`; use an address returned by search",
                    conversation.as_str()
                ))),
            };
        }
        match (self.source.as_deref(), parsed) {
            (Some("conversation"), Address::Conversation(_)) | (None | Some("all"), _) => Ok(()),
            (Some("conversation"), _) => Err(provider_error(
                "get is outside the caller's --source conversation scope",
            )),
            (Some("code" | "doc"), Address::Conversation(_)) => Err(provider_error(
                "get is outside the caller's non-conversation source scope",
            )),
            (Some("code" | "doc"), _) => Ok(()),
            (Some(other), _) => Err(provider_error(&format!(
                "unsupported immutable ask source `{other}`"
            ))),
        }
    }

    fn payload_in_scope(&self, payload: &GetPayload) -> bool {
        if self.conversation.is_some() {
            return true;
        }
        let GetPayload::Conversation(window) = payload else {
            return true;
        };
        let repo_matches = self
            .scope
            .repo
            .as_deref()
            .is_none_or(|repo| window.repo.as_deref() == Some(repo));
        let worktree_matches = match (self.scope.worktree.as_deref(), window.worktree.as_deref()) {
            (Some(expected), Some(actual)) => expected == actual,
            _ => true,
        };
        repo_matches && worktree_matches
    }

    fn payload_in_source(&self, payload: &GetPayload) -> bool {
        match (self.source.as_deref(), payload) {
            (None | Some("all"), _) => true,
            (Some("conversation"), GetPayload::Conversation(_)) => true,
            (Some("code"), GetPayload::Element(element)) => {
                matches!(element.kind.as_str(), "file" | "container" | "function")
            }
            (Some("doc"), GetPayload::Element(element)) => {
                matches!(element.kind.as_str(), "section" | "row")
            }
            _ => false,
        }
    }

    fn payload_in_path(&self, payload: &GetPayload) -> bool {
        match (self.path.as_deref(), payload) {
            (None, _) => true,
            (Some(glob), GetPayload::Element(element)) => {
                crate::search::path_matches_glob(&element.path, glob)
            }
            (Some(_), GetPayload::Conversation(_)) => false,
        }
    }

    async fn run_get(&self, arguments: &Value) -> CoreResult<ToolOutcome> {
        let address = arguments
            .get("address")
            .and_then(Value::as_str)
            .ok_or_else(|| provider_error("get needs a string `address`"))?;
        self.guard_address(address)?;
        let request = crate::read::GetRequest {
            address: address.to_string(),
            ..Default::default()
        };

        let (payload, _source) = crate::read::get(self.state, &request, &self.scope)
            .await
            .map_err(|failure| provider_error(&failure.message))?;
        if !self.payload_in_scope(&payload) {
            return Err(provider_error(
                "get resolved outside the caller's immutable repository scope",
            ));
        }
        if !self.payload_in_source(&payload) {
            return Err(provider_error(
                "get resolved outside the caller's immutable --source scope",
            ));
        }
        if !self.payload_in_path(&payload) {
            return Err(provider_error(
                "get resolved outside the caller's immutable --path scope",
            ));
        }

        self.read
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(address.to_string());

        serde_json::to_string_pretty(&payload)
            .map(ToolOutcome::evidence)
            .map_err(|e| provider_error(&format!("could not render {address}: {e}")))
    }
}

fn provider_error(message: &str) -> fs3_core::Error {
    fs3_core::Error::Provider(message.to_string())
}

#[async_trait]
impl ToolBox for IndexTools<'_> {
    fn schemas(&self) -> Vec<ToolSchema> {
        // These descriptions are PROMPT, not documentation: the model chooses
        // tools and arguments by reading them, so vagueness here shows up as
        // bad tool selection rather than as a bad doc.
        vec![
            ToolSchema {
                name: "search".into(),
                description: format!(
                    "Semantic search over the complete code, document, and conversation index. \
                     Returns hits with an `address` to pass to `get`. Ask meaning-shaped questions, \
                     not identifiers. Current {}. Pass repo=\"all\" to search every indexed \
                     repository, or source=\"code\"|\"doc\"|\"conversation\" to narrow the corpus.",
                    self.scope_line(&self.scope)
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "A short, meaning-shaped question or phrase."},
                        "repo": {"type": "string", "description": "A repository identity, or \"all\" to widen to every indexed repository."},
                        "path": {"type": "string", "description": "Optional glob to restrict paths, e.g. \"crates/daemon/**\"."},
                        "source": {"type": "string", "enum": ["all", "code", "doc", "conversation"], "description": "Content corpus; absent or all searches every source."},
                        "limit": {"type": "integer", "description": "How many hits (1-15, default 6)."}
                    },
                    "required": ["query"]
                }),
            },
            ToolSchema {
                name: "get".into(),
                description:
                    "Read one address in full — a code/document element or a conversation \
                              turn window. Use the `address` exactly as `search` returned it."
                        .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "An address as returned by search."}
                    },
                    "required": ["address"]
                }),
            },
        ]
    }

    async fn call(&self, name: &str, arguments: &str) -> CoreResult<ToolOutcome> {
        // Malformed arguments are the model's mistake to correct, so they come
        // back as an error the loop hands over as a tool result.
        let parsed: Value = serde_json::from_str(arguments)
            .map_err(|e| provider_error(&format!("arguments were not valid JSON: {e}")))?;

        match name {
            "search" => self.run_search(&parsed).await,
            "get" => self.run_get(&parsed).await,
            other => Err(provider_error(&format!(
                "unknown tool `{other}`; available tools are: search, get"
            ))),
        }
    }
}

/// Answer one question against the index.
///
/// # Errors
/// Only when the chat provider itself fails. Tool failures never surface here:
/// they are fed back to the model, which is what lets it recover.
pub async fn ask(
    state: &AppState,
    request: &AskRequest,
    scope: Scope,
    corpus: AskCorpus,
) -> CoreResult<AskReport> {
    let chat = state.agent_for(scope.repo.as_deref());

    // Refuse BEFORE the first turn when the port cannot answer. The offline
    // fake is wired, healthy and incapable: it can only emit a placeholder,
    // and that placeholder once reached callers as a real `answer` on an
    // `ok: true` envelope. Our own envelope rule tells consumers to branch on
    // `ok` alone, so a non-answer riding a success is not a cosmetic problem —
    // it is the verb lying in the one field everything downstream trusts.
    // `grounded: false` and the suspicious next_action were both present and
    // both insufficient, because neither is where a machine looks.
    if !chat.can_answer() {
        return Err(fs3_core::Error::Provider(format!(
            "the agent port is wired to `{}`, which cannot answer questions",
            chat.key()
        )));
    }
    let bounds = AgentBounds {
        max_iterations: state.config.agent.max_iterations,
        token_budget: state.config.agent.token_budget,
        tool_result_max_chars: state.config.agent.tool_result_max_chars,
    };
    let corpus_coverage = corpus.coverage();
    let tools = IndexTools::with_corpus(state, scope, &corpus);

    let answer: AgentAnswer =
        fs3_core::ask(chat.as_ref(), &tools, bounds, &request.question).await?;

    Ok(AskReport {
        question: request.question.clone(),
        answer: answer.answer,
        failure: answer.failure,
        citations: tools.citations(),
        trace: answer
            .trace
            .into_iter()
            .map(|entry| AskTraceEntry {
                iteration: entry.iteration,
                tool: entry.tool,
                arguments: entry.arguments,
                failed: entry.failed,
                evidence: entry.evidence,
                search_hits: entry.references,
                result_chars: entry.result_chars,
            })
            .collect(),
        coverage: AskCoverage {
            iterations_used: answer.iterations,
            iteration_limit: bounds.max_iterations,
            retrieval_top_k: tools.search_limits(),
            corpus: corpus_coverage,
            exhaustive: false,
        },
        iterations: answer.iterations,
        tokens_used: answer.tokens_used,
        grounded: answer.grounded,
        stopped: match answer.stopped {
            StopReason::Answered => "answered",
            StopReason::MaxIterations => "max_iterations",
            StopReason::TokenBudget => "token_budget",
            StopReason::ProviderFailure => "provider_failure",
        }
        .to_string(),
        model: chat.key(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct LegacyAskCorpusCoverage<'a> {
        source: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        conversation: Option<&'a AskConversationCoverage>,
    }

    #[test]
    fn absent_path_preserves_legacy_corpus_serialization_bytes() {
        let current = AskCorpus {
            source: None,
            conversation: None,
            path: None,
        }
        .coverage();
        let legacy = LegacyAskCorpusCoverage {
            source: "all",
            conversation: None,
        };

        assert_eq!(
            serde_json::to_vec(&current).unwrap(),
            serde_json::to_vec(&legacy).unwrap(),
            "adding --path must add no envelope bytes when the flag is absent"
        );
    }
}
