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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    /// Natural-language summary; embedded alongside the raw content.
    pub text: String,
    /// 1–5 tags naming the element's most important concepts.
    pub tags: Vec<String>,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_band_matches_prd_req_36() {
        let with = |n: usize| Summary {
            text: "s".into(),
            tags: vec!["t".to_string(); n],
        };
        assert!(!with(0).has_valid_tags());
        assert!(with(1).has_valid_tags());
        assert!(with(5).has_valid_tags());
        assert!(!with(6).has_valid_tags());
    }
}
