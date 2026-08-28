//! The three ports in fs3.
//!
//! Workshop 001 rule 3: a trait earns its existence only when a second real
//! implementation exists or is firmly planned. Embedding and summarisation each
//! have two — online API and local model (PRD req 8) — so each gets a port.
//! Everything else is concrete: the parser (tree-sitter direct *is* the point),
//! git ops, the queue, and the store (Postgres is a requirement, not a
//! variable). **A fourth port is stop-and-ask.**
//!
//! The third — [`ChatProvider`] — was asked for and granted on 2026-08-28. It
//! is not a second way to summarise: the agentic `ask` verb needs a model that
//! takes a CONVERSATION and may answer with a TOOL CALL rather than prose, and
//! it is routinely a different (larger, pricier) deployment from the one doing
//! bulk enrichment. Without a port that choice cannot be expressed in config at
//! all, and the two real implementations already exist — a hosted chat
//! deployment and the offline fake.
//!
//! Both traits are `#[async_trait]` rather than native `async fn`: native async
//! fns in traits are still not object-safe, and these seams are used as
//! `Arc<dyn Port>` by the composition root.

use async_trait::async_trait;

use crate::element::Element;
use crate::error::Result;

/// An LLM summary of one element, plus its concept tags (PRD req 36).
///
/// `text` and `tags` are the typed contract and do not move. Everything a
/// future prompt learns to extract arrives in [`Summary::extras`] first and is
/// promoted to a typed field only once it has earned one — so a provider can
/// start returning a new field today without a core change, a migration, or a
/// coordinated release.
//
// `Eq` is deliberately absent: `serde_json::Value` holds floats, so `extras`
// cannot be `Eq`. Nothing puts a `Summary` in a hash set, and a summary is
// content rather than an identity, so the derive was never load-bearing.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    /// Natural-language summary; embedded alongside the raw content.
    pub text: String,
    /// 1–5 tags naming the element's most important concepts.
    pub tags: Vec<String>,
    /// Fields beyond the typed contract, captured rather than discarded.
    ///
    /// `#[serde(flatten)]` is what makes this real at runtime: any JSON member
    /// the provider returns that is not `text` or `tags` lands here instead of
    /// being silently dropped, so "new fields land in extras first" is a
    /// property of the wire format and not a convention to remember.
    #[serde(
        flatten,
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extras: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Summary {
    /// The tag-count band PRD req 36 mandates.
    pub const TAG_RANGE: std::ops::RangeInclusive<usize> = 1..=5;

    /// Whether this summary honours the mandated tag band.
    pub fn has_valid_tags(&self) -> bool {
        Self::TAG_RANGE.contains(&self.tags.len())
    }
}

/// Turns text into vectors. Online API or local model, chosen by config.
///
/// Object-safe by construction — the composition root stores it as
/// `Arc<dyn Embedder>`:
///
/// ```
/// use std::sync::Arc;
/// use fs3_core::Embedder;
///
/// fn takes_a_port(_embedder: Arc<dyn Embedder>) {}
/// ```
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed a batch of texts, returning one vector per input in input order.
    ///
    /// # Errors
    /// [`crate::Error::Provider`] when the backing model or API fails.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// The enrichment row key for whatever this embedder produces:
    /// `model@dimensions`.
    ///
    /// Enrichment rows are keyed by this string, so a change of model — or of
    /// width — is never a migration: the new key writes new rows, the
    /// reconciler re-enriches, and the old rows survive for rollback. The
    /// width belongs in the key because it changes the *vector space*: the
    /// same model at 1024 and at 1536 produces vectors that must never be
    /// compared, and nothing else about an embedder can invalidate a stored
    /// vector so quietly.
    ///
    /// The provider owns this rather than the consumer because only the
    /// provider knows what actually served the request — on Azure that is a
    /// deployment name, which no amount of config-reading will reveal.
    fn key(&self) -> String;

    /// The most requests this provider will tolerate in flight at once.
    ///
    /// A **declaration**, not a limiter: nothing here counts anything. The
    /// scheduler owns the semaphore, because only the scheduler can see the
    /// queue — a provider handed one request cannot know how many others are
    /// in flight. What a provider does know is its own shape, and that is what
    /// this reports: a cloud endpoint sized by quota can take many, a LAN box
    /// serving one model on one GPU can take exactly one, and an in-process
    /// model behind a mutex can take exactly one no matter what anyone wishes.
    ///
    /// The intended use is `min(lane_width, provider.concurrency_ceiling())`.
    ///
    /// Deliberately **required**, with no default. A default is a number
    /// nobody chose, and both ways of being wrong are silent: too high thrashes
    /// a small box, too low drives a cloud provider at a fraction of its
    /// capacity, and neither surfaces as an error — only as throughput that
    /// nobody can explain.
    fn concurrency_ceiling(&self) -> usize;

    /// The most tokens this provider accepts in ONE input before it REJECTS
    /// the call.
    ///
    /// A **declaration**, like [`Embedder::concurrency_ceiling`], and for the
    /// same reason: only the provider knows which model is actually deployed
    /// behind it. On Azure that is a deployment name that reveals nothing from
    /// config.
    ///
    /// This is the cap on a SINGLE text, not on a request. A batching caller
    /// already limits the sum; this limits the largest member, and the two
    /// failure modes are different — an over-budget request can be split,
    /// while an over-cap input cannot be split by any amount of batching and
    /// is rejected forever. Azure answers such an input with
    /// `400 Invalid 'input[0]': maximum input length is 8192 tokens`, which
    /// retries reproduce exactly, so an element bigger than the cap stays
    /// unvectorised until somebody shortens it.
    ///
    /// Callers are expected to truncate to fit rather than skip: a vector of a
    /// long element's prefix is worth far more than no vector at all.
    ///
    /// A provider that TRUNCATES an oversized input instead of rejecting it
    /// declares [`usize::MAX`] and says so in its own documentation. The
    /// distinction is the whole point of the method: there is nothing for a
    /// caller to prevent, and a smaller number would only make the caller cut
    /// earlier than the model itself would, throwing away content for no gain.
    ///
    /// Deliberately **required**, with no default, for the reason above it: a
    /// default is a number nobody chose, and being wrong is silent in both
    /// directions — too high fails every oversized input, too low throws away
    /// content the model would happily have read.
    fn max_input_tokens(&self) -> usize;
}

/// Summarises an element into text plus concept tags.
///
/// ```
/// use std::sync::Arc;
/// use fs3_core::Summarizer;
///
/// fn takes_a_port(_summarizer: Arc<dyn Summarizer>) {}
/// ```
#[async_trait]
pub trait Summarizer: Send + Sync {
    /// Summarise one element. Returns summary text + 1–5 concept tags.
    ///
    /// # Errors
    /// [`crate::Error::Provider`] when the backing model or API fails.
    async fn summarize(&self, element: &Element) -> Result<Summary>;

    /// The enrichment row key for whatever this summarizer produces:
    /// `model@prompt_version`.
    ///
    /// The prompt is part of the key because it is part of the output: a
    /// reworded instruction or a changed response schema produces different
    /// summaries from the same model, and those must not be mistaken for the
    /// old ones. Keying them apart turns every prompt change into new rows the
    /// reconciler fills, instead of a migration that destroys the evidence of
    /// what the previous prompt said.
    fn key(&self) -> String;

    /// The most requests this provider will tolerate in flight at once.
    ///
    /// A **declaration**, not a limiter: nothing here counts anything. The
    /// scheduler owns the semaphore, because only the scheduler can see the
    /// queue — a provider handed one request cannot know how many others are
    /// in flight. What a provider does know is its own shape, and that is what
    /// this reports: a cloud endpoint sized by quota can take many, a LAN box
    /// serving one model on one GPU can take exactly one, and an in-process
    /// model behind a mutex can take exactly one no matter what anyone wishes.
    ///
    /// The intended use is `min(lane_width, provider.concurrency_ceiling())`.
    ///
    /// Deliberately **required**, with no default. A default is a number
    /// nobody chose, and both ways of being wrong are silent: too high thrashes
    /// a small box, too low drives a cloud provider at a fraction of its
    /// capacity, and neither surfaces as an error — only as throughput that
    /// nobody can explain.
    fn concurrency_ceiling(&self) -> usize;

    /// The most tokens this model accepts in the PROMPT of one call.
    ///
    /// The same declaration [`Embedder::max_input_tokens`] makes, against a
    /// much larger number: a chat model's context is measured in tens or
    /// hundreds of thousands of tokens rather than thousands. It is a cliff
    /// all the same. A generated file, a vendored bundle or a data table that
    /// tree-sitter hands back as ONE element can be hundreds of kilobytes, and
    /// a prompt built around it is rejected on arrival, retried identically,
    /// and fails for good.
    ///
    /// This is the budget for everything the caller sends, so a caller must
    /// leave room for its own instructions and for the reply: the element body
    /// is the part that is truncated, because it is the only part whose size
    /// the caller does not control.
    ///
    /// Required, with no default, for the reason
    /// [`Summarizer::concurrency_ceiling`] gives.
    fn max_input_tokens(&self) -> usize;
}

/// One message in a chat exchange.
///
/// Deliberately provider-neutral. Core cannot see a wire type — it performs no
/// IO — so the loop speaks this shape and an adapter translates it at the edge.
/// The four roles are the whole protocol a tool loop needs: an instruction, the
/// user's question, what the model said (possibly a tool call), and what a tool
/// answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatMessage {
    /// The standing instruction the loop opens with.
    System(String),
    /// The question being answered.
    User(String),
    /// What the model said last turn. Carries `tool_calls` because the protocol
    /// requires the assistant's own request to be replayed back to it verbatim
    /// alongside the results; dropping it makes the next turn incoherent.
    Assistant {
        /// Prose, absent when the model replied with tool calls alone.
        content: Option<String>,
        /// The calls the model asked for, empty when it answered in prose.
        tool_calls: Vec<ToolCall>,
    },
    /// The result of one tool call, tied back to it by id.
    ToolResult {
        /// The [`ToolCall::id`] this answers.
        tool_call_id: String,
        /// What the tool produced — or an error message, which is a normal
        /// result here rather than a failure. See [`ChatTurn`].
        content: String,
    },
}

/// One tool invocation the model asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    /// Correlates this call with its [`ChatMessage::ToolResult`].
    pub id: String,
    /// Which tool: matched against the offered [`ToolSchema::name`]s.
    pub name: String,
    /// The arguments, as the JSON *string* the model emitted.
    ///
    /// Kept unparsed on purpose. A model can and does emit malformed JSON, and
    /// that is not an error the loop should die on — it is a fact to hand back
    /// so the model can correct itself. Parsing here would turn a recoverable
    /// turn into a crash.
    pub arguments: String,
}

/// A tool offered to the model: its name, what it is for, and its argument
/// schema as JSON Schema.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSchema {
    /// The name the model will call back with.
    pub name: String,
    /// What the tool does. This is prompt text — the model chooses tools by
    /// reading it, so vagueness here shows up as bad tool selection.
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: serde_json::Value,
}

/// What one turn of the model produced.
///
/// Either prose or tool calls — the loop ends on the first, and continues on
/// the second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTurn {
    /// The model's prose, absent when it only asked for tools.
    pub content: Option<String>,
    /// The tools it wants run. Empty means this turn is the answer.
    pub tool_calls: Vec<ToolCall>,
    /// Tokens this turn cost, when the provider reports them.
    ///
    /// `None` is honest ignorance, not zero: a provider that reports nothing
    /// must not read as free, or a token budget silently stops bounding.
    pub tokens_used: Option<u64>,
}

/// A chat model that can be given tools and may answer by calling them.
///
/// The third port (see the module docs). It is not a second [`Summarizer`]:
/// that one turns ONE element into a summary and never converses, while this
/// carries a growing message list and may reply with a tool call instead of
/// prose. The two are routinely different deployments — a cheap model enriches
/// in bulk, a stronger one answers questions — and a port is how config says so.
///
/// ```
/// use std::sync::Arc;
/// use fs3_core::ChatProvider;
///
/// fn takes_a_port(_chat: Arc<dyn ChatProvider>) {}
/// ```
#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// Take one turn: send the conversation and the offered tools, get back
    /// prose or tool calls.
    ///
    /// # Errors
    /// [`crate::Error::Provider`] when the backing model or API fails. A
    /// malformed tool call from the model is NOT an error — it comes back as a
    /// [`ChatTurn`] for the loop to answer.
    async fn turn(&self, messages: &[ChatMessage], tools: &[ToolSchema]) -> Result<ChatTurn>;

    /// Which model answered, for the trace and for support questions.
    fn key(&self) -> String;

    /// The most tokens this model accepts in the PROMPT of one call.
    ///
    /// The same declaration [`Summarizer::max_input_tokens`] makes, and it
    /// bites harder here: a tool loop GROWS its prompt every turn, so the
    /// caller must know the ceiling it is walking towards.
    fn max_input_tokens(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_band_matches_prd_req_36() {
        let with = |n: usize| Summary {
            text: "s".into(),
            tags: vec!["t".to_string(); n],
            ..Summary::default()
        };
        assert!(!with(0).has_valid_tags());
        assert!(with(1).has_valid_tags());
        assert!(with(5).has_valid_tags());
        assert!(!with(6).has_valid_tags());
    }

    /// The point of `extras`: a field the typed contract has never heard of
    /// survives the boundary instead of being dropped on the floor.
    #[test]
    fn an_unknown_field_lands_in_extras_rather_than_being_discarded() {
        let summary: Summary =
            serde_json::from_str(r#"{"text":"t","tags":["a"],"complexity":7,"risk":"low"}"#)
                .expect("unknown members are captured, not rejected");

        assert_eq!(summary.text, "t");
        assert_eq!(summary.tags, ["a"]);
        assert_eq!(summary.extras["complexity"], serde_json::json!(7));
        assert_eq!(summary.extras["risk"], serde_json::json!("low"));
    }

    /// Extras round-trip at the top level, so a promoted field reads back the
    /// same whether it was typed when it was written or not.
    #[test]
    fn extras_round_trip_flattened() {
        let mut summary = Summary {
            text: "t".into(),
            tags: vec!["a".into()],
            ..Summary::default()
        };
        summary
            .extras
            .insert("complexity".into(), serde_json::json!(7));

        let json = serde_json::to_string(&summary).expect("serialisable");
        assert_eq!(json, r#"{"text":"t","tags":["a"],"complexity":7}"#);
        assert_eq!(
            serde_json::from_str::<Summary>(&json).expect("round-trips"),
            summary
        );
    }

    /// An empty map must not add a key, or every stored summary grows a
    /// meaningless `"extras":{}` the day this landed.
    #[test]
    fn empty_extras_serialise_to_nothing() {
        let summary = Summary {
            text: "t".into(),
            tags: vec!["a".into()],
            ..Summary::default()
        };
        assert_eq!(
            serde_json::to_string(&summary).expect("serialisable"),
            r#"{"text":"t","tags":["a"]}"#
        );
    }
}
