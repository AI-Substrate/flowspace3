# fs3 provider roster
**Status doc** — which Embedder/Summarizer adapters exist, are in flight, or are wanted. The two ports are frozen (workshop 001); every adapter is a `fs3-providers` module + offline stub-server tests + a `#[ignore]`d keyed contract run + a snap-in recipe (one `ProviderConfig` variant + one composition-root match arm, wired only at adoption). The shared contract suite in `fs3-testkit` is the done-bar.

| Adapter | Ports | Status | Notes |
|---|---|---|---|
| fake (testkit) | both | ✅ landed (s001) | deterministic feature-hash embeddings; CI baseline |
| OpenAI | both | ✅ landed (s001) | api-key auth, `api_base` overridable. Structured summaries: `response_format: json_schema` (`strict`) with a remembered downgrade to `json_object` for compat servers that reject it. |
| **Azure OpenAI** | **both** | ✅ landed (w-azure) | api-key AND Entra auth (`azure_identity`, `az login`/managed identity); deployment-URL + `api-version` scheme; optional `dimensions`. Structured summaries with the same schema and downgrade (older `api-version`s predate it). Offline stub-server tests + keyed contract run green against a live resource (Entra; that resource has key auth disabled). Snap-in recipe on the module; core/daemon untouched. |
| OpenAI-compatible generic | both | wanted | `base_url` + auth mode; covers Ollama-compat, LM Studio, vLLM, llama.cpp server |
| Ollama native | both | wanted | local models, native protocol |
| Anthropic | summarizer | wanted | native messages API |
| Gemini | both | wanted | native API |
| Voyage / Cohere | embedder | wanted | dedicated embedding APIs; pick one when needed |
| **fastembed/ONNX in-process** | **embedder** | ✅ landed (w-local-embed) | serverless local embeddings; air-gapped + test-friendly. `BAAI/bge-small-en-v1.5` by default (384-dim, CLS pooling, L2-normalised, ~129 MB) — the model fs2 settled on. ONNX Runtime is statically linked, so there is nothing to install; the only network is the first model pull. Contract leg needs no credentials — it is `#[ignore]`d on the **slow** tier, not the keyed one: `cargo test -p fs3-providers --test local_contract -- --ignored` (18 s cold, 0.2 s warm). Config = model name + cache dir + intra-op threads. CPU only. |
| Concurrency combinator | wraps any | wanted | `Batched`/`Throttled`/`Retry` over `Arc<dyn>`; the parallel-execution layer, written once for all adapters |

**Cross-cutting, landed with structured summaries (w-structured-summaries, 2026-08-26):** both ports expose `key()` — the enrichment row key, `model@prompt_version` for summarizers and `model@dimensions` for embedders — so a prompt, schema, model or width change is a new key rather than a migration. `fs3_core::Summary` carries `extras` (`serde(flatten)`), so a field a future prompt learns to extract is captured instead of dropped. The prompt and schema are versioned by `OpenAiSummarizer::PROMPT_VERSION`, which the Azure adapter shares because it shares both.

Adoption order beyond Azure is unset — Jordan names the next pick. Update this table when an adapter lands or a decision changes it.

## Deployment policy (Jordan, 2026-08-26)
Machine-wide default = **Azure** (summarizer azure chat deployment, embedder text-embedding-3-small@1024). Per-repo registry overrides route PRIVATE repos fully on-network: local ONNX embeddings (bge-small@384) + the LAN openai-compat LLM. Mixed dims are safe by construction (per-dim tables, model_key-scoped rows); a repo queries with the same embedder it indexed with via the per-repo lookup, and switching an override re-enriches via the reconciler (new key = missing rows), old rows kept for rollback.
