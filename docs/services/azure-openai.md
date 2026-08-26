# Azure OpenAI provider
**Built**: 2026-08-26 (worker pij-sure-kazimir, w-azure) · **Code**: `crates/providers/src/azure_openai.rs` (+ `tests/azure_openai_{stub,contract}.rs`)

Both ports (`Embedder` + `Summarizer`) against Azure OpenAI deployments. Auth: api-key header OR Entra bearer (`azure_identity` `DeveloperToolsCredential` via `AzureCredential::from_environment()`; `AzureCredential::entra(...)` lets a daemon inject managed identity and a test inject a fake).

Azure serves OpenAI's *response* shapes behind a different address and a different door. Those two differences are the whole adapter; everything below them is the OpenAI wire format, which is why the summarizer reuses `OpenAiSummarizer::SYSTEM_PROMPT` and `user_prompt` rather than growing a second prompt that can drift from the first.

## Key decisions
- Deployment name lives in the URL path, NO `model` field in the body; `api-version` is a query param and differs per route (chat vs embeddings). Hence **one `AzureOpenAiConfig` per port**, not one per resource.
- Exactly one of api-key / bearer is ever sent — they are one enum (`AzureCredential`), not two optional fields that could both be set or both be empty.
- Optional `dimensions` is requested AND verified against the response — a deployment that silently ignores it would poison a store column sized from that number.
- Entra tokens cached with a 300s refresh skew (uncached CLI credential = one `az` process spawn per batch).
- The key is held in `azure_core`'s `Secret`, so redaction lives in the type: any struct holding one keeps `#[derive(Debug)]` and stays safe.
- Errors name **which** of Azure's rejections happened and what to do about it, because the four common ones look alike and have four different remedies (see below).
- `azure_identity` does the Entra acquisition. Hand-rolling the OAuth flow would have been the reinvention the arch allowlist exists to notice.

### Structured summaries (added 2026-08-26)
- Chat calls ask for `response_format: json_schema` with `strict: true`, so "the model replied with prose" stops being a failure class rather than being caught after the fact.
- **Graceful downgrade, not a hard requirement.** An endpoint that rejects the schema (an older `api-version`; any OpenAI-compatible server) gets one retry in `json_object`, and the downgrade is remembered for the life of the process — one wasted round trip per process, not per element. Both paths leave through the same `parse_summary`, which is what makes the fallback safe rather than merely convenient.
- The 1–5 tag band stays in the **prompt**, because OpenAI's `strict` subset has no `minItems`/`maxItems`. A test asserts the schema still lacks them, so the day that changes, the reminder exists.
- **The prompt is a versioned artifact.** `OpenAiSummarizer::PROMPT_VERSION` covers the prompt *and* the schema, and the Azure adapter shares the constant because it shares both. Bump it when either changes.
- **Both ports expose `key()`** — the enrichment row key. Summarizer: `deployment@prompt_version`. Embedder: `deployment@dimensions`, or the deployment alone at native width. The provider owns this because on Azure config cannot tell you what served the request; only the deployment name can. A prompt or model change is therefore never a migration: new key, the reconciler re-enriches, old rows survive for rollback.
- `Summary` carries `extras` (`serde(flatten)`), so a field a future prompt learns to extract lands there instead of being dropped, and gets promoted to a typed field only once it earns one.

## Gotchas learned
- The live resource has **key auth disabled** (403 `AuthenticationTypeDisabled`) — Entra is the only way in. Consequence for this repo: **the api-key leg is proved offline only**; it cannot be exercised against that resource at all.
- **Auth precedence trap**: an exported stale `AZURE_OPENAI_API_KEY` silently beats `az login`, and the resulting 401 reads like a broken Entra setup. Costly enough to be worth the `env -u` in the verify command below. (It cost one confused run here; the error naming *both* fixes is what made it a one-run diagnosis rather than a hunt.)
- Azure's rejections are mutually confusable and individually fixable: **401** = credential rejected; **403 + `AuthenticationTypeDisabled`** = key auth off, use Entra; **403** alone = identity lacks the *Cognitive Services OpenAI User* role; **404** = wrong DEPLOYMENT name (not the model name), which reads like a wrong URL; **400** mentioning `api-version` = unsupported version for that route. The adapter maps each to its own instruction.
- The embeddings response is a **mapping keyed by `index`**, not a list. A duplicated or out-of-range index must be rejected, never silently left as an empty vector in a slot.
- **`DefaultAzureCredential` does not exist in the Rust SDK.** It exists in the Python/.NET/Java/Go SDKs, so cross-language prior art (including fs2's) names something Rust has not got. `azure_identity` 1.0 offers `DeveloperToolsCredential` (`az login` + `azd`), `ManagedIdentityCredential`, `WorkloadIdentityCredential`, `ClientSecretCredential`.
- `azure_core` brings its own reqwest 0.13 beside the workspace's 0.12 — two HTTP stacks in the tree. Accepted: `default-features = false` would strip the HTTP client `ManagedIdentityCredential` needs.
- A trailing slash on the endpoint must not become `//` in the path — Azure answers that with a 404 that reads like a missing deployment, sending the reader after the wrong thing. Pinned by a test.

## Verify
Offline, keyless, no Azure account — this is what CI runs:
```bash
cargo test -p fs3-providers      # 25 unit + 13 stub-server tests; the 4 keyed tests stay ignored
harness checks                   # docs, fmt, clippy -D warnings, cargo test --all, arch drift
```

Keyed, against a real deployment. `env -u` is not decoration — see the precedence trap above:
```bash
env -u AZURE_OPENAI_API_KEY -u AZURE_EMBEDDING_API_KEY \
  AZURE_OPENAI_ENDPOINT=https://<resource>.openai.azure.com \
  AZURE_OPENAI_CHAT_DEPLOYMENT=<chat-deployment> \
  AZURE_OPENAI_EMBEDDING_DEPLOYMENT=<embed-deployment> \
  AZURE_OPENAI_EMBEDDING_DIMENSIONS=1024 \
  cargo test -p fs3-providers --test azure_openai_contract -- --ignored
```
Prerequisite for that form: a current `az login` whose identity holds the *Cognitive Services OpenAI User* role on the resource. Export `AZURE_OPENAI_API_KEY` instead to take the api-key path. `AZURE_OPENAI_CHAT_API_VERSION` and `AZURE_OPENAI_EMBEDDING_API_VERSION` are optional and default to the versions pinned in the module. A missing required variable fails by name rather than skipping — a keyed run that passes because it did nothing is worse than a red one.

This machine's working values (endpoint + both deployment names + dimensions) are the ones already in `~/.config/fs2/config.yaml`; fs2 is where they came from.

Keyed run green 3× on 2026-08-26 against the live resource (o-prime verified independently), and re-run green from a clean checkout of `2b11b07` while writing this page.

## Code pointers
- `crates/providers/src/azure_openai.rs` — `AzureCredential` (`api_key`, `api_key_from_env`, `from_environment`, `entra`), `AzureOpenAiConfig`, `AzureOpenAiEmbedder`, `AzureOpenAiSummarizer`. `AzureOpenAiClient::url` is the addressing scheme; `AzureOpenAiClient::failure` is the error mapping; `AzureOpenAiClient::bearer` is the token cache.
- `crates/providers/tests/azure_openai_stub.rs` — the offline proof. `StubServer` is an axum server on `127.0.0.1:0` that records what it was asked (a **fake**, not a mock — workshop 001 rule 5); `ScriptedCredential` is a `TokenCredential` that dictates tokens and counts acquisitions.
- `crates/providers/tests/azure_openai_contract.rs` — the keyed leg; its header is the canonical env-var list.
- `crates/testkit/arch-allowlist.toml` — the `fs3-providers` row records `azure_core`, `azure_identity`, `axum@dev` and why each is a real need.
- `docs/plans/prd/providers-roster.md` — where this sits among the other adapters.

## Snap-in
Not yet wired into the composition root — the `ProviderConfig` variant + match arm recipe is a doc comment on the module; the integrating stream adds it at adoption. Note for whoever does it: the two ports usually want *different* deployments and api-versions, so config should carry one `AzureOpenAiConfig` per port.
