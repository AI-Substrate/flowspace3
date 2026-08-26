# fs3 provider roster
**Status doc** — which Embedder/Summarizer adapters exist, are in flight, or are wanted. The two ports are frozen (workshop 001); every adapter is a `fs3-providers` module + offline stub-server tests + a `#[ignore]`d keyed contract run + a snap-in recipe (one `ProviderConfig` variant + one composition-root match arm, wired only at adoption). The shared contract suite in `fs3-testkit` is the done-bar.

| Adapter | Ports | Status | Notes |
|---|---|---|---|
| fake (testkit) | both | ✅ landed (s001) | deterministic feature-hash embeddings; CI baseline |
| OpenAI | both | ✅ landed (s001) | api-key auth, `api_base` overridable |
| **Azure OpenAI** | **both** | ✅ landed (w-azure) | api-key AND Entra auth (`azure_identity`, `az login`/managed identity); deployment-URL + `api-version` scheme; optional `dimensions`. Offline stub-server tests + keyed contract run green against a live resource (Entra; that resource has key auth disabled). Snap-in recipe on the module; core/daemon untouched. |
| OpenAI-compatible generic | both | wanted | `base_url` + auth mode; covers Ollama-compat, LM Studio, vLLM, llama.cpp server |
| Ollama native | both | wanted | local models, native protocol |
| Anthropic | summarizer | wanted | native messages API |
| Gemini | both | wanted | native API |
| Voyage / Cohere | embedder | wanted | dedicated embedding APIs; pick one when needed |
| fastembed/ONNX in-process | embedder | wanted | serverless local embeddings; air-gapped + test-friendly |
| Concurrency combinator | wraps any | wanted | `Batched`/`Throttled`/`Retry` over `Arc<dyn>`; the parallel-execution layer, written once for all adapters |

Adoption order beyond Azure is unset — Jordan names the next pick. Update this table when an adapter lands or a decision changes it.
