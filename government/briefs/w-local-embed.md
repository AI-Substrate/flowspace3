# Worker brief — local embeddings validation + adapter · (seat at canary, pane %42)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · one bounded task, two stages

## The job
Jordan: fs2 "has the ability to use local embedding models and it pulls them from hugging face … validate that we can use a little local model like it does to do embeddings as an option."

### Stage 1 — validate (POC, throwaway, scratch dir `.harness/temp/w-local-embed/**`)
1. Mine fs2 read-only (`/Users/jordanknight/substrate/fs2/flow_squared`): its onnx/local embedding mode — which model it defaults to (bge-small / all-MiniLM class), how it pulls from HuggingFace, dimensions (384?), device handling.
2. Prove the Rust path: the **fastembed** crate (ort + hf-hub under the hood — lib-reuse rule, do NOT hand-roll ONNX sessions unless fastembed is genuinely unfit; if unfit, ort + tokenizers directly and say why). Small throwaway bin: download a small model from HF, embed a handful of texts, print dimensions + a similarity sanity check ("cat"~"kitten" > "cat"~"carburetor"), timings (model download cold, embed warm), disk footprint, offline behaviour on second run (cache hit — where does the model cache live? It must respect our zero-repo-footprint rule: cache under `~/.cache` or `~/.config/flowspace3/models`, NEVER in a repo).
3. GO/NO-GO verdict with numbers.

### Stage 2 — on GO: land the adapter (the roster row "fastembed/ONNX in-process")
Follow **`.agents/skills/add-provider/SKILL.md`** exactly — Embedder port only. Bonus of local: the CONTRACT LEG RUNS LIVE FOR FREE (no keys) — so unlike the cloud adapters, run the full contract suite un-ignored if runtime cost permits (< ~60s), else `#[ignore]`d with the run documented AND executed once in your report. Config shape: model name + optional cache dir + device. Flip the roster row; write `docs/services/local-embeddings.md`.

## Rules & fence
- Architecture: `docs/rules-idioms-architecture/fs3-architecture.md`; allowlist rows only for your real deps (fastembed/ort); model downloads NEVER into any repo.
- Fence: stage 1 scratch only; stage 2 = the add-provider file set + roster + your service page.
- Commit+push per unit, FILE-scoped adds for shared files, push-first (ruling 2026-08-26-commit-push-as-you-go.md).
- Gates: `harness checks` + `cargo test -p fs3-providers`. Report to pij-instant-lynx: stage-1 numbers · stage-2 files/gates · observations (esp. anything the add-provider skill got wrong for a LOCAL provider — it was written for HTTP APIs).
