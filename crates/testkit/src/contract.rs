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
/// assertions keep their force.
const SAME_EMBEDDING: f32 = 0.999;

/// How close two vectors must sit to mean the same thing when they arrive in
/// the SAME response — tighter than [`SAME_EMBEDDING`], because there is no
/// batch composition to differ, but not exact, because there is quantisation.
const SAME_MEANING: f32 = 0.9999;

/// The widest component-wise gap a repeated text may show inside one response.
///
/// Live Azure returns differences of exactly 2^-13 for a repeated input; 2^-11
/// leaves four quantisation steps of headroom without approaching the scale at
/// which a component means something different.
const QUANTISATION_TOLERANCE: f32 = 1.0 / 2048.0;

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

/// Assert that two vectors from ONE response carry the same meaning.
///
/// Both checks are needed, and each covers what the other cannot: cosine is
/// blind to a pure rescale, and a component-wise bound is blind to a small
/// rotation of a small vector. Together they admit hardware quantisation and
/// nothing else — in particular they still reject a slot holding a different
/// text's embedding, which is the failure this assertion exists to catch.
fn assert_same_meaning(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "contract: {context} — dimensionality must be stable within a response"
    );

    let similarity = cosine(actual, expected);
    assert!(
        similarity >= SAME_MEANING,
        "contract: {context} — must mean the same thing, but cosine similarity \
         is {similarity}, below {SAME_MEANING}"
    );

    let (index, gap) = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .expect("a non-empty vector, already proved");
    assert!(
        gap <= QUANTISATION_TOLERANCE,
        "contract: {context} — component {index} differs by {gap}, beyond the \
         {QUANTISATION_TOLERANCE} a provider's quantisation can explain"
    );
}

/// Assert the [`Embedder`] contract over any implementation.
///
/// Proves: batch shape (one vector per input), **input order — every slot, not
/// just the first** — checked against an independently obtained vector for that
/// text, fixed dimensionality across calls, non-degenerate vectors, and
/// **determinism**: the same text embedded twice yields the same embedding.
///
/// Determinism is asserted at two grades, because the port promises two
/// different things. *Across* calls, comparison is cosine similarity against
/// [`SAME_EMBEDDING`] — a real provider's float kernels are not bit-reproducible
/// across batch compositions. *Within* one response, comparison is `==`: the
/// same text sent twice in one batch must come back bit-identical, since
/// nothing was recomputed between the two slots. The second grade is not
/// implied by the first, and a provider can pass one while failing the other.
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

    // Within ONE response the same text must come back meaning the same thing.
    //
    // This clause used to demand BIT-IDENTICAL vectors, reasoning that nothing
    // is recomputed inside a single call. Measurement says otherwise: 12
    // identical requests to a live Azure `text-embedding-3-small` deployment,
    // each carrying the same text in slots 0 and 2, returned DIFFERENT vectors
    // for those slots in 4 of the 12. The difference is always exactly 2^-13
    // (0.0001220703125) on at least one component — a quantisation step, not
    // drift — and it appears with and without the `dimensions` parameter, so it
    // is not Matryoshka truncation. Bit-exactness here failed a *correct*
    // provider about one run in six.
    //
    // So the defended property is the one fs3 actually relies on: a repeated
    // text is never given a different MEANING. Belt and braces, because either
    // check alone is weak — cosine cannot see a rescale, and an absolute
    // tolerance cannot see a small rotation of a small vector.
    //
    // Do not re-tighten this from first principles. The evidence above is why.
    let repeated = &texts[0];
    let duplicated = vec![repeated.clone(), texts[1].clone(), repeated.clone()];
    // Slots 0 and 2 must actually be the SAME text, or the comparison below is
    // asserting something else entirely — and would then be satisfied by an
    // embedder that ignores its input.
    assert_eq!(
        duplicated[0], duplicated[2],
        "harness: the repeat check needs the same text in both slots"
    );
    let duplicate_vectors = embedder
        .embed(&duplicated)
        .await
        .expect("embedder should embed a batch containing a repeated text");
    assert_eq!(
        duplicate_vectors.len(),
        duplicated.len(),
        "contract: one vector per input text, repeats included"
    );
    assert_ne!(
        duplicate_vectors[0], duplicate_vectors[1],
        "contract: two different texts in one response must not collapse to \
         one vector, which would make the repeat check below vacuous"
    );
    assert_same_meaning(
        &duplicate_vectors[0],
        &duplicate_vectors[2],
        "the same text twice in ONE response (slots 0 and 2)",
    );

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
    Element::new(
        fs3_core::ElementKind::Container,
        "struct_item",
        "Element",
        "core/src/element.rs::Element",
        fs3_core::Span::new(92, 118),
        "pub struct Element { pub name: String }",
    )
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

        fn key(&self) -> String {
            "jittery@8".to_string()
        }

        fn concurrency_ceiling(&self) -> usize {
            1
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

        fn key(&self) -> String {
            "swapped-slot@8".to_string()
        }

        fn concurrency_ceiling(&self) -> usize {
            1
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

        fn key(&self) -> String {
            "constant@8".to_string()
        }

        fn concurrency_ceiling(&self) -> usize {
            1
        }
    }

    /// Returns a DIFFERENT text's embedding in the repeated slot.
    ///
    /// This replaces a rescaling fixture that perturbed every vector by ~1e-5
    /// relative. That one was retired with the bit-exact clause it defended: a
    /// pure rescale is meaning-neutral under `vector_cosine_ops`, the metric
    /// fs3 actually searches with, so it violated no property fs3 relies on —
    /// and no tolerance wide enough to admit real hardware quantisation could
    /// have caught it anyway.
    ///
    /// Substituting another text's embedding is the failure that matters, and
    /// it is caught by any sane tolerance — which is what keeps the relaxed
    /// check provably non-vacuous.
    #[derive(Default)]
    struct SubstitutedSlotEmbedder {
        inner: FakeEmbedder,
    }

    #[async_trait]
    impl Embedder for SubstitutedSlotEmbedder {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let mut vectors = self.inner.embed(texts).await?;
            // Only when the batch actually repeats a text — [a, b, a] — hand
            // back b's vector for the second `a`, exactly as a mis-indexed
            // response would. Corrupting every batch would instead trip the
            // distinctness precondition earlier in the contract, and this
            // fixture would then prove nothing about the repeat check.
            if texts.len() > 2 && texts[0] == texts[2] {
                vectors[2] = vectors[1].clone();
            }
            Ok(vectors)
        }

        fn key(&self) -> String {
            "substituted-slot@8".to_string()
        }

        fn concurrency_ceiling(&self) -> usize {
            1
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

    #[tokio::test]
    async fn the_substitution_fixture_really_does_return_the_wrong_vector() {
        let embedder = SubstitutedSlotEmbedder::default();
        let texts = ["alpha alpha".to_string(), "beta beta".to_string()];
        let vectors = embedder
            .embed(&[texts[0].clone(), texts[1].clone(), texts[0].clone()])
            .await
            .expect("fixture embeds");

        // Without this, the should_panic test below could pass because the
        // fixture is broken rather than because the contract caught it.
        assert_eq!(
            vectors[2], vectors[1],
            "the fixture must put the WRONG text's vector in the repeated slot"
        );
    }

    #[tokio::test]
    #[should_panic(expected = "must mean the same thing")]
    async fn a_substituted_duplicate_slot_is_caught() {
        embedder_contract(&SubstitutedSlotEmbedder::default()).await;
    }

    /// The relaxation must actually admit what it was relaxed for. A provider
    /// whose repeated slot differs by one quantisation step is CORRECT, and
    /// this is the fixture that proves the harness now says so.
    #[tokio::test]
    async fn a_quantisation_step_between_duplicate_slots_is_accepted() {
        /// Nudges one component of every vector by the 2^-13 step measured on
        /// live Azure.
        #[derive(Default)]
        struct QuantisedEmbedder {
            inner: FakeEmbedder,
            vectors: AtomicUsize,
        }

        #[async_trait]
        impl Embedder for QuantisedEmbedder {
            async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
                let mut vectors = self.inner.embed(texts).await?;
                for vector in &mut vectors {
                    let nth = self.vectors.fetch_add(1, Ordering::Relaxed);
                    if nth.is_multiple_of(2) {
                        vector[0] += 1.0 / 8192.0;
                    }
                }
                Ok(vectors)
            }

            fn key(&self) -> String {
                "quantised@8".to_string()
            }

            fn concurrency_ceiling(&self) -> usize {
                1
            }
        }

        let embedder = QuantisedEmbedder::default();
        let text = "fn main() { println!(\"hello\"); }".to_string();
        let vectors = embedder
            .embed(&[text.clone(), text])
            .await
            .expect("fixture embeds");
        assert_ne!(
            vectors[0], vectors[1],
            "the fixture must actually differ, or this proves nothing"
        );

        // The finding, as a test: bit-exactness would panic here, and a real
        // provider does exactly this about one response in three.
        embedder_contract(&embedder).await;
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
