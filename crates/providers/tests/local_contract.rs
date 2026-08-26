//! The local ONNX embedder runs the *same* contract harness the fake does.
//!
//! `#[ignore]`d on the **slow** tier, not the keyed one. There is nothing to
//! authenticate against here — the run is free and needs no account — but it
//! loads a real ONNX model, which costs ~18 s and a ~129 MB download the first
//! time on any machine. That is enough to keep it out of the default
//! `cargo test` / `harness checks` path, which must stay fast and offline
//! everywhere.
//!
//! Repo convention: an `#[ignore]` carries a reason, and the reason names its
//! tier — `keyed: <vars>` for runs that need credentials, `slow: <why>` for
//! runs that are merely expensive. A bare `#[ignore]` says nothing about which
//! it is, and so says nothing about whether *you* can run it.
//!
//! # Running it
//!
//! ```bash
//! cargo test -p fs3-providers --test local_contract -- --ignored
//!
//! # optional — put the model cache somewhere else (default:
//! # ~/.cache/flowspace3/models). NEVER point this inside a repository.
//! export FS3_LOCAL_MODEL_CACHE=/var/cache/flowspace3/models
//!
//! # optional — run a different catalogue model, by name or HuggingFace code:
//! export FS3_LOCAL_MODEL=AllMiniLML6V2
//! ```
//!
//! A first run with no network fails rather than skipping: a green test that
//! silently did nothing is worse than a red one. After that first pull the
//! whole file runs offline in about a fifth of a second.

use std::sync::LazyLock;

use fs3_providers::{DEFAULT_LOCAL_MODEL, LocalEmbedder, LocalEmbedderConfig};
use fs3_testkit::embedder_contract;

/// The one loaded model every test in this file shares.
///
/// Not an optimisation. `cargo test` runs these concurrently, and three
/// threads each loading a cold cache means three simultaneous downloads of the
/// same files into the same directory — which collide, and two of the three
/// fail. Loading once is also what a daemon does: build the provider at the
/// composition root, then share the handle.
static MODEL: LazyLock<LocalEmbedder> = LazyLock::new(|| {
    let model =
        std::env::var("FS3_LOCAL_MODEL").unwrap_or_else(|_| DEFAULT_LOCAL_MODEL.to_string());
    let mut config = LocalEmbedderConfig::new(&model).expect("a catalogue model name");
    if let Ok(dir) = std::env::var("FS3_LOCAL_MODEL_CACHE") {
        config = config.with_cache_dir(dir);
    }
    LocalEmbedder::load(config).expect(
        "the local model should load; the FIRST run downloads ~128 MB from HuggingFace and \
         needs network access — see this file's header",
    )
});

/// Get the shared model, loading it on a blocking thread the first time.
///
/// `load` downloads and builds an ONNX session; doing that on an async worker
/// thread is precisely what the adapter tells callers not to do, so the test
/// demonstrates the documented shape rather than contradicting it. Cloning the
/// handle shares the loaded session — it reloads nothing.
async fn embedder() -> LocalEmbedder {
    tokio::task::spawn_blocking(|| MODEL.clone())
        .await
        .expect("model load task")
}

#[tokio::test]
#[ignore = "slow: loads the ONNX model (~18s and ~129MB on a cold cache) — see this file's header"]
async fn local_embedder_honours_the_embedder_contract() {
    embedder_contract(&embedder().await).await;
}

/// The width the store's vector column has to be built for.
///
/// `dimensions()` answers before anything is embedded, which is what lets a
/// config-time check catch a model swap. fs2 shipped without that answer and
/// had to add a mismatch guard later; the number is asserted here so a
/// catalogue change cannot move it quietly.
#[tokio::test]
#[ignore = "slow: loads the ONNX model (~18s and ~129MB on a cold cache) — see this file's header"]
async fn the_default_model_embeds_at_the_width_it_advertises() {
    let embedder = embedder().await;
    if embedder.model() != DEFAULT_LOCAL_MODEL {
        return; // a caller-chosen model has its own width
    }

    assert_eq!(embedder.dimensions(), 384, "BGE-small is a 384-dim model");

    let vectors = <LocalEmbedder as fs3_core::Embedder>::embed(
        &embedder,
        &["a short piece of prose".to_string()],
    )
    .await
    .expect("a one-text batch");
    assert_eq!(
        vectors[0].len(),
        embedder.dimensions(),
        "the advertised width must be the width actually produced"
    );
}

/// Semantic sanity: the vectors must carry meaning, not just shape.
///
/// The whole contract suite is satisfiable by an embedder that hashes its
/// input — deterministic, ordered, non-degenerate, and useless for search.
/// This is the assertion that says the model is really a model.
#[tokio::test]
#[ignore = "slow: loads the ONNX model (~18s and ~129MB on a cold cache) — see this file's header"]
async fn near_synonyms_sit_closer_than_unrelated_words() {
    let embedder = embedder().await;
    let texts = ["cat", "kitten", "carburetor"].map(str::to_string).to_vec();
    let vectors = <LocalEmbedder as fs3_core::Embedder>::embed(&embedder, &texts)
        .await
        .expect("a three-text batch");

    let cosine = |a: &[f32], b: &[f32]| -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (na * nb)
    };

    let related = cosine(&vectors[0], &vectors[1]);
    let unrelated = cosine(&vectors[0], &vectors[2]);
    assert!(
        related > unrelated,
        "cos(cat, kitten) = {related} should exceed cos(cat, carburetor) = {unrelated}"
    );
}
