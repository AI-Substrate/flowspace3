# w-openrouter — OpenRouter provider support (OpenAI-shaped endpoints only)

**Ruled in by Jordan 2026-08-28.** flowspace3 gains an OpenRouter provider.
Scope guard: OpenAI-shaped endpoints ONLY for now — chat completions and
embeddings in the OpenAI wire shape against https://openrouter.ai/api/v1.
No provider-routing extras, no model-list discovery, no streaming work beyond
what existing surfaces already do.

## Where it plugs in (facts, verified against main c8f5006+)

- Providers are declared in `~/.config/flowspace3/config.toml` under
  `[providers.<name>]` with a `kind` (today: `azure_openai`, plus fakes), and
  selected per surface via `[embedder]/[summarizer]/[agent] active = "<name>"`.
- Config layers: defaults < config.toml < `FS3_*` env. `flowspace3 config
  show` prints the effective config and the layer each section came from —
  the new kind must render there like the others.
- **THE KEY LIVES IN `~/.config/flowspace3/secrets.env`** — this file is
  already designed into resolution (config show reports it present/absent;
  "secrets are never printed") but has been empty-by-design because Azure
  auth is Entra. OpenRouter is its first real tenant. Jordan supplies the
  key. Secrets NEVER go in config.toml, never in the envelope, never in
  logs, never in `config show` output.
- Four ports exist (ChatProvider, ConversationSource, embedder, summarizer —
  a FIFTH is stop-and-ask, see ports.rs guard). OpenRouter is NEW
  IMPLEMENTATIONS of existing ports, not a new port: an OpenAI-shaped
  embedder + an OpenAI-shaped chat/summarize/agent provider.

## Shape guidance (coder proposes, prime rules on deviation)

- Prefer ONE `kind = "openai_compat"` (base_url + api_key_env + model) over a
  hardcoded `openrouter` kind, so any OpenAI-shaped endpoint works and
  OpenRouter is just config: base_url https://openrouter.ai/api/v1. If the
  implementation genuinely wants OpenRouter-specific headers (HTTP-Referer /
  X-Title attribution headers are recommended by OpenRouter), a thin
  openrouter kind wrapping openai_compat is acceptable — argue it in the ack.
- Key resolution: `api_key_env = "OPENROUTER_API_KEY"` style — the provider
  names the env var, secrets.env supplies it at daemon boot. Missing key at
  wiring time = honest config error naming the var and the file
  (FS3-E-PROVIDER-CANNOT-ANSWER family precedent, #55).
- Respect DL-004/tenet 14 lineage: fake providers stay sandbox-only; a
  misconfigured OpenRouter provider refuses loudly, never silently degrades.
- Embedding dimensions are config (`dimensions = N` like azure-embed) —
  remember the index's most dangerous failure is searching with a different
  embedder than the one that wrote (CONF-004): the provider identity rides
  the model_key like existing providers.

## Done bar

- `config show` renders the new provider kind with secrets absent.
- Boot line names it like the others (posture printed AFTER wiring proven —
  row 50's lesson; do not print a ready line you have not validated).
- Live proof against real OpenRouter with Jordan's key: one embed round-trip
  + one summarize/chat round-trip, receipts in the PR body (token counts
  from the response — and note item: OpenRouter returns usage in the OpenAI
  shape, so tokens_used should be REAL here, unlike the current Azure
  adapter gap, backlog row 48c).
- Offline tests with fakes; no test needs the real key; `harness checks`
  green; PR into main.
