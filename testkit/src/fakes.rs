//! Deterministic fakes for both ports.

use std::sync::Mutex;

use async_trait::async_trait;
use fs3_core::{Element, Embedder, Error, Result, Summarizer, Summary};

/// Vector width the fakes produce. Small enough to eyeball in a failing
/// assertion, wide enough for cosine similarity to mean something.
pub const FAKE_DIMENSIONS: usize = 8;

/// Hash text into a unit vector.
///
/// FNV-1a over the bytes, re-hashed per dimension, then L2-normalised. The
/// point is not cryptographic quality — it is that *the same text always
/// yields the same vector*, so similarity search behaves meaningfully in tests
/// and near-identical inputs do not collide.
fn hash_vector(text: &str, dimensions: usize) -> Vec<f32> {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut raw = Vec::with_capacity(dimensions);
    for dimension in 0..dimensions {
        let mut hash = OFFSET ^ (dimension as u64).wrapping_mul(PRIME);
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        // Map to [-1, 1] without going through a float cast of the full u64.
        let scaled = ((hash >> 11) as f64) / ((1u64 << 53) as f64);
        raw.push((scaled * 2.0 - 1.0) as f32);
    }

    let norm = raw.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        return raw;
    }
    raw.into_iter().map(|v| v / norm).collect()
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
            calls.push(element.qualified_name.clone());
            calls.len() - 1
        };
        if self.fail_after.is_some_and(|limit| call_index >= limit) {
            return Err(Error::Provider(format!(
                "FakeSummarizer: injected failure on call {}",
                call_index + 1
            )));
        }

        // Tags from the element's own address: always 1–5, always the same for
        // the same element (PRD req 36's band, honoured by the fake too).
        let mut tags = vec![element.kind.as_str().to_string()];
        tags.extend(
            element
                .qualified_name
                .split(['.', '>'])
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(str::to_lowercase)
                .take(4),
        );

        Ok(Summary {
            text: format!(
                "{} `{}` at {}:{}-{} ({} lines)",
                element.kind,
                element.qualified_name,
                element.path,
                element.start_line,
                element.end_line,
                element.line_count()
            ),
            tags,
        })
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
}
