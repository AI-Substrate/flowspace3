# Worker brief — structured summaries · pij-sure-kazimir (RE-OPENED)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · one bounded feature on your own prior work

## The job
Jordan: summaries need a real prompt + a constrained return format ("they're going to have to extract tags and also summary and maybe even other fields in the future"). You built the adapters — add the feature:

1. **Structured outputs**: OpenAI + Azure chat calls use `response_format` with a JSON schema constraining `{summary, tags}` — the parse-failure class disappears where the API supports it; graceful FALLBACK to the current prompt+parse (validated) where it doesn't (older api-versions, local/compat providers). Stub tests pin that the request carries response_format and that the fallback triggers on a rejecting endpoint.
2. **Prompt as versioned artifact**: the prompt + schema get an explicit `PROMPT_VERSION` per adapter family. Rationale (already schema-anticipated): enrichment rows are keyed `model_key = model@prompt_version`, so a prompt/schema change is NEVER a migration — new key, reconciler re-enriches, old rows survive for rollback.
3. **Port evolution (coordinated, minimal)**: both port traits gain `fn key(&self) -> String` returning `model@prompt_version` (embedder: model@dims or model alone — your call, justify) so enrichment consumers get the row key from the provider instead of re-deriving it from config. Sawfish (first-light, wiring enrichment NOW) is told to consume it — additive, does not change its contract otherwise.
4. **Future fields**: `fs3_core::Summary` gains `extras: BTreeMap<String, serde_json::Value>` (default empty; `text`+`tags` stay the typed contract; `has_valid_tags` untouched). New fields land in extras first, promoted to typed later. Fakes: `FakeSummarizer` fills a deterministic extras entry so the shape is exercised in CI.
5. Keyed contract re-run (Azure, ambient Entra creds — your own doc has the command) proving structured output live; update `docs/services/azure-openai.md` + the module docs; roster row note.

## Coordination (three writers near one crate — discipline)
- hummingbird is ACTIVE in crates/providers (new local.rs module) — you touch openai.rs/azure_openai.rs/core only; lib.rs/manifest edits file-scoped + committed promptly; hunk-audit shared files (ruling 2026-08-26-commit-push-as-you-go.md).
- sawfish is ACTIVE in daemon/cli/store — your core touch is ports.rs/element-adjacent Summary only; coordinate through me if anything else beckons.

## Rules
Architecture binds (no new ports — `key()` is evolution of the existing two, sanctioned here); no mocks; gates `harness checks` + `cargo test -p fs3-providers -p fs3-core` green; ddocs untouched (no plan rows — this is a roster-level feature). Report: claim · commits · keyed-run output · observations. Deviations = stop-and-ask.

---

## UNIT 2 (added 2026-08-26, Jordan) — OpenAI-compatible generic adapter (the roster row), Summarizer-first

A live LAN endpoint exists NOW for real validation. Jordan's connection block, verbatim:

- Base URL `http://192.168.1.134:8080/v1` · API key not required (send any placeholder) · model id ignored ("local") · context 131,072 · currently serving a Q5_K_M quant.
- **Reasoning model**: `max_tokens` 2000+ mandatory — thinking goes to `reasoning_content`, the answer to `content`, both share the budget; too low ⇒ EMPTY content with NO error. The adapter must treat empty-content-without-error as a NAMED failure (fix: raise max_tokens), never as an empty summary.
- Only `POST /v1/chat/completions` + `GET /v1/models` exist — `/v1/embeddings` 404s ⇒ this adapter is **Summarizer only**; an embedder config pointing at it gets an actionable refusal. `GET /health` 200s only once the model is loaded — poll, never sleep.
- Tool calling exists but is irrelevant to summarize. Streaming irrelevant (we don't stream).

Shape: `OpenAiCompatSummarizer` (new module) — `base_url` + optional `api_key_env` (placeholder default) + `max_tokens` (default 4000) + your PROMPT_VERSION machinery; structured outputs attempted, your unit-1 fallback path when the server rejects `response_format`; `key()` = served model id from `/v1/models` if stable, else configured instance name @ prompt_version — your call, justify. Config kind: `openai-compat`.

Caveats to carry in docs/services (not code): LAN-only/no-auth — usable only on-network; the box serves ONE model at a time, so a mid-run mode switch silently changes quants (note under gotchas; `key()` can't fully defend this — say so honestly).

Validation: stub tests as usual PLUS a live `#[ignore]`d leg against the LAN endpoint — run it while on-network and report actual output (a real summary of a real element, with the reasoning-budget behaviour observed). Roster row flips on landing.
