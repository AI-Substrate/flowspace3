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

/// Assert the [`Embedder`] contract over any implementation.
///
/// Proves: batch shape (one vector per input), **input order — every slot, not
/// just the first** — checked against an independently obtained vector for that
/// text, fixed dimensionality across calls, non-degenerate vectors, and
/// **determinism**: the same text embedded twice yields the same vector.
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

    // Input order, slot by slot. Embedding each text on its own yields an
    // independently obtained vector for that text, so the batch is correct only
    // if every slot equals its own. Checking slot 0 alone — as this once did —
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
        assert_eq!(
            alone[0], vectors[index],
            "contract: batch slot {index} must hold the embedding of input \
             {index} — same text, same vector, in input order"
        );
        assert_eq!(
            alone[0].len(),
            dimensions,
            "contract: dimensionality is stable across calls (index {index})"
        );
    }

    // Order is a property of the request, not of the text: the same texts sent
    // in a different order must come back in that different order.
    let reversed: Vec<String> = texts.iter().rev().cloned().collect();
    let reversed_vectors = embedder
        .embed(&reversed)
        .await
        .expect("embedder should embed a reordered batch");
    for (index, vector) in reversed_vectors.iter().enumerate() {
        assert_eq!(
            *vector,
            vectors[texts.len() - 1 - index],
            "contract: reordering the inputs reorders the outputs (index {index})"
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
