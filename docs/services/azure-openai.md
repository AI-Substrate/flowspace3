# Azure OpenAI provider
**Built**: 2026-08-26 (worker pij-sure-kazimir, w-azure) · **Code**: `crates/providers/src/azure_openai.rs` (+ `tests/azure_openai_{stub,contract}.rs`)

Both ports (`Embedder` + `Summarizer`) against Azure OpenAI deployments. Auth: api-key header OR Entra bearer (`azure_identity` `DeveloperToolsCredential` via `AzureCredential::from_environment()`; `AzureCredential::entra(...)` lets a daemon inject managed identity and a test inject a fake).

## Key decisions
- Deployment name lives in the URL path, NO `model` field in the body; `api-version` is a query param and differs per route (chat vs embeddings).
- Exactly one of api-key / bearer is ever sent.
- Optional `dimensions` is requested AND verified against the response — a deployment that silently ignores it would poison a store column.
- Entra tokens cached with a 300s refresh skew (uncached CLI credential = one `az` process spawn per batch).

## Gotchas learned
- The live resource has **key auth disabled** (403 `AuthenticationTypeDisabled`) — Entra is the only way in; the adapter's error names both fixes.
- Auth precedence trap: an exported stale `AZURE_OPENAI_API_KEY` silently beats `az login` — precedence is documented in the contract test header.
- `DefaultAzureCredential` does not exist in the Rust SDK (Python-ism); `azure_core` brings its own reqwest 0.13 beside the workspace's 0.12 (accepted — trimming breaks ManagedIdentityCredential).

## Verify
```bash
AZURE_OPENAI_ENDPOINT=https://<resource>.openai.azure.com \
AZURE_OPENAI_CHAT_DEPLOYMENT=<chat-deployment> \
AZURE_OPENAI_EMBEDDING_DEPLOYMENT=<embed-deployment> \
AZURE_OPENAI_EMBEDDING_DIMENSIONS=1024 \
cargo test -p fs3-providers --test azure_openai_contract -- --ignored
```
Keyed run green 3× on 2026-08-26 against the live resource (o-prime verified independently). Offline: 13 stub-server tests run keyless in `cargo test -p fs3-providers`.

## Snap-in
Not yet wired into the composition root — the `ProviderConfig` variant + match arm recipe is a doc comment on the module; the integrating stream adds it at adoption.
