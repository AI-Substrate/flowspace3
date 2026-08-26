//! Contract tests: written once, run over every implementation of a port.
//!
//! The fake runs them in CI; a real provider runs the same function behind
//! `#[ignore]` on demand. That symmetry is the only thing that keeps a fake
//! honest, and it is why fs3 needs no mocking framework.
//!
//! Each harness panics on violation, so callers are one line:
//!
//! ```
//! # use fs3_testkit::{FakeEmbedder, embedder_contract};
//! # tokio_test_stub(async {
//! embedder_contract(&FakeEmbedder::default()).await;
//! # });
//! # fn tokio_test_stub<F: std::future::Future>(_f: F) {}
//! ```

use fs3_core::{Element, Embedder, Summarizer};

/// How close two vectors must sit to count as *the same embedding* when they
/// arrive from SEPARATE calls.
///
/// Bit-exact equality across calls is a property of our fake, not of the port.
/// A real provider batches its float kernels, so the reduction order for one
/// text depends on what else travelled with it: the same input embedded alone
/// and embedded inside a batch of three can differ in the last few ulps.
/// Demanding `==` there made the keyed promotion run plausibly unsatisfiable —
/// the harness would have failed a *correct* provider.
///
/// 0.999 sits far above provider jitter (which lands around 1.0 - 1e-6) and far
/// below the similarity of two genuinely different texts, so the ordering
/// assertions keep their force. Within a single response nothing is
/// recomputed, so equality there stays exact.
const SAME_EMBEDDING: f32 = 0.999;

/// Cosine similarity. Callers have already proved both vectors non-degenerate,
/// so the denominator cannot be zero.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|y| y * y).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}

/// Assert that two vectors obtained from separate calls are the same embedding.
fn assert_same_embedding(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "contract: {context} — dimensionality must be stable across calls"
    );
    let similarity = cosine(actual, expected);
    assert!(
        similarity >= SAME_EMBEDDING,
        "contract: {context} — expected the same embedding, but cosine \
         similarity is {similarity}, below {SAME_EMBEDDING}"
    );
}

/// Assert the [`Embedder`] contract over any implementation.
///
/// Proves: batch shape (one vector per input), **input order — every slot, not
/// just the first** — checked against an independently obtained vector for that
/// text, fixed dimensionality across calls, non-degenerate vectors, and
/// **determinism**: the same text embedded twice yields the same embedding.
///
/// Comparisons *within* one response are exact. Comparisons *across* calls use
/// cosine similarity against [`SAME_EMBEDDING`], because a real provider's
/// float kernels are not bit-reproducible across batch compositions.
///
/// # Panics
/// On any contract violation, naming the property that broke.
pub async fn embedder_contract<E: Embedder + ?Sized>(embedder: &E) {
    let texts: Vec<String> = vec![
        "fn main() { println!(\"hello\"); }".to_string(),
        "# Architecture\n\nThe workspace is the architecture.".to_string(),
        "struct Calculator { total: i64 }".to_string(),
    ];

    let vectors = embedder
        .embed(&texts)
        .await
        .expect("embedder should embed a plain batch");

    assert_eq!(
        vectors.len(),
        texts.len(),
        "contract: one vector per input text"
    );

    let dimensions = vectors[0].len();
    assert!(dimensions > 0, "contract: vectors must not be empty");
    for (index, vector) in vectors.iter().enumerate() {
        assert_eq!(
            vector.len(),
            dimensions,
            "contract: every vector in a batch has the same dimensionality (index {index})"
        );
        assert!(
            vector.iter().all(|value| value.is_finite()),
            "contract: vectors contain only finite values (index {index})"
        );
        assert!(
            vector.iter().any(|value| *value != 0.0),
            "contract: vectors must not be all-zero (index {index})"
        );
    }

    // The slot checks below identify a vector by *similarity*, so they mean
    // nothing unless the sample texts embed to distinguishable vectors. Prove
    // that precondition rather than assume it: an embedder returning one
    // constant vector per call would otherwise satisfy every ordering
    // assertion that follows.
    for (left, first) in vectors.iter().enumerate() {
        for (right, second) in vectors.iter().enumerate().skip(left + 1) {
            let similarity = cosine(first, second);
            assert!(
                similarity < SAME_EMBEDDING,
                "contract: inputs {left} and {right} are different texts but \
                 embed to the same vector (cosine {similarity}), which would \
                 make the ordering checks vacuous"
            );
        }
    }

    // Input order, slot by slot. Embedding each text on its own yields an
    // independently obtained vector for that text, so the batch is correct only
    // if every slot holds its own. Checking slot 0 alone — as this once did —
    // is passed by an implementation that swaps slots 1 and 2.
    for (index, text) in texts.iter().enumerate() {
        let alone = embedder
            .embed(std::slice::from_ref(text))
            .await
            .expect("embedder should embed a single-item batch");
        assert_eq!(
            alone.len(),
            1,
            "contract: a single-item batch returns one vector (index {index})"
        );
        assert_same_embedding(
            &alone[0],
            &vectors[index],
            &format!(
                "batch slot {index} must hold the embedding of input {index} — \
                 same text, same vector, in input order"
            ),
        );
    }

    // Order is a property of the request, not of the text: the same texts sent
    // in a different order must come back in that different order.
    let reversed: Vec<String> = texts.iter().rev().cloned().collect();
    let reversed_vectors = embedder
        .embed(&reversed)
        .await
        .expect("embedder should embed a reordered batch");
    assert_eq!(
        reversed_vectors.len(),
        texts.len(),
        "contract: one vector per input text, whatever the order"
    );
    for (index, vector) in reversed_vectors.iter().enumerate() {
        assert_same_embedding(
            vector,
            &vectors[texts.len() - 1 - index],
            &format!("reordering the inputs reorders the outputs (index {index})"),
        );
    }

    // An empty batch is legal and costs nothing.
    let empty = embedder
        .embed(&[])
        .await
        .expect("embedder should accept an empty batch");
    assert!(empty.is_empty(), "contract: empty batch returns no vectors");
}

/// Assert the [`Summarizer`] contract over any implementation.
///
/// Proves: a non-empty summary, and the 1–5 tag band of PRD req 36.
///
/// # Panics
/// On any contract violation, naming the property that broke.
pub async fn summarizer_contract<S: Summarizer + ?Sized>(summarizer: &S) {
    let element = sample_element();

    let summary = summarizer
        .summarize(&element)
        .await
        .expect("summarizer should summarize a plain element");

    assert!(
        !summary.text.trim().is_empty(),
        "contract: summary text must not be empty"
    );
    assert!(
        summary.has_valid_tags(),
        "contract: PRD req 36 mandates 1-5 tags, got {}: {:?}",
        summary.tags.len(),
        summary.tags
    );
    assert!(
        summary.tags.iter().all(|tag| !tag.trim().is_empty()),
        "contract: tags must not be blank: {:?}",
        summary.tags
    );
}

/// A plain element for contract harnesses to work on.
pub fn sample_element() -> Element {
    Element {
        path: "core/src/element.rs".to_string(),
        blob: fs3_core::BlobRef::new("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391")
            .expect("literal is a valid digest"),
        ts_kind: "struct_item".to_string(),
        kind: fs3_core::ElementKind::Type,
        qualified_name: "Element".to_string(),
        start_line: 92,
        end_line: 118,
        text: "pub struct Element { pub path: String }".to_string(),
        has_error: false,
    }
}

#[cfg(test)]
mod tests {
    //! The harness judging itself.
    //!
    //! Two properties are in tension here, so both are pinned: the harness must
    //! ACCEPT a provider whose floats wobble between calls, and must still
    //! REJECT one that mixes up the slots. Loosening the first without pinning
    //! the second is how a contract test quietly stops testing.

    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use fs3_core::Result;

    use super::*;
    use crate::FakeEmbedder;

    /// Returns the fake's vectors with a fresh last-ulp-scale perturbation on
    /// every call — what a real batched provider does, and precisely what the
    /// old bit-exact harness would have rejected.
    #[derive(Default)]
    struct JitteryEmbedder {
        inner: FakeEmbedder,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Embedder for JitteryEmbedder {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            let mut vectors = self.inner.embed(texts).await?;
            for vector in &mut vectors {
                for (index, value) in vector.iter_mut().enumerate() {
                    let sign = if (index + call).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    };
                    *value += *value * sign * 1e-5;
                }
            }
            Ok(vectors)
        }
    }

    /// Honest vectors, wrong order: the exact defect the slot-by-slot check
    /// exists to catch.
    #[derive(Default)]
    struct SwappedSlotEmbedder(FakeEmbedder);

    #[async_trait]
    impl Embedder for SwappedSlotEmbedder {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let mut vectors = self.0.embed(texts).await?;
            if vectors.len() > 2 {
                vectors.swap(1, 2);
            }
            Ok(vectors)
        }
    }

    /// One vector for every input. Bit-exact and deterministic — which is why
    /// the old harness passed it, and why the distinctness precondition exists.
    struct ConstantEmbedder;

    #[async_trait]
    impl Embedder for ConstantEmbedder {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.5_f32; 8]).collect())
        }
    }

    #[tokio::test]
    async fn float_jitter_across_calls_is_still_the_same_embedding() {
        let embedder = JitteryEmbedder::default();
        let text = vec!["fn main() { println!(\"hello\"); }".to_string()];

        // The fixture has to actually wobble, or this proves nothing.
        let once = embedder.embed(&text).await.expect("first call");
        let twice = embedder.embed(&text).await.expect("second call");
        assert_ne!(
            once[0], twice[0],
            "the fixture must perturb its output, otherwise this test passes \
             for the wrong reason"
        );
        let similarity = cosine(&once[0], &twice[0]);
        assert!(
            similarity >= SAME_EMBEDDING,
            "the perturbation must stay within provider jitter, got {similarity}"
        );

        // This is the finding: bit-exact equality across calls would panic here.
        embedder_contract(&embedder).await;
    }

    #[tokio::test]
    #[should_panic(expected = "batch slot 1")]
    async fn a_swapped_slot_is_still_caught_by_similarity() {
        embedder_contract(&SwappedSlotEmbedder::default()).await;
    }

    #[tokio::test]
    #[should_panic(expected = "embed to the same vector")]
    async fn one_vector_for_every_text_fails_the_distinctness_precondition() {
        embedder_contract(&ConstantEmbedder).await;
    }

    #[test]
    fn cosine_separates_provider_jitter_from_a_different_text() {
        let vector = [1.0_f32, 2.0, 3.0, 4.0];
        let jittered = [1.00001_f32, 1.99998, 3.00002, 3.99997];
        assert!(cosine(&vector, &jittered) >= SAME_EMBEDDING);

        let different = [4.0_f32, 1.0, -2.0, 0.5];
        assert!(cosine(&vector, &different) < SAME_EMBEDDING);
    }
}
