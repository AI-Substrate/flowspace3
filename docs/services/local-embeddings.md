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

**CPU only.** `fastembed` 6.0.1 exposes no CoreML or CUDA execution-provider
feature (its `metal`/`cuda` features only wire the candle backend for
qwen3/nomic; `directml` is Windows-only). `with_intra_threads(n)` caps ONNX
Runtime's thread count — the knob that matters on a laptop, where an
all-cores batch makes everything else stutter. GPU means depending on `ort`
directly to build an `ExecutionProviderDispatch`; that is the extension point,
deliberately not taken.

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

**First run needs network and ~129 MB.** A cold `cargo test --all` on a
machine with no network fails this suite rather than skipping it. That is
deliberate: a green test that silently did nothing is worse than a red one.

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

## Snap-in

Not wired. `crates/providers/src/local.rs` carries the recipe in its module
docs: one `ProviderConfig::Local { model, cache_dir, intra_threads }` variant
and one composition-root match arm. Wiring happens at adoption, by the
integrating stream — note that `LocalEmbedder::load` blocks and may download,
so a composition root that runs inside an async context must `spawn_blocking`
it.

There is no local `Summarizer`: generation needs a different class of model,
and the roster tracks it separately.
