# 019 agent hooks: what each harness can do on prompt submit

Researched 2026-10-09 against the installed versions on Jordan's machine. Sources
were the installed bundles and binaries, git-ai's installers (`~/github/git-ai/src/mdm/agents/*`),
and vendor docs. Nothing was installed or edited under `~`.

| | Claude Code | Codex CLI 0.159.1 | Copilot CLI 1.0.95 | pi 0.83.0 | omp 18.3.2 |
|---|---|---|---|---|---|
| Runs on prompt submit | `UserPromptSubmit` command hook | `UserPromptSubmit` command hook (hooks are stable and on by default) | `userPromptSubmitted` / `UserPromptSubmit` command hook | TS extension: `before_agent_start` (or `input`) | TS extension: `before_agent_start` (or `input`) |
| Can add context | yes: `additionalContext` (about a 10k-char cap) | yes: `additionalContext` or plain stdout (about a 2.5k-token cap; tunable per handler with `additionalContextLimit`) | **command hooks: no**, their output is dropped (per the docs). **Extensions: yes**, `onUserPromptSubmitted` returning `additionalContext` | yes: return `{message:{customType, content, display}}`. It is stored in the session and sent to the LLM | yes: same as pi. It is stored as `role:"custom"` |
| Can rewrite the prompt | no | no (can only block) | an extension can (`modifiedPrompt`) | yes: `input` → `{action:"transform", text}` | yes: `input` → `{text}` |
| Input | stdin JSON with `prompt`, `session_id`, `cwd`, `transcript_path` | stdin JSON with `prompt`, `session_id`, `cwd`, `transcript_path`, `turn_id`, `model` | stdin JSON with `prompt`, `sessionId` (= `~/.copilot/session-state/<id>`), `cwd`, `timestamp`. The extension gets the same fields as an object | `event.prompt`, `ctx.sessionManager.getSessionId()`, `ctx.cwd` | same as pi |
| Output shape | `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":…}}` | same JSON as Claude (schema `additionalProperties:false`) | the extension returns `{additionalContext}` | the extension returns `{message}` | the extension returns `{message}` |
| Timeout | per hook `timeout` (we use 40s) | per hook `timeout`, default 600s | default `timeoutSec` 30 | **none by default**: a slow handler holds the turn, so we pass `pi.exec({timeout})` | **30s per handler**: past that the result is silently dropped |
| Config, user scope | `~/.claude/settings.json` `hooks` | `~/.codex/hooks.json` or `[hooks]` in `~/.codex/config.toml` | `~/.copilot/extensions/<name>/extension.mjs` | `~/.pi/agent/extensions/<name>.ts` | `~/.omp/agent/extensions/<name>.ts` |
| Config, project scope | `.claude/settings{,.local}.json` | `<repo>/.codex/hooks.json` (needs a trusted project) | `.github/extensions/<name>/` | `.pi/extensions/` (needs a trusted project) | `<cwd>/.omp/extensions/` |
| Trust gate | none | **hash-pinned approval**: a new hook is skipped until it is approved in `/hooks`. git-ai pre-approves by writing `[hooks.state."<path>:<event>:<i>:<j>"] trusted_hash = "sha256:…"` | none found | project scope only | none found |
| `-this` / THIS CHAT tagging works | yes (`conversation verify --harness claude`) | **no**: fs3 does not ingest Codex chats | **no**: fs3 does not ingest Copilot chats | **no**: fs3 does not ingest pi chats | yes (`--harness omp`) |

## Design that follows

- **One implementation:** `flowspace3 hook prompt --harness <h>`. It reads a payload, runs the ported Python logic, and prints the shape that harness needs. Claude and Codex read stdin directly. The Copilot/pi/omp extensions are a few lines of JS/TS. Each checks the `fs3` prefix itself (no process spawned for other prompts), then pipes `{prompt, session_id, cwd}` to the same subcommand and returns its stdout as context. `pi.exec` ignores stdin, so the subcommand also accepts `--prompt/--session/--cwd`.
- **Gaps are named, not hidden:** on Codex, Copilot and pi, `-this` answers "this harness's chats are not indexed by flowspace3", and THIS CHAT tagging is skipped with the same note.
- **Time budget:** omp drops a handler's result at 30s, and the Python allows up to 25s per search call. For parity we keep the 25s per-call timeout, but the subcommand takes an overall deadline (default 25s), so a `-this` multi-page search can't outlive omp's limit.

## Product calls for Jordan

1. **Codex trust:** should we pre-approve our hook by writing `trusted_hash` (what git-ai does; works first time), or install it unapproved and print "approve it in `/hooks`" (the user sees Codex's own review prompt)? *Recommend pre-approve.* The user ran the install command, which is the consent.
2. **Copilot needs an extension, not a hook:** command hooks can't add context. *Recommend* a bundled `~/.copilot/extensions/fs3/extension.mjs` (Copilot SDK, the same mechanism as the pij extension already there). The fallback, a `userPromptTransformed` command hook returning a modified prompt, is unconfirmed, and I'd only use it if a live test shows extensions failing.
3. **pi/omp visibility:** the injected hits become a session message. Show them in the TUI (`display:true`) or hide them (`display:false`, which matches Claude, where the user never sees the raw hits)? *Recommend hidden*, for parity.
4. **pi/omp could strip the `fs3 "…"` line** via `input` transform, so the agent sees only the message. That would be better than Claude, which can't. *Recommend parity (no rewrite)* for v1: the same grammar and the same agent view everywhere.
5. **Python hook:** *recommend* turning it into a 3-line shim (`exec flowspace3 hook prompt --harness claude`) for one release, so existing curl installs keep working; `hooks install` then rewrites the registration to the binary and reports the Python one as stale. Delete the shim in the release after.
6. **Copilot also reads a repo's `.claude/settings.json` hooks.** A project-scope Claude install therefore also fires inside Copilot: a wasted search whose output is dropped, which can add up to 25s of latency. *Recommend* that `hooks install` defaults to **user scope**, and that the subcommand exits immediately when it detects it is running under Copilot (an environment probe, verified during the build).
