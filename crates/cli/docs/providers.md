# Providers — setting one up from scratch

fs3 has exactly two ports: an **embedder** (text to vectors) and a
**summarizer** (element to summary plus 1–5 concept tags). Configuration
declares a REGISTRY of named instances; the ports name one of them.

**On a fresh install you have no real provider.** The defaults ship a single
`fake` instance and both ports name it — that is what makes the whole stack,
search included, work offline before you have any credentials. `flowspace3
doctor` reports this as a `providers` warning. If offline is what you want,
nothing needs doing. Otherwise this page is the whole setup.

Declaring an instance costs nothing: only the ones a port or a repo actually
names are ever constructed, so leaving a provider you have no key for in the
file is free.

Config lives in `~/.config/flowspace3/config.toml`
(`flowspace3 docs get config`).

## The shape

```toml
[providers.<name>]          # any name you like — this is the registry key
kind = "fake" | "openai" | "azure_openai"

[embedder]
active = "<name>"           # which instance embeds

[summarizer]
active = "<name>"           # which instance summarises
```

The two ports are chosen separately and usually differ: embedding is cheap and
high-volume, summarising is expensive and low-volume.

## kind = "fake"

```toml
[providers.offline]
kind = "fake"
```

Deterministic, offline, keyless — and a legal PRODUCTION value, not a test
hook. It emits real vectors at the store's width, so search genuinely works.
Same text always yields the same vector, and related text ranks above unrelated
text. Useful for trying fs3, for CI, and for repositories you do not want sent
anywhere.

## kind = "azure_openai"

One instance per DEPLOYMENT. The two ports normally want two instances, because
the chat and embedding deployments are different names and usually different
`api-version`s.

```toml
[providers.azure-embed]
kind = "azure_openai"
endpoint = "https://YOUR-RESOURCE.openai.azure.com"
deployment = "text-embedding-3-small"   # the DEPLOYMENT name, not the model
api_version = "2024-02-01"
dimensions = 1024                       # embeddings only; must match the store

[providers.azure-chat]
kind = "azure_openai"
endpoint = "https://YOUR-RESOURCE.openai.azure.com"
deployment = "gpt-4o"
api_version = "2024-12-01-preview"

[embedder]
active = "azure-embed"

[summarizer]
active = "azure-chat"
```

### Auth mode 1 — Entra (no key in config, recommended)

Omit `api_key_env` entirely. The adapter uses `azure_identity`'s developer
credential chain: managed identity where present, otherwise your `az login`.

```bash
az login
# the signed-in identity needs the "Cognitive Services OpenAI User" role
# on the resource
```

This is the **only** way into a resource that has key auth disabled — such a
resource answers 403 `AuthenticationTypeDisabled` to any key.

### Auth mode 2 — api-key

```toml
[providers.azure-embed]
kind = "azure_openai"
endpoint = "https://YOUR-RESOURCE.openai.azure.com"
deployment = "text-embedding-3-small"
api_version = "2024-02-01"
dimensions = 1024
api_key_env = "AZURE_OPENAI_API_KEY"    # the NAME of a variable, never the key
```

Then supply the value out of band — the process environment, or
`~/.config/flowspace3/secrets.env`:

```
AZURE_OPENAI_API_KEY=…
```

Exactly one mode is used per instance. Naming `api_key_env` selects the key;
omitting it selects Entra.

### Azure failures, and which is which

They are mutually confusable and individually fixable:

- **401** — the credential was rejected.
- **403 with `AuthenticationTypeDisabled`** — key auth is off; use Entra.
- **403 alone** — the identity lacks *Cognitive Services OpenAI User*.
- **404** — wrong DEPLOYMENT name. Reads like a wrong endpoint; it is not.
- **400 mentioning `api-version`** — that version is unsupported for that route.

**The precedence trap:** an exported stale `AZURE_OPENAI_API_KEY` silently
beats `az login`, and the resulting 401 reads like a broken Entra setup. If
Entra auth fails unexpectedly, unset the key variables first.

## kind = "openai"

```toml
[providers.small]
kind = "openai"
model = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"          # the default if omitted
# api_base = "https://api.openai.com/v1"
```

## Choosing an embedder

| option | where it runs | keys | notes |
|---|---|---|---|
| `fake` | in process | none | offline, deterministic, real vectors |
| `azure_openai` | Azure | Entra or key | `dimensions` must match the store's table |
| `openai` | OpenAI | key | |

Two more adapters exist in the codebase and are **not yet selectable from
config** — the config enum has no variant for them, so writing one is a startup
error rather than a working setup:

- **Local ONNX embeddings** (in-process, fastembed/BGE-small, 384 dimensions,
  no API and no key). Note 384 ≠ the 1024-wide store table, so it needs its own
  `embeddings_384` migration before it can be used. See the repository's
  `docs/services/local-embeddings.md`.
- **OpenAI-compatible summarizer** for a LAN server (llama.cpp, Ollama, vLLM,
  LM Studio). Summarizer only — those servers answer `/v1/embeddings` with 501.
  It discovers the served model rather than trusting a configured name. See
  `docs/services/openai-compat.md`.

Ask for them if you need them; do not write config keys for them yet.

## Per-repo overrides

```toml
[repos."git:github.com/AI-Substrate/flowspace3"]
embedder = "azure-embed"
summarizer = "offline"           # a private repo whose code stays local
```

Keyed by repo identity, exactly as `flowspace3 status` reports it. A repository
that names nothing uses the global actives. This is how one machine indexes
public code with a cloud model and private code with the offline fake.

## After changing providers

Enrichment rows are keyed by a `model_key` the PROVIDER reports —
`model@dimensions` for an embedder, `model@prompt_version` for a summarizer,
using the DEPLOYMENT name on Azure. Two consequences:

- **A model change is not a migration.** New key, new rows; old rows survive
  untouched, so rolling back is instant.
- **Search only reads vectors written under the key it is searching with.**
  Switching the embedder means existing vectors are no longer searched, and a
  search will return nothing while the index looks full. Re-run
  `flowspace3 add <path>` to re-index under the new model, and
  `flowspace3 doctor` to confirm which provider is active.

Verify a change with:

```bash
flowspace3 doctor          # the providers row names the active instances
flowspace3 ping            # the daemon reports the arms it wired
```
