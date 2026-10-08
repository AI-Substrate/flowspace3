//! What a stored turn IS, so search can tell work from retrieval traffic.
//!
//! Measured on the live index (2026-10-09): asking "which agent compared LLM
//! pricing" returned twenty hits, every one of them an echo — agents' own
//! `flowspace3 search` calls and the envelopes those calls printed, each a
//! near-verbatim copy of the query and so scored 1.0. Each failed lookup became
//! the top answer for the next agent to ask. The original work sat below them.
//!
//! The fix is structural, not textual: a turn that consists only of an fs3
//! retrieval call, or only of the result one printed, carries no work — it is
//! the index looking at itself. Those two classes are what search hides by
//! default. Every other tool turn is labelled too, so a caller can filter on
//! it later, but labelled is not hidden.
//!
//! Two rules keep this from hiding anything that matters:
//!
//! * **Prose is never hidden.** A turn with its own words is [`TurnClass::Work`]
//!   whatever tools it also called. A model's conclusion stated beside a search
//!   call is still a conclusion.
//! * **Per turn, never per session.** An agent's own conversation stays
//!   searchable — after compaction fs3 may hold the only copy of its earlier
//!   turns (ruled 2026-10-09).

use serde::{Deserialize, Serialize};

use crate::conversation::{ToolInput, Turn, TurnItem};
use crate::error::{Error, Result};

/// The read-only fs3 verbs whose output is the index's own content played back.
///
/// `status`, `doctor` and `conversation list` are deliberately absent: their
/// output describes the system rather than repeating indexed content, so it is
/// not an echo.
const RETRIEVAL_VERBS: &[&str] = &[
    "search",
    "get",
    "ask",
    "refs",
    "tree",
    "docs",
    "agents-start-here",
];

/// The binary a retrieval call invokes.
const BINARY: &str = "flowspace3";

/// How far into a result head the envelope's `command` field may sit.
///
/// The envelope prints `ok`, `command` and `v` first; anything later is a
/// document that merely mentions a command.
const ENVELOPE_PREFIX: usize = 256;

/// What a stored turn is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnClass {
    /// Prose, or anything not recognised below. Never hidden.
    Work,
    /// Only tool calls, none of them fs3 retrieval.
    ToolCall,
    /// Only tool results, not from fs3 retrieval.
    ToolResult,
    /// Only fs3 retrieval calls (`search`, `get`, `ask`, …).
    RetrievalCall,
    /// Only the output of fs3 retrieval calls.
    RetrievalResult,
}

impl TurnClass {
    /// The classes search leaves out unless the caller asks for them.
    pub const HIDDEN_BY_DEFAULT: &'static [TurnClass] =
        &[TurnClass::RetrievalCall, TurnClass::RetrievalResult];

    /// Every class, in a stable order.
    pub const ALL: &'static [TurnClass] = &[
        TurnClass::Work,
        TurnClass::ToolCall,
        TurnClass::ToolResult,
        TurnClass::RetrievalCall,
        TurnClass::RetrievalResult,
    ];

    /// The stable wire/storage spelling. Matches the serde representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TurnClass::Work => "work",
            TurnClass::ToolCall => "tool_call",
            TurnClass::ToolResult => "tool_result",
            TurnClass::RetrievalCall => "retrieval_call",
            TurnClass::RetrievalResult => "retrieval_result",
        }
    }

    /// Whether search leaves this class out by default.
    #[must_use]
    pub fn hidden_by_default(self) -> bool {
        Self::HIDDEN_BY_DEFAULT.contains(&self)
    }
}

impl std::str::FromStr for TurnClass {
    type Err = Error;

    /// Read a stored spelling back.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] naming the value, for a row that is not one of
    /// the spellings the column's check constraint allows.
    fn from_str(value: &str) -> Result<Self> {
        TurnClass::ALL
            .iter()
            .copied()
            .find(|class| class.as_str() == value)
            .ok_or_else(|| Error::InvalidConfig(format!("unknown turn class {value:?}")))
    }
}

impl std::fmt::Display for TurnClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classifies the turns of ONE conversation, in order.
///
/// Stateful because a result does not say what produced it: a harness may store
/// the call and its output as separate turns, and the output of
/// `flowspace3 search … | python3 summarise.py` is not an envelope. The
/// classifier remembers whether the most recent calls were all retrieval and
/// pairs the results that follow with them.
#[derive(Clone, Debug, Default)]
pub struct TurnClassifier {
    /// The latest turn that made calls made only retrieval calls, and nothing
    /// but results has come since.
    retrieval_pending: bool,
}

impl TurnClassifier {
    /// A classifier at the start of a conversation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A classifier positioned after `previous`, for an append that starts
    /// mid-conversation. `None` is the start of the conversation.
    #[must_use]
    pub fn after(previous: Option<&Turn>) -> Self {
        let mut classifier = Self::new();
        if let Some(turn) = previous {
            classifier.classify(turn);
        }
        classifier
    }

    /// Classify the next turn and advance.
    pub fn classify(&mut self, turn: &Turn) -> TurnClass {
        let calls: Vec<bool> = turn
            .items
            .iter()
            .filter_map(|item| match item {
                TurnItem::ToolCall { tool, input } => Some(is_retrieval_call(tool, input)),
                TurnItem::ToolResult { .. } => None,
            })
            .collect();
        let results: Vec<bool> = turn
            .items
            .iter()
            .filter_map(|item| match item {
                TurnItem::ToolResult { head, .. } => {
                    Some(self.retrieval_pending || is_retrieval_envelope(head))
                }
                TurnItem::ToolCall { .. } => None,
            })
            .collect();

        let class = if has_prose(turn) || (calls.is_empty() && results.is_empty()) {
            TurnClass::Work
        } else if !calls.is_empty() {
            if calls.iter().all(|&retrieval| retrieval)
                && results.iter().all(|&retrieval| retrieval)
            {
                TurnClass::RetrievalCall
            } else {
                TurnClass::ToolCall
            }
        } else if results.iter().all(|&retrieval| retrieval) {
            TurnClass::RetrievalResult
        } else {
            TurnClass::ToolResult
        };

        // A turn that calls tools resets the pairing; a results-only turn keeps
        // it, because two calls can come back as two result turns; anything
        // else — prose, a person — ends it.
        self.retrieval_pending = if calls.is_empty() {
            self.retrieval_pending && !results.is_empty() && !has_prose(turn)
        } else {
            calls.iter().all(|&retrieval| retrieval)
        };

        class
    }
}

/// Whether the turn says something in its own words.
///
/// Some harnesses copy a tool result's head into the turn body. That copy is
/// not prose, so a body that merely repeats a result head does not count.
fn has_prose(turn: &Turn) -> bool {
    let body = turn.body.trim();
    if body.is_empty() {
        return false;
    }
    !turn.items.iter().any(|item| match item {
        TurnItem::ToolResult { head, .. } => {
            let head = head.trim();
            !head.is_empty() && (head.starts_with(body) || body.starts_with(head))
        }
        TurnItem::ToolCall { .. } => false,
    })
}

/// Whether a tool call asks fs3 for indexed content.
fn is_retrieval_call(tool: &str, input: &ToolInput) -> bool {
    if tool.to_ascii_lowercase().contains("flowspace") {
        return true;
    }
    match input {
        ToolInput::Verbatim { text } => invokes_retrieval(text),
        ToolInput::Elided { .. } => false,
    }
}

/// Whether a command line runs `flowspace3 <retrieval verb>`.
///
/// The binary must sit at a command boundary (start, whitespace, quote, path
/// separator or shell operator) so a word like `myflowspace3` does not match,
/// and global flags may come between it and the verb.
fn invokes_retrieval(text: &str) -> bool {
    let mut rest = text;
    while let Some(at) = rest.find(BINARY) {
        let before = rest[..at].chars().next_back();
        let after = &rest[at + BINARY.len()..];
        rest = after;
        let boundary = before.is_none_or(|c| {
            c.is_whitespace() || matches!(c, '"' | '\'' | '/' | ';' | '|' | '&' | '(' | '`' | '=')
        });
        if !boundary || !after.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let verb = after
            .split_whitespace()
            .map(|word| word.trim_matches(|c| matches!(c, '"' | '\'' | '\\')))
            .find(|word| !word.starts_with('-'));
        if verb.is_some_and(|verb| RETRIEVAL_VERBS.contains(&verb)) {
            return true;
        }
    }
    false
}

/// Whether a result head is an fs3 envelope from a retrieval verb.
fn is_retrieval_envelope(head: &str) -> bool {
    let head = head.trim_start();
    if !head.starts_with('{') {
        return false;
    }
    let window = &head[..floor_char_boundary(head, ENVELOPE_PREFIX)];
    if !window.contains("\"ok\"") {
        return false;
    }
    let Some(at) = window.find("\"command\"") else {
        return false;
    };
    let value = window[at + "\"command\"".len()..]
        .trim_start()
        .strip_prefix(':')
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('"'))
        .and_then(|rest| rest.split('"').next());
    value.is_some_and(|command| RETRIEVAL_VERBS.contains(&command))
}

/// The largest char boundary at or below `index`.
fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    (0..=index)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{TurnRole, TurnSource};

    fn turn(role: TurnRole, body: &str, items: Vec<TurnItem>) -> Turn {
        Turn {
            turn_no: 1,
            role,
            source: TurnSource::System,
            head_sha: None,
            at: "2026-10-09T00:00:00Z".to_string(),
            body: body.to_string(),
            items,
        }
    }

    fn call(tool: &str, text: &str) -> TurnItem {
        TurnItem::ToolCall {
            tool: tool.to_string(),
            input: ToolInput::Verbatim {
                text: text.to_string(),
            },
        }
    }

    fn result(tool: &str, head: &str) -> TurnItem {
        TurnItem::ToolResult {
            tool: tool.to_string(),
            head: head.to_string(),
            total_bytes: head.len() as u64,
            truncated: false,
        }
    }

    const SEARCH_ENVELOPE: &str = "{\n  \"ok\": true,\n  \"command\": \"search\",\n  \"v\": 1,\n  \"data\": {\"results\": []}}";

    #[test]
    fn a_search_call_and_its_envelope_are_retrieval() {
        let mut classifier = TurnClassifier::new();
        let asked = turn(
            TurnRole::Agent,
            "",
            vec![call(
                "Bash",
                r#"{"command":"flowspace3 search \"where is retry\" --json"}"#,
            )],
        );
        let answered = turn(TurnRole::Human, "", vec![result("Bash", SEARCH_ENVELOPE)]);
        assert_eq!(classifier.classify(&asked), TurnClass::RetrievalCall);
        assert_eq!(classifier.classify(&answered), TurnClass::RetrievalResult);
    }

    #[test]
    fn a_piped_search_result_pairs_with_its_call() {
        // `| python3 summarise.py` output is no envelope; the pairing carries it.
        let mut classifier = TurnClassifier::new();
        let asked = turn(
            TurnRole::Agent,
            "",
            vec![call(
                "Bash",
                "flowspace3 search \"pricing\" --repo all --json | python3 -c 'print(1)'",
            )],
        );
        let answered = turn(
            TurnRole::Human,
            "",
            vec![result("Bash", "conv:abc t4 0.98")],
        );
        classifier.classify(&asked);
        assert_eq!(classifier.classify(&answered), TurnClass::RetrievalResult);
    }

    #[test]
    fn two_calls_can_come_back_as_two_result_turns() {
        let mut classifier = TurnClassifier::new();
        let asked = turn(
            TurnRole::Agent,
            "",
            vec![
                call("bash", "flowspace3 get 'conv:1#t3' --before 0"),
                call("bash", "FS3_OUTPUT=json flowspace3 search \"x\""),
            ],
        );
        classifier.classify(&asked);
        for _ in 0..2 {
            let answered = turn(
                TurnRole::Agent,
                "plain text",
                vec![result("bash", "plain text")],
            );
            assert_eq!(classifier.classify(&answered), TurnClass::RetrievalResult);
        }
    }

    #[test]
    fn prose_is_never_hidden_even_beside_a_search() {
        let mut classifier = TurnClassifier::new();
        let found = turn(
            TurnRole::Agent,
            "Found it: the retry policy lives in the runner.",
            vec![call("Bash", "flowspace3 search \"retry\"")],
        );
        assert_eq!(classifier.classify(&found), TurnClass::Work);
        // …and the result that follows is still paired with the search.
        let answered = turn(TurnRole::Human, "", vec![result("Bash", "anything")]);
        assert_eq!(classifier.classify(&answered), TurnClass::RetrievalResult);
    }

    #[test]
    fn a_body_that_only_copies_the_result_head_is_not_prose() {
        let mut classifier = TurnClassifier::new();
        let copied = turn(
            TurnRole::Agent,
            SEARCH_ENVELOPE,
            vec![result("bash", SEARCH_ENVELOPE)],
        );
        assert_eq!(classifier.classify(&copied), TurnClass::RetrievalResult);
    }

    #[test]
    fn other_tools_are_labelled_but_not_retrieval() {
        let mut classifier = TurnClassifier::new();
        let build = turn(TurnRole::Agent, "", vec![call("Bash", "cargo test --all")]);
        let output = turn(TurnRole::Human, "", vec![result("Bash", "test result: ok")]);
        assert_eq!(classifier.classify(&build), TurnClass::ToolCall);
        assert_eq!(classifier.classify(&output), TurnClass::ToolResult);
        assert!(!TurnClass::ToolCall.hidden_by_default());
        assert!(!TurnClass::ToolResult.hidden_by_default());
    }

    #[test]
    fn a_mixed_batch_of_calls_is_ordinary_tool_work() {
        let mut classifier = TurnClassifier::new();
        let mixed = turn(
            TurnRole::Agent,
            "",
            vec![
                call("Bash", "flowspace3 search \"x\""),
                call("Bash", "cargo build"),
            ],
        );
        assert_eq!(classifier.classify(&mixed), TurnClass::ToolCall);
        let output = turn(TurnRole::Human, "", vec![result("Bash", "Finished")]);
        assert_eq!(classifier.classify(&output), TurnClass::ToolResult);
    }

    #[test]
    fn status_and_doctor_are_not_echoes() {
        let mut classifier = TurnClassifier::new();
        let status = turn(
            TurnRole::Agent,
            "",
            vec![call("Bash", "flowspace3 status --json")],
        );
        assert_eq!(classifier.classify(&status), TurnClass::ToolCall);
        let doctor = "{\"ok\": true, \"command\": \"doctor\", \"v\": 1}";
        let output = turn(TurnRole::Human, "", vec![result("Bash", doctor)]);
        assert_eq!(classifier.classify(&output), TurnClass::ToolResult);
    }

    #[test]
    fn an_envelope_is_recognised_without_its_call() {
        // A Read of an artifact holding fs3 output: the call is not retrieval,
        // the content is.
        let mut classifier = TurnClassifier::new();
        classifier.classify(&turn(
            TurnRole::Agent,
            "",
            vec![call("read", "artifact://11:raw")],
        ));
        let docs = "{ \"ok\": true, \"command\": \"docs\", \"v\": 1, \"data\": {}}";
        let output = turn(TurnRole::Agent, "", vec![result("read", docs)]);
        assert_eq!(classifier.classify(&output), TurnClass::RetrievalResult);
    }

    #[test]
    fn the_binary_must_stand_at_a_command_boundary() {
        assert!(invokes_retrieval("/usr/local/bin/flowspace3 search x"));
        assert!(invokes_retrieval("cd /tmp && flowspace3 --json get conv:1"));
        assert!(invokes_retrieval(
            "FS3_OUTPUT=json flowspace3 agents-start-here"
        ));
        assert!(!invokes_retrieval("myflowspace3 search x"));
        assert!(!invokes_retrieval("flowspace3 status"));
        assert!(!invokes_retrieval("cargo build -p flowspace3"));
        assert!(!invokes_retrieval("grep -rn search crates/flowspace3-cli"));
    }

    #[test]
    fn mcp_style_tool_names_count_as_retrieval() {
        let mut classifier = TurnClassifier::new();
        let asked = turn(
            TurnRole::Agent,
            "",
            vec![call("mcp__flowspace__search", "{}")],
        );
        assert_eq!(classifier.classify(&asked), TurnClass::RetrievalCall);
    }

    #[test]
    fn a_person_ends_the_pairing() {
        let mut classifier = TurnClassifier::new();
        classifier.classify(&turn(
            TurnRole::Agent,
            "",
            vec![call("Bash", "flowspace3 search x")],
        ));
        classifier.classify(&turn(TurnRole::Human, "thanks, now build it", vec![]));
        let output = turn(TurnRole::Human, "", vec![result("Bash", "Compiling")]);
        assert_eq!(classifier.classify(&output), TurnClass::ToolResult);
    }

    #[test]
    fn resuming_mid_conversation_restores_the_pairing() {
        let asked = turn(
            TurnRole::Agent,
            "",
            vec![call("Bash", "flowspace3 search x")],
        );
        let mut classifier = TurnClassifier::after(Some(&asked));
        let output = turn(TurnRole::Human, "", vec![result("Bash", "rows")]);
        assert_eq!(classifier.classify(&output), TurnClass::RetrievalResult);
    }

    #[test]
    fn spellings_round_trip() {
        for class in TurnClass::ALL {
            assert_eq!(class.as_str().parse::<TurnClass>().unwrap(), *class);
        }
        assert!("derived".parse::<TurnClass>().is_err());
    }

    #[test]
    fn a_multibyte_head_does_not_panic() {
        let head = format!(
            "{{\"ok\": true, \"command\": \"search\" {}",
            "é".repeat(300)
        );
        assert!(is_retrieval_envelope(&head));
    }
}
