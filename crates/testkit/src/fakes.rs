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
    /// The per-input cap this fake declares AND enforces.
    ///
    /// [`usize::MAX`] by default, which is the shape of a provider that never
    /// rejects. Set it — via [`FakeEmbedder::capped`] — to make the fake
    /// behave like a hosted embeddings API, which answers an oversized input
    /// with a 400 rather than a short vector.
    pub max_input_tokens: usize,
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail_after: None,
            dimensions: FAKE_DIMENSIONS,
            max_input_tokens: usize::MAX,
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

    /// A fake that refuses any single input over `max_input_tokens`, the way
    /// Azure and OpenAI do.
    ///
    /// This is what makes a per-input guard testable without a network: the
    /// refusal below is modelled on the real message, and a caller that does
    /// not truncate gets it.
    pub fn capped(max_input_tokens: usize) -> Self {
        Self {
            max_input_tokens,
            ..Self::default()
        }
    }

    /// How many times [`Embedder::embed`] has been called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("fake embedder lock").len()
    }

    /// Every text this fake has been handed, flattened across calls.
    ///
    /// The surface a guard test asserts against: what ARRIVED at the provider,
    /// not what the caller believed it sent.
    pub fn received(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("fake embedder lock")
            .iter()
            .flatten()
            .cloned()
            .collect()
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

        // The hosted providers' actual behaviour: one oversized member fails
        // the WHOLE request, naming the offending index, and says nothing
        // about the others. Counted with `fs3_core`'s convention because the
        // guard truncates with that same convention — a fake that measured
        // differently would be testing the disagreement instead of the guard.
        if let Some((index, text)) = texts
            .iter()
            .enumerate()
            .find(|(_, text)| fs3_core::estimate_tokens(text) > self.max_input_tokens)
        {
            return Err(Error::Provider(format!(
                "FakeEmbedder: Invalid 'input[{index}]': maximum input length is {} tokens, \
                 got about {}",
                self.max_input_tokens,
                fs3_core::estimate_tokens(text)
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

    /// Whatever the fake was built with: [`usize::MAX`] unless
    /// [`FakeEmbedder::capped`] said otherwise.
    fn max_input_tokens(&self) -> usize {
        self.max_input_tokens
    }
}

/// A [`Summarizer`] that produces a stable, readable summary and tags derived
/// from the element's own address — no network, no keys, no model.
#[derive(Debug)]
pub struct FakeSummarizer {
    /// Qualified names passed to [`Summarizer::summarize`], in call order.
    pub calls: Mutex<Vec<String>>,
    /// The element BODIES passed in, in call order.
    ///
    /// Separate from `calls` because an address says which element was
    /// summarised and only the body says how much of it actually travelled —
    /// which is the question a truncation guard has to answer.
    pub bodies: Mutex<Vec<String>>,
    /// Fail every call after this many successful ones. `None` never fails.
    pub fail_after: Option<usize>,
    /// The prompt cap this fake declares AND enforces. See
    /// [`FakeEmbedder::max_input_tokens`].
    pub max_input_tokens: usize,
}

impl Default for FakeSummarizer {
    fn default() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            bodies: Mutex::new(Vec::new()),
            fail_after: None,
            max_input_tokens: usize::MAX,
        }
    }
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

    /// A fake that refuses any prompt over `max_input_tokens`.
    pub fn capped(max_input_tokens: usize) -> Self {
        Self {
            max_input_tokens,
            ..Self::default()
        }
    }

    /// How many times [`Summarizer::summarize`] has been called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("fake summarizer lock").len()
    }

    /// Every element body this fake has been handed, in call order.
    pub fn received(&self) -> Vec<String> {
        self.bodies.lock().expect("fake summarizer lock").clone()
    }
}

#[async_trait]
impl Summarizer for FakeSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        let call_index = {
            let mut calls = self.calls.lock().expect("fake summarizer lock");
            calls.push(element.address.clone());
            self.bodies
                .lock()
                .expect("fake summarizer lock")
                .push(element.raw_text.clone());
            calls.len() - 1
        };
        if self.fail_after.is_some_and(|limit| call_index >= limit) {
            return Err(Error::Provider(format!(
                "FakeSummarizer: injected failure on call {}",
                call_index + 1
            )));
        }

        // A chat endpoint out of context refuses the call. See
        // [`FakeEmbedder::embed`] for why the fake counts the way the guard
        // counts.
        let tokens = fs3_core::estimate_tokens(&element.raw_text);
        if tokens > self.max_input_tokens {
            return Err(Error::Provider(format!(
                "FakeSummarizer: prompt of about {tokens} tokens exceeds the {} this model \
                 accepts",
                self.max_input_tokens
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

    /// Whatever the fake was built with: [`usize::MAX`] unless
    /// [`FakeSummarizer::capped`] said otherwise.
    fn max_input_tokens(&self) -> usize {
        self.max_input_tokens
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
