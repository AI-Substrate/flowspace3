//! The only two ports in fs3 v1.
//!
//! Workshop 001 rule 3: a trait earns its existence only when a second real
//! implementation exists or is firmly planned. Embedding and summarisation each
//! have two — online API and local model (PRD req 8) — so each gets a port.
//! Everything else is concrete: the parser (tree-sitter direct *is* the point),
//! git ops, the queue, and the store (Postgres is a requirement, not a
//! variable). **A third port is stop-and-ask.**
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
