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
    AgentAnswer, AgentBounds, Result as CoreResult, ToolBox, ToolOutcome, ToolSchema,
    agent::StopReason,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::scope::Scope;
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

/// The finite probe behind an answer, stated separately from its prose.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskCoverage {
    /// Model turns consumed by this run.
    pub iterations_used: u32,
    /// Maximum model turns the run was allowed to consume.
    pub iteration_limit: u32,
    /// The top-k requested by each valid search call, in call order.
    pub retrieval_top_k: Vec<i64>,
    /// Always false: bounded nearest-neighbour retrieval cannot prove completeness.
    pub exhaustive: bool,
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
    /// Why the run ended: `answered`, `max_iterations` or `token_budget`.
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

/// The tools the loop may call, bound to one request's scope.
pub struct IndexTools<'a> {
    state: &'a AppState,
    scope: Scope,
    /// Addresses actually read, in call order — the citation record.
    read: std::sync::Mutex<Vec<String>>,
    /// Top-k used by each valid search call, in call order.
    search_limits: std::sync::Mutex<Vec<i64>>,
}

impl<'a> IndexTools<'a> {
    /// Bind the toolbox to the state and the scope this request resolved to.
    pub fn new(state: &'a AppState, scope: Scope) -> Self {
        Self {
            state,
            scope,
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

    /// How a scope should be described to the model, every time.
    ///
    /// Takes the scope that was actually APPLIED rather than reading the
    /// request's, so a widened call reports the width it really had.
    fn scope_line(scope: &Scope) -> String {
        match &scope.repo {
            Some(repo) => format!("scope: repository `{repo}` only"),
            None => "scope: every indexed repository".to_string(),
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
            path: arguments
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string),
            source: None,
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

        let results = crate::search::search(self.state, &request, &scope)
            .await
            .map_err(|failure| {
                provider_error(&format!("[{}] {}", failure.code, failure.message))
            })?;

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
                Self::scope_line(&scope),
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
            return Ok(ToolOutcome::nothing(format!(
                "NO HITS ({}).\nThis does NOT mean the subject does not exist — only that nothing \
                 matched IN THIS SCOPE. If the answer might live in another repository, call \
                 search again with repo=\"all\". Otherwise try a shorter, differently worded query.",
                Self::scope_line(&scope)
            )));
        }

        let mut rendered = format!(
            "{} — {} hits\n",
            Self::scope_line(&scope),
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

    async fn run_get(&self, arguments: &Value) -> CoreResult<ToolOutcome> {
        let address = arguments
            .get("address")
            .and_then(Value::as_str)
            .ok_or_else(|| provider_error("get needs a string `address`"))?;
        let request = crate::read::GetRequest {
            address: address.to_string(),
            ..Default::default()
        };

        let (payload, _source) = crate::read::get(self.state, &request, &self.scope)
            .await
            .map_err(|failure| provider_error(&failure.message))?;

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
                    "Semantic search over the code index. Returns hits with an `address` to pass \
                     to `get`. Ask meaning-shaped questions, not identifiers. Current {}. Pass \
                     repo=\"all\" to search every indexed repository when the answer may live \
                     outside the current one.",
                    Self::scope_line(&self.scope)
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "A short, meaning-shaped question or phrase."},
                        "repo": {"type": "string", "description": "A repository identity, or \"all\" to widen to every indexed repository."},
                        "path": {"type": "string", "description": "Optional glob to restrict paths, e.g. \"crates/daemon/**\"."},
                        "limit": {"type": "integer", "description": "How many hits (1-15, default 6)."}
                    },
                    "required": ["query"]
                }),
            },
            ToolSchema {
                name: "get".into(),
                description: "Read one address in full — an element with its children, or a whole \
                              file. Use the `address` exactly as `search` returned it."
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
pub async fn ask(state: &AppState, request: &AskRequest, scope: Scope) -> CoreResult<AskReport> {
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
    let tools = IndexTools::new(state, scope);

    let answer: AgentAnswer =
        fs3_core::ask(chat.as_ref(), &tools, bounds, &request.question).await?;

    Ok(AskReport {
        question: request.question.clone(),
        answer: answer.answer,
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
            exhaustive: false,
        },
        iterations: answer.iterations,
        tokens_used: answer.tokens_used,
        grounded: answer.grounded,
        stopped: match answer.stopped {
            StopReason::Answered => "answered",
            StopReason::MaxIterations => "max_iterations",
            StopReason::TokenBudget => "token_budget",
        }
        .to_string(),
        model: chat.key(),
    })
}
