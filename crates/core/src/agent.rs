//! The bounded agent loop behind `flowspace3 ask`.
//!
//! A question comes in; the loop hands it to a chat model along with the tools
//! that can read the index; the model asks for searches and reads; the loop
//! runs them and hands the results back; eventually the model answers. That is
//! the whole idea, and it is small enough to own outright — which is why fs3
//! has no agent-framework dependency.
//!
//! It lives in core, and therefore performs no IO. Both effectful halves are
//! injected: [`crate::ChatProvider`] talks to the model, [`ToolBox`] runs the
//! tools. The shell supplies real ones; a test supplies fakes and the entire
//! loop — bounds, recovery, grounding — is provable offline with no network and
//! no database.
//!
//! ## Two properties this module exists to guarantee
//!
//! **It is BOUNDED.** A loop that calls a paid API in a cycle is a bill with no
//! ceiling, and "the model will stop eventually" is not a design. Every run is
//! capped three ways ([`AgentBounds`]): iterations, total tokens, and the size
//! of any single tool result. All three are configuration, not constants,
//! because the right numbers differ per model and per deployment.
//!
//! **It is GROUNDED.** The answer must come from what the tools actually
//! returned. A model asked about code it cannot see will happily produce a
//! fluent, plausible, wrong answer, and a confident wrong answer about your own
//! codebase is worse than no answer — you cannot tell it apart from a right
//! one. So the prompt demands citations and an explicit "I could not find it",
//! and [`SYSTEM_PROMPT`] is part of the contract rather than a tuneable.
//!
//! ## Bad tool calls are data, not failures
//!
//! Models emit unknown tool names and malformed JSON arguments. None of that
//! ends a run: the error goes back as the tool's result and the model corrects
//! itself on the next turn. This is not politeness — it was measured. In the
//! prototype the model asked for an address that needed a disambiguating
//! argument, got the error back as a result, and recovered unaided on the
//! following turn. A loop that had returned `Err` there would have thrown away
//! a run that was about to succeed.

use async_trait::async_trait;

use crate::error::Result;
use crate::ports::{ChatMessage, ChatProvider, ChatTurn, ToolSchema};

/// The standing instruction every run opens with.
///
/// Part of the contract, not a knob. The grounding rules here are the reason
/// the answer can be trusted, so they are not left to a caller to remember.
pub const SYSTEM_PROMPT: &str = "You are a code-question agent for the flowspace3 semantic index. \
Answer the user's question by CALLING TOOLS to gather evidence: search for relevant code, then \
read the addresses that matter. Keep queries short and meaning-shaped.\n\
RULES:\n\
1. Ground every claim in tool results. Never answer from prior knowledge of any codebase.\n\
2. Cite the addresses you used at the end, under 'Sources:'.\n\
3. If the tools do not surface real evidence, say plainly that you could not find it. \
An honest 'not found' beats a plausible guess.\n\
4. Prefer a few focused tool calls over many speculative ones.\n\
Finish with a concise answer in prose.";

/// The three caps on one run.
///
/// Defaults are the prototype's measured values: they answered a real
/// architectural question about this repository in 7 turns and roughly 45k
/// tokens, so 8 turns and 80k tokens is headroom rather than a guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentBounds {
    /// How many model turns before the loop gives up.
    pub max_iterations: u32,
    /// Total tokens across the run, as reported by the provider.
    pub token_budget: u64,
    /// The longest a single tool result may be before it is truncated.
    ///
    /// A whole-file read can be enormous, and one such result can crowd out
    /// every other piece of evidence in the context window.
    pub tool_result_max_chars: usize,
}

impl Default for AgentBounds {
    fn default() -> Self {
        Self {
            max_iterations: 8,
            token_budget: 80_000,
            tool_result_max_chars: 7_000,
        }
    }
}

/// The tools the loop can run.
///
/// The second injected seam. Core cannot reach the store or the network, so the
/// daemon implements this over its own in-process query services — the same
/// code path `search` and `get` already use, not a subprocess and not an HTTP
/// round trip.
#[async_trait]
pub trait ToolBox: Send + Sync {
    /// The tools to offer the model, with their argument schemas.
    fn schemas(&self) -> Vec<ToolSchema>;

    /// Run one tool.
    ///
    /// `arguments` is the raw JSON string the model emitted and may be
    /// malformed — implementations report that as an `Err` and the loop turns
    /// it into a result the model can read.
    ///
    /// # Errors
    /// Any failure the tool itself reports. This ends the CALL, never the run.
    async fn call(&self, name: &str, arguments: &str) -> Result<String>;
}

/// Why a run stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// The model produced prose. The ordinary ending.
    Answered,
    /// [`AgentBounds::max_iterations`] was reached first.
    MaxIterations,
    /// [`AgentBounds::token_budget`] was exhausted first.
    TokenBudget,
}

/// One tool call as it happened, for the trace the user sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEntry {
    /// Which turn it happened on, counting from 1.
    pub iteration: u32,
    /// The tool the model asked for — as asked, even if no such tool exists.
    pub tool: String,
    /// The arguments it sent, verbatim.
    pub arguments: String,
    /// Whether the call produced a result or an error the model had to recover
    /// from. Both are normal; a run with a recovered error is a healthy run.
    pub failed: bool,
    /// Size of the result handed back, after any truncation.
    pub result_chars: usize,
}

/// What a run produced.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentAnswer {
    /// The model's prose. Absent only when the run hit a bound before
    /// answering — the caller must say so rather than invent an answer.
    pub answer: Option<String>,
    /// Every tool call, in order.
    pub trace: Vec<TraceEntry>,
    /// Turns taken.
    pub iterations: u32,
    /// Tokens spent, or `None` when the provider reported nothing.
    ///
    /// `None` is not zero. A provider that declines to report usage has told us
    /// it does not know, and flattening that to `0` would publish a number
    /// nobody measured — the run would read as free. Unknown stays unknown all
    /// the way to the caller, so a budget assertion can say "not measured"
    /// instead of silently passing.
    pub tokens_used: Option<u64>,
    /// Why it ended.
    pub stopped: StopReason,
    /// Whether the answer rests on evidence the loop actually read.
    ///
    /// False means the model answered without ever successfully reading
    /// anything — from its own memory, in other words. The loop pushes back
    /// once before allowing that (see [`ask`]), so a false here is a model that
    /// insisted. It is a FIELD rather than a warning in prose because an
    /// ungrounded answer that reads like a grounded one is the failure this
    /// verb exists to prevent, and a caller — or an evaluator — must be able to
    /// tell them apart without parsing English.
    pub grounded: bool,
}

/// Run one question to an answer.
///
/// # Errors
/// [`crate::Error::Provider`] only when the model itself fails. Tool failures
/// and malformed tool calls never surface here — they are fed back to the model
/// as results, which is what lets it recover.
pub async fn ask(
    chat: &dyn ChatProvider,
    tools: &dyn ToolBox,
    bounds: AgentBounds,
    question: &str,
) -> Result<AgentAnswer> {
    let schemas = tools.schemas();
    let mut messages = vec![
        ChatMessage::System(SYSTEM_PROMPT.to_string()),
        ChatMessage::User(question.to_string()),
    ];
    let mut trace = Vec::new();
    // `None` until a provider reports usage: see `AgentAnswer::tokens_used`.
    let mut tokens_used: Option<u64> = None;
    let mut nudged = false;

    for iteration in 1..=bounds.max_iterations {
        let turn: ChatTurn = chat.turn(&messages, &schemas).await?;
        if let Some(spent) = turn.tokens_used {
            tokens_used = Some(tokens_used.unwrap_or(0) + spent);
        }

        if turn.tool_calls.is_empty() {
            let grounded = read_something(&trace);

            // A model that answers having read NOTHING is answering from
            // memory, which is precisely what this verb must not do. Asking is
            // cheap and it usually works, so push back once rather than
            // publishing the answer or throwing it away.
            if !grounded && !nudged {
                nudged = true;
                messages.push(ChatMessage::Assistant {
                    content: turn.content.clone(),
                    tool_calls: vec![],
                });
                messages.push(ChatMessage::User(
                    "You answered without reading anything from the index, so that answer is \
                     not grounded in this codebase. Use the search and get tools to find real \
                     evidence, then answer citing the addresses you read. If you search and \
                     genuinely find nothing, say so plainly — an honest 'not found' is a \
                     correct answer, but a remembered one is not."
                        .to_string(),
                ));
                continue;
            }

            return Ok(AgentAnswer {
                answer: turn.content,
                trace,
                iterations: iteration,
                tokens_used,
                stopped: StopReason::Answered,
                grounded,
            });
        }

        // The assistant's own request must be replayed verbatim alongside the
        // results, or the next turn cannot tell which result answers what.
        messages.push(ChatMessage::Assistant {
            content: turn.content.clone(),
            tool_calls: turn.tool_calls.clone(),
        });

        for call in &turn.tool_calls {
            let (content, failed) = match tools.call(&call.name, &call.arguments).await {
                Ok(result) => (truncate(result, bounds.tool_result_max_chars), false),
                // Deliberately a result, not an early return: see the module docs.
                Err(error) => (format!("ERROR: {error}"), true),
            };
            trace.push(TraceEntry {
                iteration,
                tool: call.name.clone(),
                arguments: call.arguments.clone(),
                failed,
                result_chars: content.chars().count(),
            });
            messages.push(ChatMessage::ToolResult {
                tool_call_id: call.id.clone(),
                content,
            });
        }

        if tokens_used.is_some_and(|spent| spent >= bounds.token_budget) {
            let grounded = read_something(&trace);
            return Ok(AgentAnswer {
                answer: None,
                trace,
                iterations: iteration,
                tokens_used,
                stopped: StopReason::TokenBudget,
                grounded,
            });
        }
    }

    let grounded = read_something(&trace);
    Ok(AgentAnswer {
        answer: None,
        trace,
        iterations: bounds.max_iterations,
        tokens_used,
        stopped: StopReason::MaxIterations,
        grounded,
    })
}

/// Whether any tool call actually returned evidence.
///
/// A run where every call failed has nothing in it, however fluent the reply —
/// so this, and not the presence of an answer, is what "grounded" means.
fn read_something(trace: &[TraceEntry]) -> bool {
    trace.iter().any(|entry| !entry.failed)
}

/// Cut `text` to `max` characters, saying so where it was cut.
///
/// Character-based rather than byte-based: a byte cut can land inside a
/// multi-byte character, and source code is full of them.
fn truncate(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}\n…[truncated at {max} characters]")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::ports::ToolCall;

    /// Drive a future to completion with no runtime.
    ///
    /// Core takes no tokio — rule 2, the functional core performs no IO — so
    /// there is no `#[tokio::test]` available here and adding one would put an
    /// async runtime in the dependency graph of a crate that exists precisely
    /// to have none. It is not needed: every future in these tests is built
    /// from fakes that return immediately, so nothing ever parks and a bare
    /// poll loop is a complete executor for them.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};

        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                // Unreachable with these fakes; if a future ever does park here
                // it means a test grew a real await, and spinning makes that
                // loud rather than silently wrong.
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    /// Run the loop and unwrap it — every test wants the answer, not the
    /// `Result`, because a provider error is its own separate test.
    fn run(future: impl std::future::Future<Output = Result<AgentAnswer>>) -> AgentAnswer {
        block_on(future).expect("the loop completed")
    }

    /// A chat model that replays a script, and records what it was asked.
    struct ScriptedChat {
        turns: Mutex<std::collections::VecDeque<ChatTurn>>,
        seen: Mutex<Vec<Vec<ChatMessage>>>,
    }

    impl ScriptedChat {
        fn new(turns: Vec<ChatTurn>) -> Self {
            Self {
                turns: Mutex::new(turns.into()),
                seen: Mutex::new(Vec::new()),
            }
        }

        fn last_conversation(&self) -> Vec<ChatMessage> {
            self.seen
                .lock()
                .unwrap()
                .last()
                .cloned()
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl ChatProvider for ScriptedChat {
        async fn turn(&self, messages: &[ChatMessage], _tools: &[ToolSchema]) -> Result<ChatTurn> {
            self.seen.lock().unwrap().push(messages.to_vec());
            Ok(self
                .turns
                .lock()
                .unwrap()
                .pop_front()
                .expect("the script ran out of turns"))
        }

        fn key(&self) -> String {
            "scripted@1".into()
        }

        fn max_input_tokens(&self) -> usize {
            100_000
        }
    }

    /// A toolbox that answers one way, or refuses.
    struct StubTools {
        answer: Result<String>,
    }

    #[async_trait]
    impl ToolBox for StubTools {
        fn schemas(&self) -> Vec<ToolSchema> {
            vec![ToolSchema {
                name: "search".into(),
                description: "search the index".into(),
                parameters: json!({"type": "object", "properties": {}}),
            }]
        }

        async fn call(&self, _name: &str, _arguments: &str) -> Result<String> {
            match &self.answer {
                Ok(text) => Ok(text.clone()),
                Err(error) => Err(crate::Error::Provider(error.to_string())),
            }
        }
    }

    fn tools_ok(answer: &str) -> StubTools {
        StubTools {
            answer: Ok(answer.to_string()),
        }
    }

    fn call(id: &str, name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }

    fn prose(text: &str, tokens: u64) -> ChatTurn {
        ChatTurn {
            content: Some(text.into()),
            tool_calls: vec![],
            tokens_used: Some(tokens),
        }
    }

    fn calls(tool_calls: Vec<ToolCall>, tokens: u64) -> ChatTurn {
        ChatTurn {
            content: None,
            tool_calls,
            tokens_used: Some(tokens),
        }
    }

    #[test]
    fn a_model_that_answers_without_reading_anything_is_pushed_back_once() {
        // This test previously asserted the OPPOSITE — that first-turn prose
        // with no tool calls was simply the answer. That enshrined the exact
        // failure the verb exists to prevent: a fluent reply drawn from the
        // model's memory, indistinguishable from one drawn from the codebase.
        // The loop now refuses it once and demands evidence.
        let chat = ScriptedChat::new(vec![
            prose("I remember this codebase debounces per directory", 10),
            calls(vec![call("c1", "search", "{}")], 10),
            prose("grounded answer", 10),
        ]);

        let outcome = run(ask(&chat, &tools_ok("HIT"), AgentBounds::default(), "q"));

        assert_eq!(outcome.answer.as_deref(), Some("grounded answer"));
        assert!(outcome.grounded, "it read something before answering");
        assert_eq!(outcome.iterations, 3, "the pushback cost a turn");

        // The pushback must be visible to the model as a user turn, or it is
        // not a mechanism — it is a hope.
        let conversation = chat.last_conversation();
        assert!(
            conversation.iter().any(|message| matches!(
                message,
                ChatMessage::User(text) if text.contains("not grounded")
            )),
            "the model must be TOLD why its answer was refused: {conversation:?}"
        );
    }

    #[test]
    fn a_model_that_insists_on_answering_ungrounded_is_reported_as_ungrounded() {
        // Pushback is one nudge, not a fight. A model that answers from memory
        // twice gets published — but flagged, so a caller or an evaluator can
        // tell a remembered answer from a read one without parsing prose.
        let chat = ScriptedChat::new(vec![
            prose("from memory", 10),
            prose("still from memory", 10),
        ]);

        let outcome = run(ask(&chat, &tools_ok("unused"), AgentBounds::default(), "q"));

        assert_eq!(outcome.answer.as_deref(), Some("still from memory"));
        assert!(
            !outcome.grounded,
            "nothing was read, so nothing is grounded"
        );
        assert_eq!(outcome.stopped, StopReason::Answered);
    }

    #[test]
    fn an_answer_after_every_tool_call_failed_is_not_grounded() {
        // Tool errors are data, so the run completes — but a run where nothing
        // succeeded has no evidence in it, and saying otherwise would let a
        // broken index masquerade as a working one.
        let chat = ScriptedChat::new(vec![
            calls(vec![call("c1", "search", "{}")], 10),
            prose("answered anyway", 10),
            prose("answered anyway", 10),
        ]);
        let tools = StubTools {
            answer: Err(crate::Error::Provider("the index is empty".into())),
        };

        let outcome = run(ask(&chat, &tools, AgentBounds::default(), "q"));

        assert_eq!(outcome.answer.as_deref(), Some("answered anyway"));
        assert!(
            !outcome.grounded,
            "every tool call failed, so the answer rests on nothing"
        );
    }

    #[test]
    fn the_loop_runs_a_tool_and_feeds_the_result_back_to_the_model() {
        let chat = ScriptedChat::new(vec![
            calls(vec![call("c1", "search", r#"{"query":"watcher"}"#)], 20),
            prose("grounded answer", 20),
        ]);
        let outcome = run(ask(
            &chat,
            &tools_ok("HIT: watcher.rs"),
            AgentBounds::default(),
            "q",
        ));

        assert_eq!(outcome.answer.as_deref(), Some("grounded answer"));
        assert_eq!(outcome.trace.len(), 1);
        assert!(!outcome.trace[0].failed);

        // The model must see its own request AND the result, or the second turn
        // cannot tell which result answers what.
        let conversation = chat.last_conversation();
        assert!(matches!(conversation[2], ChatMessage::Assistant { .. }));
        assert_eq!(
            conversation[3],
            ChatMessage::ToolResult {
                tool_call_id: "c1".into(),
                content: "HIT: watcher.rs".into(),
            }
        );
    }

    #[test]
    fn a_failing_tool_is_reported_to_the_model_rather_than_ending_the_run() {
        // Two prose turns: the first answer follows a FAILED call, so it is
        // ungrounded and earns the pushback; the second is what gets published.
        let chat = ScriptedChat::new(vec![
            calls(vec![call("c1", "search", "not json")], 10),
            prose("recovered", 10),
            prose("recovered", 10),
        ]);
        let tools = StubTools {
            answer: Err(crate::Error::Provider("address needs --span".into())),
        };

        let outcome = run(ask(&chat, &tools, AgentBounds::default(), "q"));

        assert_eq!(outcome.answer.as_deref(), Some("recovered"));
        assert!(outcome.trace[0].failed);
        let conversation = chat.last_conversation();
        let ChatMessage::ToolResult { content, .. } = &conversation[3] else {
            panic!("expected a tool result");
        };
        assert!(content.starts_with("ERROR: "), "got {content}");
    }

    #[test]
    fn a_model_that_only_ever_calls_tools_stops_at_the_iteration_bound() {
        let turns = (0..4)
            .map(|_| calls(vec![call("c", "search", "{}")], 1))
            .collect();
        let chat = ScriptedChat::new(turns);
        let bounds = AgentBounds {
            max_iterations: 4,
            ..AgentBounds::default()
        };

        let outcome = run(ask(&chat, &tools_ok("hit"), bounds, "q"));

        assert_eq!(outcome.stopped, StopReason::MaxIterations);
        assert_eq!(outcome.iterations, 4);
        // No answer is invented when the loop runs out of room.
        assert!(outcome.answer.is_none());
    }

    #[test]
    fn spending_the_token_budget_stops_the_run() {
        let chat = ScriptedChat::new(vec![calls(vec![call("c", "search", "{}")], 500)]);
        let bounds = AgentBounds {
            token_budget: 100,
            ..AgentBounds::default()
        };

        let outcome = run(ask(&chat, &tools_ok("hit"), bounds, "q"));

        assert_eq!(outcome.stopped, StopReason::TokenBudget);
        assert_eq!(outcome.tokens_used, Some(500));
        assert!(outcome.answer.is_none());
    }

    #[test]
    fn an_enormous_tool_result_is_truncated_before_the_model_sees_it() {
        let chat = ScriptedChat::new(vec![
            calls(vec![call("c", "get", "{}")], 10),
            prose("done", 10),
        ]);
        let bounds = AgentBounds {
            tool_result_max_chars: 20,
            ..AgentBounds::default()
        };

        let outcome = run(ask(&chat, &tools_ok(&"x".repeat(5_000)), bounds, "q"));

        let ChatMessage::ToolResult { content, .. } = &chat.last_conversation()[3] else {
            panic!("expected a tool result");
        };
        assert!(content.starts_with(&"x".repeat(20)));
        assert!(content.contains("truncated at 20 characters"));
        assert!(outcome.trace[0].result_chars < 100);
    }

    #[test]
    fn a_provider_that_reports_no_usage_is_not_treated_as_free() {
        let unreported = || ChatTurn {
            content: Some("answer".into()),
            tool_calls: vec![],
            tokens_used: None,
        };
        // Twice: answering with no tool calls earns one pushback before the
        // loop publishes it.
        let chat = ScriptedChat::new(vec![unreported(), unreported()]);

        let outcome = run(ask(&chat, &tools_ok("x"), AgentBounds::default(), "q"));

        // Unknown must stay unknown: reporting 0 would publish a number nobody
        // measured, and a budget assertion against it would pass having
        // measured nothing.
        assert_eq!(outcome.tokens_used, None);
        assert_eq!(outcome.stopped, StopReason::Answered);
    }

    #[test]
    fn the_system_prompt_demands_grounding_and_an_honest_not_found() {
        // The grounding bar is a product requirement, so it is asserted rather
        // than left to whoever next edits the prompt.
        assert!(SYSTEM_PROMPT.contains("Never answer from prior knowledge"));
        assert!(SYSTEM_PROMPT.contains("could not find it"));
        assert!(SYSTEM_PROMPT.contains("Cite the addresses"));
    }
}
