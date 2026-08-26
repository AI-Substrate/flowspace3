//! Deterministic fakes for both ports.

use std::sync::Mutex;

use async_trait::async_trait;
use fs3_core::{ADDRESS_SEGMENT, Element, Embedder, Error, Result, Summarizer, Summary};

/// Vector width the fakes produce. Wide enough that unrelated texts do not
/// collide in every bucket, narrow enough to eyeball in a failing assertion.
pub const FAKE_DIMENSIONS: usize = 32;

/// Hash text into a unit vector whose direction is carried by its *tokens*.
///
/// This is signed feature hashing (the "hashing trick"): every token is hashed
/// to one bucket and a sign, and buckets accumulate. Texts that share tokens
/// therefore share components, so cosine similarity ranks related text above
/// unrelated text — which is the only reason a fake embedder is worth having
/// at all (workshop 001: the fakes must make similarity search meaningful).
///
/// The previous version hashed the *whole string* independently per dimension.
/// That was deterministic, but it carried no shared signal whatsoever: two
/// snippets differing by one token were as far apart as either was from
/// unrelated prose, so any similarity assertion built on it was vacuous.
fn hash_vector(text: &str, dimensions: usize) -> Vec<f32> {
    let mut raw = vec![0.0f32; dimensions];

    for token in tokens(text) {
        let hash = fnv1a(token.as_bytes());
        let bucket = (hash % dimensions as u64) as usize;
        // The sign comes from an independent bit, so unrelated tokens landing
        // in one bucket cancel on average instead of always reinforcing.
        let sign = if hash & (1 << 63) == 0 { 1.0 } else { -1.0 };
        raw[bucket] += sign;
    }

    // Punctuation-only input, or a full cancellation, would leave an all-zero
    // vector — which the port contract forbids. Fall back to the whole string.
    if raw.iter().all(|value| *value == 0.0) {
        let hash = fnv1a(text.as_bytes());
        raw[(hash % dimensions as u64) as usize] = 1.0;
    }

    let norm = raw.iter().map(|v| v * v).sum::<f32>().sqrt();
    raw.into_iter().map(|v| v / norm).collect()
}

/// Split into lowercase alphanumeric-or-underscore tokens: the identifier-ish
/// and word-ish units that two related snippets actually have in common.
fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
}

/// FNV-1a. Not cryptographic — just stable across runs and platforms, which is
/// all that determinism needs.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// An [`Embedder`] that needs no network, no keys, and no model.
///
/// Records every batch it was handed, and can be told to start failing after
/// `n` successful calls so retry/backoff paths are exercisable without a
/// mocking framework.
#[derive(Debug)]
pub struct FakeEmbedder {
    /// Every batch passed to [`Embedder::embed`], in call order.
    pub calls: Mutex<Vec<Vec<String>>>,
    /// Fail every call after this many successful ones. `None` never fails.
    pub fail_after: Option<usize>,
    /// Width of the vectors produced.
    pub dimensions: usize,
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail_after: None,
            dimensions: FAKE_DIMENSIONS,
        }
    }
}

impl FakeEmbedder {
    /// A fake that starts failing after `successes` successful calls.
    pub fn failing_after(successes: usize) -> Self {
        Self {
            fail_after: Some(successes),
            ..Self::default()
        }
    }

    /// How many times [`Embedder::embed`] has been called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("fake embedder lock").len()
    }
}

#[async_trait]
impl Embedder for FakeEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let call_index = {
            let mut calls = self.calls.lock().expect("fake embedder lock");
            calls.push(texts.to_vec());
            calls.len() - 1
        };
        if self.fail_after.is_some_and(|limit| call_index >= limit) {
            return Err(Error::Provider(format!(
                "FakeEmbedder: injected failure on call {}",
                call_index + 1
            )));
        }
        Ok(texts
            .iter()
            .map(|text| hash_vector(text, self.dimensions))
            .collect())
    }

    /// `fake@<width>` — the same `model@dimensions` shape a real embedder
    /// keys by, so a store keyed on the fake's output looks like a store keyed
    /// on a real one.
    fn key(&self) -> String {
        format!("fake@{}", self.dimensions)
    }

    /// Unbounded in practice — it is a hash function. `usize::MAX` would be
    /// posturing, so this is simply a number no test will ever saturate.
    fn concurrency_ceiling(&self) -> usize {
        64
    }
}

/// A [`Summarizer`] that produces a stable, readable summary and tags derived
/// from the element's own address — no network, no keys, no model.
#[derive(Debug, Default)]
pub struct FakeSummarizer {
    /// Qualified names passed to [`Summarizer::summarize`], in call order.
    pub calls: Mutex<Vec<String>>,
    /// Fail every call after this many successful ones. `None` never fails.
    pub fail_after: Option<usize>,
}

impl FakeSummarizer {
    /// The fake's answer to a real adapter's `PROMPT_VERSION`. Bump it when
    /// the fake's text or tags change, for the same reason a real one bumps:
    /// rows written by the old shape must not be mistaken for the new.
    pub const PROMPT_VERSION: &'static str = "1";

    /// A fake that starts failing after `successes` successful calls.
    pub fn failing_after(successes: usize) -> Self {
        Self {
            fail_after: Some(successes),
            ..Self::default()
        }
    }

    /// How many times [`Summarizer::summarize`] has been called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("fake summarizer lock").len()
    }
}

#[async_trait]
impl Summarizer for FakeSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        let call_index = {
            let mut calls = self.calls.lock().expect("fake summarizer lock");
            calls.push(element.address.clone());
            calls.len() - 1
        };
        if self.fail_after.is_some_and(|limit| call_index >= limit) {
            return Err(Error::Provider(format!(
                "FakeSummarizer: injected failure on call {}",
                call_index + 1
            )));
        }

        // Tags from the element's own address: always 1–5, always the same for
        // the same element (PRD req 36's band, honoured by the fake too). The
        // first address segment is the file path, which is a location rather
        // than a concept, so the tags start after it.
        let mut tags = vec![element.kind.as_str().to_string()];
        tags.extend(
            element
                .address
                .split(ADDRESS_SEGMENT)
                .skip(1)
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(str::to_lowercase)
                .take(4),
        );

        let mut summary = Summary {
            text: format!(
                "{} `{}` at {} ({} lines)",
                element.kind,
                element.address,
                element.span,
                element.line_count()
            ),
            tags,
            ..Summary::default()
        };
        // Deterministic, and derived from the element rather than invented, so
        // CI exercises the extras shape on every run instead of only on the
        // day a provider first returns one.
        summary.extras.insert(
            "line_count".to_string(),
            serde_json::json!(element.line_count()),
        );
        Ok(summary)
    }

    /// `fake@<prompt version>` — the same `model@prompt_version` shape a real
    /// summarizer keys by. The version moves when the fake's text or tags do.
    fn key(&self) -> String {
        format!("fake@{}", Self::PROMPT_VERSION)
    }

    /// See [`FakeEmbedder::concurrency_ceiling`]: no network, no lock, no cost.
    fn concurrency_ceiling(&self) -> usize {
        64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_vectors_are_deterministic_and_normalised() {
        let a = hash_vector("fn main() {}", FAKE_DIMENSIONS);
        let b = hash_vector("fn main() {}", FAKE_DIMENSIONS);
        assert_eq!(a, b);

        let norm = a.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "expected a unit vector, got {norm}"
        );
    }

    #[test]
    fn different_text_gives_a_different_vector() {
        assert_ne!(
            hash_vector("alpha", FAKE_DIMENSIONS),
            hash_vector("beta", FAKE_DIMENSIONS)
        );
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    /// The property that makes the fake worth using: *related* text must rank
    /// above unrelated text. Proving only that different text differs is
    /// vacuous — a counter would pass it — and the whole-string hash this
    /// replaced passed it while ranking related and unrelated text identically.
    #[test]
    fn related_text_ranks_above_unrelated_text() {
        let query = hash_vector(
            "fn parse_markdown(path: &str) -> Vec<Element>",
            FAKE_DIMENSIONS,
        );
        let related = hash_vector(
            "fn parse_markdown(path: &str) -> Vec<Section>",
            FAKE_DIMENSIONS,
        );
        let unrelated = hash_vector(
            "SELECT id FROM users WHERE created_at > now()",
            FAKE_DIMENSIONS,
        );

        let near = cosine(&query, &related);
        let far = cosine(&query, &unrelated);
        assert!(
            near > far,
            "shared tokens must produce shared signal: related {near}, unrelated {far}"
        );
        assert!(
            near > 0.5,
            "text differing by one token should stay close, got {near}"
        );
    }

    /// Shared *tokens* are the signal, not shared byte prefixes.
    #[test]
    fn a_shared_token_is_the_unit_of_similarity() {
        let a = hash_vector("classify element kind", FAKE_DIMENSIONS);
        let shared = hash_vector("kind of element to classify", FAKE_DIMENSIONS);
        let disjoint = hash_vector("network socket timeout", FAKE_DIMENSIONS);
        assert!(
            cosine(&a, &shared) > cosine(&a, &disjoint),
            "word order must not destroy similarity"
        );
    }

    /// The contract forbids all-zero vectors; punctuation has no tokens.
    #[test]
    fn tokenless_input_still_yields_a_unit_vector() {
        for text in ["", "   ", "{}();"] {
            let vector = hash_vector(text, FAKE_DIMENSIONS);
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            assert!(
                (norm - 1.0).abs() < 1e-5,
                "expected a unit vector for {text:?}, got {norm}"
            );
        }
    }
}
