# OpenAI-compatible provider (generic)
**Built**: 2026-08-26; extended for configured hosted endpoints and OpenRouter 2026-08-28 · **Code**: `crates/providers/src/openai_compat.rs`

One OpenAI wire shape serves the existing `Embedder`, `Summarizer`, and `ChatProvider` ports. OpenRouter is configuration, not a provider-specific adapter: `base_url = "https://openrouter.ai/api/v1"`, a configured `model`, and `api_key_env = "OPENROUTER_API_KEY"`. No OpenRouter-only attribution or routing headers are sent.

## Two endpoint postures

- **Hosted/multi-model**: configuration chooses the model. `OpenAiCompatEmbedder`, `OpenAiCompatSummarizer::configured`, and `OpenAiCompatChatClient` send that id and use it in their keys. This is the OpenRouter posture.
- **Single-model LAN**: `OpenAiCompatSummarizer::connect` still discovers the loaded model from `/models`; that id, including weights or quantisation, becomes the summary key. This preserves the llama.cpp/Ollama/vLLM/LM Studio readiness and identity contract.

The config registry uses the hosted posture because one provider entry deliberately names one model. To use one account with multiple models, declare multiple entries sharing `base_url` and `api_key_env`:

```toml
[providers.openrouter-chat]
kind = "openai_compat"
base_url = "https://openrouter.ai/api/v1"
model = "z-ai/glm-5.3-flash"
api_key_env = "OPENROUTER_API_KEY"
max_tokens = 4000

[providers.openrouter-embed]
kind = "openai_compat"
base_url = "https://openrouter.ai/api/v1"
model = "openai/text-embedding-3-small"
api_key_env = "OPENROUTER_API_KEY"
dimensions = 1024

[agent]
active = "openrouter-chat"
[embedder]
active = "openrouter-embed"
[summarizer]
active = "azure-chat"
```

`flowspace3 config show` renders each surface → provider → kind/model mapping. Surface selection remains independent; using OpenRouter for `ask` does not move summaries or embeddings.

## Credentials and failure posture

The config stores only the environment variable name. The value belongs in `~/.config/flowspace3/secrets.env`:

```text
OPENROUTER_API_KEY=…
```

The CLI loads that file before configuration is resolved. A selected entry whose named variable is absent fails during composition-root wiring, naming the provider, variable, and `secrets.env`; no request is attempted. Secret values are wrapped in a redacting type and never enter `Debug`, logs, envelopes, or `config show`. Keyless LAN endpoints omit `api_key_env`.

## Vector-space and usage invariants

- `dimensions`, when configured, is sent in `/embeddings`, checked against every returned vector, and included in `model_key` as `model@dimensions`. A mismatched response is refused before storage.
- `/embeddings` response indices are treated as a mapping: missing, duplicate, and out-of-range indices fail.
- `ChatProvider` maps `usage.total_tokens` to `ChatTurn.tokens_used`. A missing usage object remains `None`, never fabricated as zero.
- Real OpenRouter usage activates the existing `[agent] token_budget` bound. A run can now stop at the configured budget where a provider that reports unknown usage cannot.

## Summary compatibility

Structured output is attempted first and a schema rejection downgrades once to `json_object`. Every path still validates the summary. Reasoning models may spend `max_tokens` on `reasoning_content` and return HTTP 200 with empty `content`; empty or whitespace-only summaries are named failures, not stored enrichment.

## Verify

Offline recorded OpenAI-shape contracts:

```bash
cargo test -p fs3-providers --test openai_compat_stub
```

Live OpenRouter chat and embedding receipts, one command; the key is read but never printed:

```bash
set -a; source ~/.config/flowspace3/secrets.env; set +a; cargo test -p fs3-providers --test openrouter_contract -- --ignored --nocapture
```

The live chat model is `z-ai/glm-5.3-flash`. The embedding receipt uses `openai/text-embedding-3-small` at 1024 configured dimensions; chat success is never presented as embedding proof.

Observed 2026-08-28: both tests passed in one run. `z-ai/glm-5.3-flash`
reported `usage.total_tokens = 282`; `openai/text-embedding-3-small` returned
one vector at exactly the configured 1024 dimensions.
