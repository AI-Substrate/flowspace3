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
