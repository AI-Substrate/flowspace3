# GitHub Copilot provider

**Code**: `crates/providers/src/github_copilot.rs` · `crates/cli/src/github_copilot.rs` · `crates/daemon/src/wiring.rs`

GitHub Copilot is a first-class `github_copilot` provider for all three existing
model surfaces: agent chat (`ask`), summarization, and embeddings. It is never a
default. Each surface or repo selects a named instance explicitly.

## Wire contract of record

Measured 2026-08-29 against the machine's OMP login, without printing bearer
bytes:

- `GET https://api.github.com/copilot_internal/user` with GitHub OAuth auth
  returned the account-specific API endpoint.
- `GET /copilot_internal/v2/token` returned 403. Token exchange is not the live
  protocol and is deliberately absent.
- The GitHub OAuth token works directly as `Authorization: Bearer …` at the
  discovered endpoint.
- `/models` returned 44 model ids.
- `/embeddings` with `text-embedding-3-small` returned a 1536-wide vector.
- Bearer plus `Accept` was sufficient in the read-only header matrix. The
  adapter also sends OMP's stable integration metadata:
  `User-Agent: opencode/1.3.15` and
  `X-GitHub-Api-Version: 2026-06-01`. No
  `Copilot-Integration-Id` was required. Chat adds
  `Openai-Intent: conversation-edits` and `X-Initiator: user`.

The Copilot API is not a public arbitrary-client contract. These measured
headers and endpoint discovery are therefore versioned behavior, not a claim of
GitHub support for third-party clients.

## Authentication

```bash
flowspace3 login github-copilot
```

Credential precedence:

1. `COPILOT_GITHUB_TOKEN`, including a value loaded from flowspace3's
   mode-0600 `~/.config/flowspace3/secrets.env`.
2. `~/.config/github-copilot/hosts.json` or `apps.json`.
3. OMP's `github-copilot` OAuth row in `~/.omp/agent/agent.db`.

The OMP store is opened with SQLite `mode=ro&immutable=1`; flowspace3 never
writes, migrates, or locks it. When no reusable credential exists, login runs
GitHub's device-code flow and writes only flowspace3's own `secrets.env`, mode
0600. Tokens are redacted from `Debug`, errors, logs, config output, doctor, and
command envelopes.

`flowspace3 doctor` reports `logged in`, `token expired`, or `not logged in` for
an active Copilot instance. An unusable login steers directly to the login
command.

## Configuration

```toml
[providers.copilot-chat]
kind = "github_copilot"
model = "gpt-5.4"
max_tokens = 4000

[providers.copilot-embed]
kind = "github_copilot"
model = "text-embedding-3-small"
dimensions = 1536

[agent]
active = "copilot-chat"
[summarizer]
active = "copilot-chat"
# Explicit capability selection; never automatic.
[embedder]
active = "copilot-embed"
```

List account-visible model ids:

```bash
flowspace3 models copilot-chat
```

Embedding identity includes the configured width (`model@dimensions`). A corpus
embedded under one provider or width is not mixed silently with another even
when model names match. Changing `[embedder]` is an operator decision and writes
new enrichment under a new key; old rows remain for rollback.

## Verification

```bash
cargo test -p fs3-providers --test github_copilot_stub
cargo test -p fs3-core config::tests::github_copilot --lib
cargo test -p fs3-daemon --test config_wiring github_copilot_wires_only_when_explicitly_selected
cargo test -p fs3-cli copilot --lib
cargo test -p fs3-providers --test github_copilot_contract -- --ignored --nocapture
```

The ignored contract run accepts `COPILOT_GITHUB_TOKEN`, GitHub Copilot files,
or an existing OMP login. It runs the shared embedder and summarizer contracts
against the live service.
