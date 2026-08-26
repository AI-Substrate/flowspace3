# Local embeddings (fastembed / ONNX in-process)

**What it is.** An `Embedder` that runs a sentence-transformer model inside the
fs3 process — no API, no key, no server, no Docker. The model is pulled once
from HuggingFace and cached on disk; after that it is entirely offline. This is
the air-gapped option, and it is the one that makes the provider contract
suite runnable in CI for free.

Code: `crates/providers/src/local.rs` · contract leg:
`crates/providers/tests/local_contract.rs` · roster row:
`docs/plans/prd/providers-roster.md`.

```rust
let config = LocalEmbedderConfig::new(DEFAULT_LOCAL_MODEL)?; // "BGESmallENV15"
let embedder = LocalEmbedder::load(config)?;                 // blocking; may download
let vectors = embedder.embed(&["hello".to_string()]).await?; // 384 floats
```

## What you get, measured

Apple M4 Max, release build, `BAAI/bge-small-en-v1.5`:

| | |
|---|---|
| dimensions | **384** (declared and actual) |
| output | L2-normalised unit vectors — cosine is a dot product |
| pooling | CLS (BGE's requirement; mean pooling on BGE is *silently wrong*) |
| max sequence length | 512 tokens |
| first load (download + session) | ~19 s, **129 MB** on disk |
| load from warm cache, network unplugged | **79–100 ms** |
| 256 one-line code snippets | 321 ms (~800 texts/sec) |
| 64 × ~400-token code chunks | 1.7 s (~37 chunks/sec) |
| `cos(cat, kitten)` vs `cos(cat, carburetor)` | **0.876** vs **0.580** |
| binary growth | ~32 MB (ONNX Runtime is statically linked) |

## Key decisions

**fastembed, not hand-rolled ORT.** fs2 hand-wrote the ONNX session, the
tokenizer wiring, the pooling-config lookup and the L2 normalisation in Python
(`embedding_adapter_onnx.py`, ~290 lines) and left a workshop note about
getting BGE's pooling wrong. `fastembed` already owns all four, keyed off its
model catalogue, so none of that is fs3's code to get wrong.

**BGE-small as the default.** Same model fs2 defaults to, so vectors from
either system are comparable, and 384 dimensions is what fs2's config validator
forces for local mode. `LocalEmbedderConfig::dimensions()` reports the width
*before* anything is embedded — the store's vector column is fixed-width, and
fs2 had to retrofit a mismatch guard after shipping.

**`Mutex` + `spawn_blocking`.** `fastembed::TextEmbedding::embed` takes
`&mut self`, and the port is `Arc<dyn Embedder>` with `&self` — hence the
mutex. Inference is CPU-bound for hundreds of milliseconds, so it goes to
`tokio::task::spawn_blocking` rather than stalling an async worker thread. The
lock is taken *inside* the blocking closure and never held across an await.
This makes `tokio` a shipped dependency of `fs3-providers` — the only leaf
crate that ships it. Core stays tokio-free; the arch check enforces that.

**CPU only — and that is a finding, not a shrug.** `fastembed` 6.0.1 exposes no
CoreML or CUDA execution-provider feature of its own (its `metal`/`cuda`
features only wire the candle backend for qwen3/nomic; `directml` is
Windows-only). `with_intra_threads(n)` caps ONNX Runtime's thread count — the
knob that matters on a laptop, where an all-cores batch makes everything else
stutter.

GPU is nonetheless *reachable*, and the recipe is worth writing down because it
is not obvious: `fastembed` delegates provider selection to `ort`, so adding
`ort` as a direct dependency at the **exact** version `fastembed` pins
(`=2.0.0-rc.13`, so cargo unifies rather than duplicating) and passing an EP
through `with_execution_providers` works without `fastembed` knowing anything
about it. Verified present in the vendored `ort` source, not merely claimed:
`ort/src/ep/coreml.rs` has `ComputeUnits`, `CoreML`, and
`with_compute_units(…)`; the feature is `coreml = ["ort-sys/coreml"]`.

```rust
// ort = { version = "=2.0.0-rc.13", features = ["coreml"] }
use ort::ep::coreml::{ComputeUnits, CoreML};

TextInitOptions::new(model).with_execution_providers(vec![
    CoreML::default()
        .with_compute_units(ComputeUnits::CPUAndNeuralEngine)
        .into(),
])
```

**We are not taking it, on evidence.** For BERT-class encoders this size the
CoreML EP is a coin flip, not a win: it partitions the graph and falls back to
CPU for unsupported ops, pays a first-run model-compilation cost that is
proportionally *worse* for small graphs, and is documented as regressing below
the CPU EP outright when a model lands on the older NeuralNetwork format rather
than MLProgram. Published embedding benchmarks put ONNX-Runtime-CPU engines
(`fastembed` among them) at the front for `bge-small-en-v1.5`, with
candle-backed TEI roughly an order of magnitude behind on the same model. So
the CPU path is not the fallback here; it is the fast one. Revisit only with a
measurement on this workload, and measure first-run and steady-state
separately.

**rustls, no image models.** `default-features = false` plus
`hf-hub-rustls-tls` and `ort-download-binaries-rustls-tls`. The crate defaults
to native-tls (contradicting the workspace's `reqwest` choice) and pulls
`image-models`, which a text embedder never uses.

**The contract leg is `slow`, not `keyed`.** Every other adapter's contract run
is hidden because it needs credentials. This one needs none — it is free, and
anyone can run it — but it loads a real model, and `cargo test` must stay fast
and offline everywhere. So it is `#[ignore = "slow: …"]` and one flag away,
rather than unrunnable for want of an account. The distinction is the reason
the tier is in the reason string.

**The enrichment key is `catalogue-name@dimensions`.** Not the HuggingFace
code: `fastembed` maps `EmbeddingGemma300M` and `EmbeddingGemma300MQ4` onto one
code at one width, and keying on it would file two different vector spaces
under one enrichment row. Catalogue names are unique per model file. Pinned by
`the_enrichment_key_discriminator_cannot_collide_two_vector_spaces`.

## Gotchas — the expensive ones

**`fastembed` caches into your repository by default.** `get_cache_dir()` is
`.fastembed_cache` *relative to the current working directory*, and the daemon's
working directory is somebody's source tree. The adapter therefore **always**
passes `with_cache_dir` explicitly. The real resolution order is:

```
HF_HOME  >  with_cache_dir(…)  >  FASTEMBED_CACHE_DIR  >  ./.fastembed_cache
```

Note the inversion: **`HF_HOME` overrides the explicit configured directory**,
which is backwards from every other config-beats-environment convention in fs3.
If a machine exports `HF_HOME`, models land there no matter what fs3 is
configured with. Default without it: `~/.cache/flowspace3/models`, on every
platform.

**The HuggingFace code is not the name anyone uses.** `fastembed` pulls ONNX
exports from re-publishers, so `BGESmallENV15` reports its code as
`Xenova/bge-small-en-v1.5` — not `BAAI/bge-small-en-v1.5`, which is what fs2's
config says and what a human writes. `LocalEmbedderConfig::new` therefore
accepts three forms: catalogue name, exact code, or the bare model id after the
`/`. An id matching several entries is an error naming all of them, never a
silent first-match — `fastembed` really does map both `EmbeddingGemma300M` and
`EmbeddingGemma300MQ4` onto one code.

**Concurrent cold loads collide.** Three tests loading the same missing model
in parallel means three simultaneous downloads into one cache directory; two of
them fail. Load once and share the handle (`LocalEmbedder` clones are handle
clones). This is what the contract test's `LazyLock` is for, and what a
composition root does anyway.

**`fastembed`'s catalogue is a `HashMap`.** `list_supported_models()` returns a
different order every run, so anything built from it — an error message, a
`--list-models` output — must sort. `supported_models()` does.

**Its errors hide their cause.** `Display` for the common failure is
`Failed to retrieve model file 'onnx/model.onnx'`; the "connection refused"
part sits behind `#[source]`, and `{e}` drops it. The adapter walks the chain.

**Avoid the `…Q` (dynamically quantised) models.** They recompute their
quantisation range per batch, so `fastembed` refuses any `batch_size` smaller
than the input — and the same text embedded alone versus in a batch may drift
further than the contract's `SAME_EMBEDDING` floor tolerates. Untested here.

**First run needs network and ~129 MB.** A cold
`cargo test -p fs3-providers --test local_contract -- --ignored` on a machine
with no network fails rather than skipping. That is deliberate: a green test
that silently did nothing is worse than a red one.

**Do not normalise again downstream.** Vectors come back L2-normalised, so
cosine similarity is a plain dot product and re-normalising is a no-op at best.
The trap runs the other way too: comparing these vectors against a naive
`ort` + `tokenizers` stack that skipped normalisation looks like "the
embeddings are wrong" when it is only a magnitude difference.

## Alternatives, and why they were not chosen

Checked after the fact, because the choice deserved a second opinion:

- **`ort` + `tokenizers` directly** — what `fastembed` already is, minus the
  model catalogue. It gives you raw hidden states: pooling and normalisation
  become your code, and picking mean where the model wants CLS is *silently*
  wrong, not an error. fs2 wrote that code and recorded getting exactly this
  wrong. No.
- **`candle`** (HuggingFace's pure-Rust framework) — genuinely pure Rust and
  the only option with a real Metal story, but you assemble BERT, pooling and
  normalisation yourself, and published benchmarks put candle-backed embedding
  of this very model roughly an order of magnitude behind ONNX Runtime CPU.
  Its own issue tracker carries "same embedding for every sentence" reports
  from hand-rolled pooling. Not for a CPU encoder.
- **`model2vec-rs`** (static distilled embeddings — no transformer at
  inference) — the one genuinely interesting alternative: order-of-magnitude
  faster on CPU for a few MTEB points of retrieval quality. Worth measuring if
  bulk first-index throughput ever becomes the bottleneck, as a *second*
  embedder rather than a replacement. The quality/speed figures available are
  estimates, not measurements — treat them as a reason to benchmark, not a
  reason to switch.
- **`llama-cpp-2` / GGUF, `mistral.rs`, `burn`, `tract`** — all run *some*
  embedding model in-process, none improve on ONNX Runtime for a 33M-parameter
  encoder, and each adds a build story. No.
- **HuggingFace `text-embeddings-inference`** — a server. The entire point of
  this adapter is not being one.

One correction worth recording, because a secondary source got it wrong:
`fastembed` does **not** apply mean pooling universally. It selects pooling per
model (`get_default_pooling_method`), defaulting to CLS — which is what makes
it correct for BGE out of the box. Verified in the crate source, and consistent
with the `cos(cat, kitten) = 0.876` measured above; mean-pooled BGE is the
failure fs2 documented.

## How to verify it works

The contract leg is `#[ignore]`d on the **slow** tier. Repo convention is two
tiers, and the reason string is mandatory and names which one:

| Tier | Reason string | Means |
|---|---|---|
| keyed | `keyed: <env vars>` | needs credentials you may not have |
| slow | `slow: <why>` | free to run, but too expensive for the default path |

This suite is free — no account, no key, nobody's quota — but it loads a real
ONNX model, so it stays out of `cargo test` / `harness checks`, which must be
fast and offline on every machine. `-- --ignored` is the whole difference.

```bash
# offline, keyless, no download, runs by default — name resolution, cache
# placement, key discriminator, error text
cargo test -p fs3-providers --lib local

# the real thing: the shared Embedder contract suite against a real model
#   ~18 s cold (downloads ~129 MB), ~0.2 s warm
cargo test -p fs3-providers --test local_contract -- --ignored

# a different model, and/or a different cache
FS3_LOCAL_MODEL=AllMiniLML6V2 \
FS3_LOCAL_MODEL_CACHE=/tmp/fs3-models \
  cargo test -p fs3-providers --test local_contract -- --ignored

# prove the offline claim: warm cache, unreachable hub
HF_ENDPOINT=http://127.0.0.1:1 \
  cargo test -p fs3-providers --test local_contract -- --ignored

# prove nothing landed in the repo
git status --porcelain | grep fastembed   # must print nothing
```

### What it costs on a real file

`cargo run --release -p fs3-providers --example embed_file -- <path> [query]`
points the shipped adapter at any file, chunks it, embeds it, and prints where
the time went. Measured on an M4 Max against a 57 KB / 7 655-word markdown file
(29 chunks):

| Phase | Cold cache | Warm cache |
|---|---|---|
| read + chunk | 0.9 ms | 0.2 ms |
| **load model** | **18 221 ms** | **93 ms** |
| embed 29 chunks | 746 ms | 744 ms |
| **total** | **18 968 ms** | **838 ms** |
| load as share of total | 96.1 % | 11.1 % |

Two things that shape how you use it. First, the cold number is a *once per
machine* 129 MB download, not a per-run cost — after it, loading is 93 ms.
Second, load amortises away completely: over every markdown file in this repo
concatenated (1.36 MB, 730 chunks) the same run spends 185 ms loading and
23.3 s embedding — **0.8 % load**, ~8 400 words/sec sustained. Per-chunk cost
is flat at ~26–32 ms, so a scan's embedding budget is essentially
`chunks × 30 ms` on one core, and the model load is a rounding error on
anything bigger than a single file.

Passing a query ranks the chunks by cosine, which is how you check the vectors
mean something rather than merely existing:

```text
query : "how do I score adaptability and operate-today"
  0.6923  #2   - onboard a repo; - assess repo onboarding difficulty; …
  0.6844  #9   In addition to the two-axis tuple, v0.2 emits an assessment matrix…
```

## Snap-in

Not wired. `crates/providers/src/local.rs` carries the recipe in its module
docs: one `ProviderConfig::Local { model, cache_dir, intra_threads }` variant
and one composition-root match arm. Wiring happens at adoption, by the
integrating stream — note that `LocalEmbedder::load` blocks and may download,
so a composition root that runs inside an async context must `spawn_blocking`
it.

There is no local `Summarizer`: generation needs a different class of model,
and the roster tracks it separately.
