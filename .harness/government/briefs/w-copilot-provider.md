# w-copilot-provider — GitHub Copilot models as a first-class provider

**From**: pij-instant-lynx (o-prime) · 2026-08-29 · Jordan's verbatim intent:
"we want to support GitHub co-pilot models in FlowSpace 3. So third-party
clients, like for example OMP, can log into GitHub Copilot and then use those
LLMs for their work. I would like a similar thing so then we can use GitHub
Copilot models for things like Ask."

## The job

A new provider kind `github_copilot` beside `openai_compat`
(crates/*, provider config in ~/.config/flowspace3/config.toml), usable as
the `[agent]` (ask) and `[summarizer]` surface. Embedder support only if the
Copilot API exposes embeddings cleanly — verify, don't assume; chat is the
deliverable.

### Units

1. **Auth**: GitHub device-code OAuth flow (`flowspace3 login github-copilot`
   or similar verb): prints user code + URL, polls, stores the GitHub token
   in ~/.config/flowspace3/secrets.env-adjacent storage (0600, never
   printed). Then the Copilot token exchange: GitHub token →
   api.github.com/copilot_internal/v2/token → short-lived bearer, cached and
   auto-refreshed before expiry. ACCELERANT (do first if simpler): detect an
   existing logged-in Copilot credential on the machine
   (~/.config/github-copilot/hosts.json / apps.json — what gh/OMP/editors
   write) and reuse it, with the device flow as the path when nothing is
   found. Study how OMP does its Copilot login — it is the named prior art.
2. **Chat adapter**: Copilot's chat completions endpoint is openai-shaped
   (api.githubcopilot.com/chat/completions + required headers like
   Copilot-Integration-Id / editor-version — establish the exact required
   set empirically). Reuse/extend the openai_compat request path rather than
   forking it; the difference is auth acquisition + headers + base URL.
3. **Config**: `[providers.<name>] kind = "github_copilot", model = "<id>"`;
   per-surface `active =` and per-repo overrides work unchanged. `doctor`
   shows login state (logged in / token expired / not logged in) without
   leaking tokens; a helpful `next_action` names the login verb.
4. **Model listing**: `flowspace3 models <provider>` (or fold into doctor)
   hitting Copilot's /models so Jordan can see which ids are usable for ask.
5. **Proof**: live ask receipt on a Copilot model (Jordan is logged into
   Copilot on this machine) + fake-provider-refusal parity (ask's honesty
   rules apply unchanged) + tests that mock the token exchange.

## Cautions

- Tokens NEVER printed, never committed, never in envelopes.
- Respect the surface split: ask/summarizer only unless embeddings are
  verified to exist on the API; do not silently wire an embedder.
- The Copilot API is not officially public for arbitrary clients — match
  the headers/behaviour of a well-known integration (OMP's) rather than
  inventing; if the exchange refuses without an integration id, use the one
  OMP uses and record that as a named limitation.
- ALL testing on alternative ports + DB configs; never against prod :7373.

## Rules & fence

Worktree /Users/jordanknight/substrate/flowspace/fs3-copilot-provider,
branch w-copilot-provider; absolute paths; per-seat CARGO_TARGET_DIR + test
DB with teardown; read CLAUDE.md + TENETS.md; numbered plan-of-attack to
pij-instant-lynx before code; gate `harness checks`; `harness commit`; PR
into main. Prior art to read: crates' openai_compat adapter (#58) and its
config plumbing (config.rs providers section).
