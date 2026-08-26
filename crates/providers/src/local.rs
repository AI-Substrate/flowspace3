//! The local ONNX adapter for the [`Embedder`] port — no server, no key.
//!
//! Everything here runs in this process: a quantisation-free ONNX Runtime
//! session, statically linked, over a sentence-transformer model pulled once
//! from HuggingFace and cached on disk. After that first pull there is no
//! network at all, which is what makes this the air-gapped and test-friendly
//! option the roster asks for.
//!
//! The work is done by [`fastembed`], which owns the four things fs2 had to
//! hand-roll in Python — the ORT session, the tokenizer, the model-specific
//! pooling (CLS for BGE, mean for MiniLM; **mean on a BGE model is silently
//! wrong**, not an error), and the L2 normalisation. Reimplementing those here
//! would be exactly the reinvention the arch allow-list exists to notice.
//!
//! ## Three facts that shape this adapter
//!
//! 1. [`fastembed::TextEmbedding::embed`] takes `&mut self`, and the port is
//!    used as `Arc<dyn Embedder>` with `&self`. Hence the [`Mutex`].
//! 2. Inference is CPU-bound and blocking — hundreds of milliseconds for a
//!    real batch. Running it on an async worker thread would stall every other
//!    task on that thread, so it goes to [`tokio::task::spawn_blocking`]. The
//!    lock is taken *inside* the blocking closure and never held across an
//!    await point.
//! 3. `fastembed`'s own cache default is `./.fastembed_cache` — **relative to
//!    the current directory**, i.e. straight into whatever repository the
//!    daemon happens to be scanning. This adapter therefore always passes an
//!    explicit cache directory, defaulting to [`default_cache_dir`].
//!
//! ## Snap-in
//!
//! Wiring happens at adoption, by the integrating stream — this crate holds
//! adapters only, and never reads config. The recipe, for whoever does it:
//!
//! ```ignore
//! // fs3-core::config — one new variant
//! pub enum ProviderConfig {
//!     Fake,
//!     OpenAi { .. },
//!     AzureOpenAi { .. },
//!     Local {
//!         /// Either the catalogue name ("BGESmallENV15") or the HuggingFace
//!         /// code ("BAAI/bge-small-en-v1.5"). Defaults to DEFAULT_LOCAL_MODEL.
//!         model: Option<String>,
//!         /// Where models are cached. Defaults to `~/.cache/flowspace3/models`.
//!         /// NEVER a path inside a repository.
//!         cache_dir: Option<PathBuf>,
//!         /// ONNX Runtime intra-op threads. `None` uses every core.
//!         intra_threads: Option<usize>,
//!     },
//! }
//!
//! // fs3-daemon composition root — one new match arm.
//! // NOTE: `load` blocks and may download ~128 MB on first use. At a
//! // composition root that runs before the daemon serves traffic this is
//! // fine; anywhere inside a request path it must be spawn_blocking'd.
//! ProviderConfig::Local { model, cache_dir, intra_threads } => {
//!     let mut config = LocalEmbedderConfig::new(
//!         model.as_deref().unwrap_or(DEFAULT_LOCAL_MODEL),
//!     )?;
//!     if let Some(dir) = cache_dir {
//!         config = config.with_cache_dir(dir);
//!     }
//!     if let Some(threads) = intra_threads {
//!         config = config.with_intra_threads(threads);
//!     }
//!     Arc::new(LocalEmbedder::load(config)?) as Arc<dyn Embedder>
//! }
//! ```
//!
//! There is no local [`fs3_core::Summarizer`] here: generation needs a
//! different class of model entirely, and the roster tracks it separately.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use fs3_core::{Embedder, Error, Result};

/// The model this adapter uses when the caller has no opinion.
///
/// `BAAI/bge-small-en-v1.5`: 384 dimensions, ~128 MB on disk, CLS pooling, and
/// the same default fs2 settled on — so a repository indexed by either system
/// produces comparable vectors.
pub const DEFAULT_LOCAL_MODEL: &str = "BGESmallENV15";

/// One entry of the model catalogue: what to call it, what it really is, and
/// how wide its vectors are.
///
/// The width matters before anything is embedded: the store's vector column is
/// fixed-width, and mixing dimensions silently breaks search rather than
/// failing loudly. fs2 learned this and added a mismatch guard; exposing the
/// number here is what lets fs3 have that conversation at config time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModelInfo {
    /// The catalogue name accepted by [`LocalEmbedderConfig::new`].
    pub name: String,
    /// The HuggingFace repository the weights come from — also accepted by
    /// [`LocalEmbedderConfig::new`].
    pub huggingface_code: String,
    /// Vector width. The store column has to match this.
    pub dimensions: usize,
    /// One line from the model's own catalogue entry.
    pub description: String,
}

/// Every model this adapter can run, sorted by catalogue name.
///
/// Sorted, because `fastembed` stores its catalogue in a `HashMap` and hands
/// it back in whatever order that iterates — which changes between runs and
/// would make any error message built from this list unreproducible.
pub fn supported_models() -> Vec<LocalModelInfo> {
    let mut models: Vec<LocalModelInfo> = TextEmbedding::list_supported_models()
        .into_iter()
        .map(|info| LocalModelInfo {
            name: info.model.to_string(),
            huggingface_code: info.model_code,
            dimensions: info.dim,
            description: info.description,
        })
        .collect();
    models.sort_by(|a, b| a.name.cmp(&b.name));
    models
}

/// Where models are cached when the caller names no directory.
///
/// `~/.cache/flowspace3/models` on every platform — the same shape
/// HuggingFace's own tooling uses, and deliberately not a per-OS location, so
/// that a cache warmed on one machine is findable on another. Never inside a
/// repository: the daemon's working directory is *someone's source tree*.
///
/// # Errors
/// [`Error::Provider`] when the home directory cannot be determined, naming the
/// variable to set.
pub fn default_cache_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            Error::Provider(
                "local embeddings: cannot find a home directory for the model cache; set HOME \
                 (or USERPROFILE on Windows), or configure an explicit cache directory"
                    .to_string(),
            )
        })?;
    Ok(Path::new(&home)
        .join(".cache")
        .join("flowspace3")
        .join("models"))
}

/// Resolve a model name to a catalogue entry, case-insensitively.
///
/// Three forms are accepted, tried in this order:
///
/// 1. the catalogue name — `BGESmallENV15`;
/// 2. the exact HuggingFace code the weights come from —
///    `Xenova/bge-small-en-v1.5`;
/// 3. the bare model id, ignoring the account that publishes it —
///    `bge-small-en-v1.5`.
///
/// The third form is not decoration. `fastembed` pulls its ONNX exports from
/// re-publishers (`Xenova/…`, `Qdrant/…`), so the code it reports is *not* the
/// name anyone knows the model by: fs2's config says `BAAI/bge-small-en-v1.5`,
/// and a config carried across would otherwise be rejected as unknown while
/// being plainly correct. Matching on the id after the `/` accepts every
/// publisher's spelling of the same weights.
///
/// Ambiguity is an error rather than a silent first-match: `fastembed`'s
/// catalogue really does map two entries onto one code (the two
/// EmbeddingGemma variants), and picking one of those by iteration order would
/// be a coin toss the caller never sees.
fn parse_model(name: &str) -> Result<EmbeddingModel> {
    let wanted = name.trim();
    let catalogue = TextEmbedding::list_supported_models();

    if let Some(info) = catalogue
        .iter()
        .find(|info| info.model.to_string().eq_ignore_ascii_case(wanted))
    {
        return Ok(info.model.clone());
    }

    let by_code: Vec<_> = catalogue
        .iter()
        .filter(|info| info.model_code.eq_ignore_ascii_case(wanted))
        .collect();
    let matches = if by_code.is_empty() {
        catalogue
            .iter()
            .filter(|info| bare_id(&info.model_code).eq_ignore_ascii_case(bare_id(wanted)))
            .collect()
    } else {
        by_code
    };

    match matches.as_slice() {
        [info] => Ok(info.model.clone()),
        [] => Err(Error::Provider(format!(
            "local embeddings: unknown model {wanted:?}; name it by catalogue entry \
             (e.g. {DEFAULT_LOCAL_MODEL}, the default), by HuggingFace code \
             (e.g. Xenova/bge-small-en-v1.5) or by bare model id \
             (e.g. bge-small-en-v1.5). Known entries: {}",
            supported_models()
                .iter()
                .map(|info| info.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        several => Err(Error::Provider(format!(
            "local embeddings: {wanted:?} matches more than one catalogue entry ({}); name the \
             one you want exactly",
            several
                .iter()
                .map(|info| info.model.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// The part of a HuggingFace code after the publishing account.
fn bare_id(code: &str) -> &str {
    code.rsplit('/').next().unwrap_or(code)
}

/// Everything [`LocalEmbedder::load`] needs. No credentials — that is the point.
#[derive(Debug, Clone)]
pub struct LocalEmbedderConfig {
    model: EmbeddingModel,
    cache_dir: PathBuf,
    intra_threads: Option<usize>,
    batch_size: Option<usize>,
}

impl LocalEmbedderConfig {
    /// Build a config for `model`, caching in [`default_cache_dir`].
    ///
    /// # Errors
    /// [`Error::Provider`] when the name matches no catalogue entry (the error
    /// lists the valid ones) or the default cache directory cannot be resolved.
    pub fn new(model: &str) -> Result<Self> {
        Ok(Self {
            model: parse_model(model)?,
            cache_dir: default_cache_dir()?,
            intra_threads: None,
            batch_size: None,
        })
    }

    /// Cache models under `dir` instead of [`default_cache_dir`].
    ///
    /// The directory is created on first use. Pointing it inside a repository
    /// is a mistake this adapter cannot detect for you.
    #[must_use]
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = dir.into();
        self
    }

    /// Cap ONNX Runtime's intra-op threads. Unset means every available core.
    ///
    /// This is the one knob that matters on a developer laptop, where an
    /// all-cores embedding batch makes everything else stutter.
    #[must_use]
    pub fn with_intra_threads(mut self, threads: usize) -> Self {
        self.intra_threads = Some(threads);
        self
    }

    /// Split batches larger than `size` into separate inference runs.
    ///
    /// Unset means `fastembed`'s own default (256). Leave it unset for
    /// dynamically-quantised models — they reject a batch size smaller than the
    /// input, because the quantisation range is recomputed per batch.
    #[must_use]
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }

    /// The vector width this configuration will produce, known before a single
    /// text is embedded and before anything is downloaded.
    pub fn dimensions(&self) -> usize {
        TextEmbedding::get_model_info(&self.model)
            .map(|info| info.dim)
            .unwrap_or_default()
    }

    /// The HuggingFace repository the weights come from.
    pub fn huggingface_code(&self) -> String {
        TextEmbedding::get_model_info(&self.model)
            .map(|info| info.model_code.clone())
            .unwrap_or_default()
    }
}

/// [`Embedder`] backed by a local ONNX model. No API, no key, no network after
/// the first load.
///
/// `Clone` is a handle clone: every clone drives the same loaded session, so
/// cloning costs nothing and does not re-download or re-open the model.
/// There is no `Debug` — `fastembed`'s session has none, and a derived one
/// would print a few hundred megabytes of nothing useful.
#[derive(Clone)]
pub struct LocalEmbedder {
    /// `Mutex` because `fastembed` embeds through `&mut self`; `Arc` because
    /// the blocking closure needs to own what it locks.
    session: Arc<Mutex<TextEmbedding>>,
    batch_size: Option<usize>,
    model: String,
    dimensions: usize,
}

impl LocalEmbedder {
    /// Load the model, downloading it on first use.
    ///
    /// **This blocks, and the first call for a given model downloads roughly
    /// 128 MB.** Call it from a composition root at start-up, or wrap it in
    /// [`tokio::task::spawn_blocking`] — never from inside a request path.
    /// Subsequent loads read the cache and need no network at all.
    ///
    /// # Errors
    /// [`Error::Provider`] when the model cannot be fetched or the ONNX session
    /// cannot be built, naming the model, the cache directory and the fix.
    pub fn load(config: LocalEmbedderConfig) -> Result<Self> {
        let LocalEmbedderConfig {
            model,
            cache_dir,
            intra_threads,
            batch_size,
        } = config;

        let dimensions = TextEmbedding::get_model_info(&model)
            .map_err(|e| Error::Provider(format!("local embeddings: {e}")))?
            .dim;
        let label = model.to_string();

        let mut options = TextInitOptions::new(model)
            // Always explicit: the crate's default would write into the
            // current working directory.
            .with_cache_dir(cache_dir.clone())
            // A progress bar on a daemon's stdout is noise, not telemetry.
            .with_show_download_progress(false);
        if let Some(threads) = intra_threads {
            options = options.with_intra_threads(threads);
        }

        let session = TextEmbedding::try_new(options)
            .map_err(|e| load_error(&label, &cache_dir, &describe(&e)))?;

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            batch_size,
            model: label,
            dimensions,
        })
    }

    /// Vector width. The store's column has to match this exactly.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// The catalogue name of the loaded model.
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// Flatten an error and everything that caused it into one line.
///
/// `fastembed`'s `Display` for the common failure is
/// `Failed to retrieve model file 'onnx/model.onnx'` — the `#[source]` holds
/// the part that says *why*, and `{e}` drops it on the floor. Walking the
/// chain is the difference between "it failed" and "connection refused".
fn describe(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(inner) = cause {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        cause = inner.source();
    }
    message
}

/// Turn a load failure into something that names the fix.
///
/// A first run without network is the single most likely failure of this
/// adapter, and `fastembed`'s own message names neither the model, nor where
/// it was looking, nor what to do — so this is the one the message has to
/// answer.
fn load_error(model: &str, cache_dir: &Path, detail: &str) -> Error {
    Error::Provider(format!(
        "local embeddings: could not load model {model} from cache {} ({detail}); the first \
         load downloads the model from HuggingFace and needs network access — check \
         connectivity (or HF_ENDPOINT if you use a mirror), and note that HF_HOME, if set, \
         overrides the configured cache directory",
        cache_dir.display()
    ))
}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Short-circuit before the model is touched: an empty batch tokenises
        // to nothing, and `fastembed` reports that as an error rather than an
        // empty result.
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let session = Arc::clone(&self.session);
        let batch_size = self.batch_size;
        let model = self.model.clone();
        let owned = texts.to_vec();

        // Inference is CPU-bound and blocking. `spawn_blocking` keeps it off
        // the async worker threads; the lock lives entirely inside the closure,
        // so it is never held across an await.
        tokio::task::spawn_blocking(move || {
            let mut guard = session.lock().map_err(|_| {
                Error::Provider(format!(
                    "local embeddings: the {model} session is poisoned — a previous embedding \
                     panicked; restart the process"
                ))
            })?;
            guard.embed(&owned, batch_size).map_err(|e| {
                Error::Provider(format!(
                    "local embeddings: {model} failed to embed {} text(s): {}",
                    owned.len(),
                    describe(&e)
                ))
            })
        })
        .await
        .map_err(|e| Error::Provider(format!("local embeddings: inference task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_model_resolves_and_is_bge_small() {
        let config = LocalEmbedderConfig::new(DEFAULT_LOCAL_MODEL)
            .expect("the documented default must be a real catalogue entry");
        assert_eq!(config.huggingface_code(), "Xenova/bge-small-en-v1.5");
        // fs2 settled on 384 for its local mode; a change here silently
        // invalidates every vector already in a store.
        assert_eq!(config.dimensions(), 384);
    }

    #[test]
    fn a_model_can_be_named_by_the_code_fastembed_pulls_from() {
        // `fastembed` fetches the ONNX export, not the original weights, so
        // the code it reports is a re-publisher's.
        let by_code = LocalEmbedderConfig::new("Xenova/bge-small-en-v1.5")
            .expect("the exact HuggingFace code must resolve");
        let by_name = LocalEmbedderConfig::new("BGESmallENV15").expect("catalogue name resolves");
        assert_eq!(by_code.huggingface_code(), by_name.huggingface_code());
    }

    #[test]
    fn a_model_can_be_named_the_way_fs2_and_everyone_else_names_it() {
        // The single most likely string in a migrated config. It is NOT the
        // code `fastembed` reports — `BAAI` publishes the weights, `Xenova`
        // publishes the ONNX export — and rejecting it would be rejecting a
        // name that is plainly correct.
        let migrated = LocalEmbedderConfig::new("BAAI/bge-small-en-v1.5")
            .expect("the upstream publisher's code must resolve to the same model");
        assert_eq!(migrated.huggingface_code(), "Xenova/bge-small-en-v1.5");
        assert_eq!(migrated.dimensions(), 384);

        // …and with no account at all.
        let bare = LocalEmbedderConfig::new("bge-small-en-v1.5").expect("a bare id must resolve");
        assert_eq!(bare.huggingface_code(), "Xenova/bge-small-en-v1.5");
    }

    #[test]
    fn model_names_are_case_insensitive_and_tolerate_surrounding_space() {
        assert!(LocalEmbedderConfig::new("bgesmallenv15").is_ok());
        assert!(LocalEmbedderConfig::new("  baai/BGE-Small-EN-v1.5  ").is_ok());
    }

    #[test]
    fn an_ambiguous_bare_id_is_refused_rather_than_guessed() {
        // `fastembed` maps both EmbeddingGemma variants onto ONE HuggingFace
        // code, and its catalogue is a `HashMap`: first-match would pick the
        // full-precision or the Q4 model depending on hash iteration order.
        let error = LocalEmbedderConfig::new("onnx-community/embeddinggemma-300m-ONNX")
            .expect_err("a code shared by two entries must not silently pick one");
        let message = error.to_string();
        assert!(
            message.contains("EmbeddingGemma300M") && message.contains("EmbeddingGemma300MQ4"),
            "the error must name every candidate so the caller can choose: {message}"
        );
    }

    #[test]
    fn an_unknown_model_error_lists_the_valid_names() {
        let error = LocalEmbedderConfig::new("bge-tiny-imaginary")
            .expect_err("an invented model must not resolve");
        let message = error.to_string();
        assert!(
            message.contains("bge-tiny-imaginary"),
            "the error must quote what was asked for: {message}"
        );
        // A list that does not name the default is a list that cannot be acted
        // on: the reader's next move is to pick one.
        assert!(
            message.contains(DEFAULT_LOCAL_MODEL),
            "the error must name the default as a starting point: {message}"
        );
        assert!(
            message.contains("AllMiniLML6V2"),
            "the error must enumerate the catalogue: {message}"
        );
    }

    #[test]
    fn the_catalogue_order_is_stable_across_calls() {
        // `fastembed` hands its catalogue back from a `HashMap`. An error
        // message built from an unsorted list would differ run to run, which
        // makes it unquotable in a bug report.
        let names = |models: Vec<LocalModelInfo>| {
            models.into_iter().map(|info| info.name).collect::<Vec<_>>()
        };
        assert_eq!(names(supported_models()), names(supported_models()));
        let sorted = {
            let mut copy = names(supported_models());
            copy.sort();
            copy
        };
        assert_eq!(names(supported_models()), sorted);
    }

    #[test]
    fn the_default_cache_directory_is_outside_any_repository() {
        let dir = default_cache_dir().expect("HOME is set in any test environment");
        assert!(
            dir.is_absolute(),
            "cache path must not be relative: {dir:?}"
        );
        assert!(
            dir.ends_with("flowspace3/models"),
            "cache path must be namespaced: {dir:?}"
        );
        // The trap this whole function exists to close: `fastembed`'s own
        // default is `./.fastembed_cache`, which lands in whatever repository
        // the daemon is scanning.
        assert!(
            !dir.starts_with(std::env::current_dir().expect("a working directory")),
            "the model cache must never sit under the working directory: {dir:?}"
        );
    }

    #[test]
    fn the_catalogue_is_not_empty_and_reports_widths() {
        let models = supported_models();
        assert!(models.len() > 5, "the catalogue should list real models");
        assert!(
            models.iter().all(|info| info.dimensions > 0),
            "every catalogue entry must declare a vector width"
        );
        assert!(
            models.iter().any(|info| info.name == DEFAULT_LOCAL_MODEL),
            "the default model must appear in the catalogue it is chosen from"
        );
    }

    #[test]
    fn a_load_failure_names_the_model_the_cache_and_the_fix() {
        let error = load_error(
            "BGESmallENV15",
            Path::new("/tmp/fs3-models"),
            "ConnectionRefused",
        );
        let message = error.to_string();
        for expected in [
            "BGESmallENV15",
            "/tmp/fs3-models",
            "ConnectionRefused",
            "network",
            "HF_HOME",
        ] {
            assert!(
                message.contains(expected),
                "a load failure must mention {expected:?}: {message}"
            );
        }
    }

    #[test]
    fn a_cause_chain_is_flattened_rather_than_dropped() {
        // The shape that actually bit: `fastembed`'s Display says only
        // "Failed to retrieve model file 'onnx/model.onnx'" and hides
        // "Connection refused" behind `#[source]`, so `{e}` reports a failure
        // with no cause. Reproduced here with a two-deep chain of our own.
        #[derive(Debug)]
        struct Layer(&'static str, Option<Box<Layer>>);
        impl std::fmt::Display for Layer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }
        impl std::error::Error for Layer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.1
                    .as_deref()
                    .map(|inner| inner as &dyn std::error::Error)
            }
        }

        let deep = Layer(
            "failed to retrieve model file",
            Some(Box::new(Layer(
                "http error",
                Some(Box::new(Layer("connection refused", None))),
            ))),
        );
        assert_eq!(
            describe(&deep),
            "failed to retrieve model file: http error: connection refused"
        );
        assert_eq!(describe(&Layer("alone", None)), "alone");
    }
}
