# OpenAI-compatible provider (generic)
**Built**: 2026-08-26 (worker pij-sure-kazimir, w-structured-summaries unit 2) · **Code**: `crates/providers/src/openai_compat.rs` (+ `tests/openai_compat_{stub,contract}.rs`)

`Summarizer` only, against any server that speaks OpenAI's `/chat/completions`: `llama.cpp`'s server, Ollama's compat endpoint, vLLM, LM Studio. One `base_url`, an optional key, a token budget.

The wire format is OpenAI's. What differs is the assumption set, and every difference below cost something to learn.

## Key decisions
- **Summarizer only, and it says so at wiring time.** The reference endpoint answers `/v1/embeddings` with `501`. An embedder configured against one of these gets `embeddings_unsupported(base_url)` — a refusal that names the endpoint *and* points at the in-process local embedder — rather than discovering it at the first batch.
- **The served model is discovered, not configured.** These servers ignore the `model` field and serve whatever is loaded, so `connect()` reads `/v1/models` and keeps the id the server reports. A configured name would key enrichment rows by a wish.
- **`key()` = `served_model@prompt_version`.** The served id names the weights *and usually the quantisation* — `/models/Qwen3.8-27B-ABLITERATED-Q5_K_M.gguf@1`. That is exactly what changes when the box is switched to another mode, so the key defends against silently mixing two models' summaries in one column.
- **`connect()` is also the readiness probe.** `llama.cpp` serves `/v1/models` only once weights are in memory, so a successful connect means the next request will be answered rather than hang. Poll it; never sleep and hope.
- **`max_tokens` defaults to 4000**, not to a number sized for a summary. See the gotcha below — this default *is* the mitigation.
- Structured outputs are attempted exactly as for OpenAI and Azure, with the same remembered downgrade. `llama.cpp` accepts them: it compiles the schema into a sampling grammar, which is stricter than either cloud enforces.

## Gotchas learned
- **An empty answer arrives as a success.** On a reasoning model the chain of thought goes to `reasoning_content`, the answer to `content`, and *both come out of one `max_tokens` budget*. Too small a budget returns HTTP 200, `finish_reason: "length"`, `content: ""` and **no error at all**. Measured directly: `max_tokens: 50` → `completion_tokens: 50`, all of it thinking, empty content. Returned as-is that would write blank enrichment rows that look successful for ever, so the adapter refuses it by name and tells you to raise the budget. There is an `#[ignore]`d live test that fails if the endpoint ever stops doing this — the workaround should not outlive the quirk.
- Whitespace-only content is refused the same way, and the refusal distinguishes `finish_reason: "length"` (budget) from `"stop"` (the model genuinely produced nothing), because those are different diagnoses.
- **LAN-only, no auth.** The reference endpoint is reachable only on-network and checks no key. Sending a placeholder bearer to a server that *does* check would fail in a way that reads like a bad key, so no key configured means no `authorization` header at all.
- **One model at a time.** The box serves a single model, so switching modes silently changes the weights and the quant underneath a running fs3. `key()` defends the *stored rows* — a different model yields a different key — but only as of the last `connect()`: a switch **during** a run is invisible to a key resolved before it happened. Reconnect when the box changes mode. This is a real limit, not a solved problem.
- `/v1/models` returning an empty list means the server is up but still loading. That is a distinct, actionable state and the adapter names it.

## Verify
Offline, no server, no network — this is what CI runs:
```bash
cargo test -p fs3-providers      # 10 stub tests for this adapter; the live leg stays ignored
```

Live, against a real endpoint:
```bash
export FS3_OPENAI_COMPAT_BASE_URL=http://192.168.1.134:8080/v1
# optional: FS3_OPENAI_COMPAT_API_KEY, FS3_OPENAI_COMPAT_MAX_TOKENS
cargo test -p fs3-providers --test openai_compat_contract -- --ignored --nocapture
```

Observed 2026-08-26 against `Qwen3.8-27B-ABLITERATED-Q5_K_M` on `192.168.1.134:8080`, both tests green in 11.6 s:

```
served model: /models/Qwen3.8-27B-ABLITERATED-Q5_K_M.gguf
row key:      /models/Qwen3.8-27B-ABLITERATED-Q5_K_M.gguf@1
text: A public Rust struct that represents a single element by its name. It exposes one
      public field, `name: String`, with no methods, constructors, or validation in the
      shown definition. It functions as a minimal, transparent data container for element
      identity within the core module.
tags: ["Rust struct", "data model", "element name", "public field", "core module"]
refusal (max_tokens 50): … returned an EMPTY summary with no error — the reply hit the 50
      token budget before it produced any answer. On a reasoning model the thinking and the
      answer share max_tokens, so raise it …
```

## Code pointers
- `crates/providers/src/openai_compat.rs` — `OpenAiCompatConfig` (builder: `with_model`, `with_api_key_from_env`, `with_max_tokens`), `OpenAiCompatSummarizer::connect`, `served_model()`, `embeddings_unsupported()`. The empty-answer refusal is in `summary_from`.
- `crates/providers/tests/openai_compat_stub.rs` — offline proof, including the empty-answer and no-model-loaded paths.
- `crates/providers/tests/openai_compat_contract.rs` — the live leg; its header is the canonical env-var list.
- The prompt, the JSON schema and `PROMPT_VERSION` are shared with `OpenAiSummarizer` — one prompt, one version, no drift.

## Snap-in
Not yet wired into the composition root. The `ProviderInstance::OpenAiCompat` variant and both match arms — the summarizer arm *and* the embedder arm that must refuse — are a doc comment on the module. Config kind: `openai-compat`.
