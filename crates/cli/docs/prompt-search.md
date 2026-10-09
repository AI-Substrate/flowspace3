# fs3 prompts: search before the agent's turn

Type a prompt that starts with `fs3` and a hook runs a flowspace3 search first,
then hands the hits to the agent together with your message. The agent never
has to decide to search. It is search only, never `ask`. It works the same in
Claude Code, Codex, GitHub Copilot CLI, pi and omp; install it with
`flowspace3 hooks install` (see [Install](#install)).

```text
fs3 "<switches> <search text>" <your message to the agent>
```

The quotes hold the search. Everything after the closing quote is your message,
and the agent reads it with the hits in front of it.

The search can also sit anywhere in the prompt as one quoted piece that starts
with `fs3`; the rest of the prompt, before and after it, is your message:

```text
okay lets talk about the pij --fyi feature "fs3 -a pij fyi"
```

The quoted piece needs real search words after `fs3`. Messages that other agents
and the harness inject (cross-session messages, task notifications, system
reminders) never trigger it, even when they quote an fs3 example.

## Switches

Switches go anywhere inside the quotes; a word starting with `-` that is not a
switch stays part of the search when it follows the search text.

| switch | short | effect |
|---|---|---|
| (none) | | this repo: code, docs and conversations |
| `-all` | `-a` | every indexed repo |
| `-repo pij` | `-r pij` | one repo, by short name (`pij`, `eldenring`) or full identity |
| `-code` | `-f` | code and docs only |
| `-convo` | `-c` | conversations only |
| `-this` | `-s` | only this chat's own conversation, in any repo |
| `-n 15` | | how many hits (default 8, max 25) |
| `-quick` | `-q` | the agent answers from the hits alone, without reading further |
| `-raw` | | keep tool calls, tool output and short fragments, which are left out by default |
| `-help` | `-h` | no search: the agent explains this page |

Where (`-all`, `-repo`) and what (`-code`, `-convo`) combine; short switches can
be joined, so `-ac` means `-all -convo`. `--all` style also works.

It is forgiving. Case does not matter, single or curly quotes work, and an
unquoted `fs3 -this nas backup` works too. A near-miss switch (`-thsi`, `-con`,
`-repos`) is read as the closest real one, and the agent tells you how it read
it. A switch it cannot place (`-co` could be `-code` or `-convo`; `--sdfjkls`
is nothing) is skipped: the search still runs on the rest, and the agent starts
its answer by naming the switch and showing you the real usage. A switch with
no search text runs no search; the agent just shows the usage.

```text
fs3 "how does ingest handle NUL"
fs3 "-all -convo copilot ask provider" what did we decide, and is it still true?
fs3 "-ac copilot ask provider" same thing, short form
fs3 "-r eldenring -c boss strategy" what worked?
fs3 "-code merge_choices" walk me through this
fs3 "-this weekly NAS backup" where do the dumps go again?
fs3 "-q -all openrouter throughput"
fs3 "-help"
```

## What the agent receives

The hook adds the hits as context for that one turn, under a budget of about
9,000 characters. Hits from one conversation are grouped under it, in rank
order; each carries its address, repo, path, kind, score and its summary or
snippet. Alongside the hits the agent is told that you typed `fs3` because you
want the answer from flowspace3, so it should use it before the web,
memory or other tools, and to:

- judge relevance first and say which hits it ignored; if none bear on the
  question, the search missed, so run another `flowspace3 search` before anything else;
- read around the strongest hits with `flowspace3 get <address>` before
  answering (`-quick` turns this off), and treat conversations as history to
  check against current code;
- dig further itself if it needs to: `flowspace3 search` with `--repo`,
  `--source`, `--path` and `--offset`, `flowspace3 tree conv:<guid>`, and
  `flowspace3 docs list`.

When the results look weak (no hit contains the search words, outside this
chat's own turns, and the best meaning match scores under 0.55), the agent is
told so plainly. It must then run at least two or three more searches with flowspace3
before saying there is nothing, and it gets ready-to-run commands: your message
as a full question, each key word alone, and the same search across every repo
and source.

With no message after the quotes, the agent summarises what the hits say. You
do not see the raw hits yourself; the agent does. A hook cannot rewrite your
prompt, so the agent also sees the `fs3 "…"` line as you typed it.

One thing is left out and one is marked, each with a note to the agent saying
how many:

- **Tool calls, tool output and fragments are left out.** Conversation turns
  with no summary that are a tool call, tool output, or under 160 characters
  (process exit lines, pane names). Commands rarely answer a question; the prose
  around them does. The note counts each kind. `-raw` keeps them, and plain
  `flowspace3 search` never drops them.
- **This chat's last 100 turns are marked, never hidden.** They are tagged
  `THIS CHAT` so the agent knows it may already have them; after a compaction,
  search may be its only copy of them.

## Limits

- **This chat lags a little.** `-this` finds turns the daemon has already
  ingested, usually a couple of minutes behind the live chat.
- **`-this` over-fetches.** `search` has no per-conversation filter yet, so the
  hook reads up to 300 conversation hits across all repos and keeps this chat's.
  A rare query whose hits all come from other chats can come back empty.
- **The hook never blocks a prompt.** A failed search, an unknown switch, or a
  stopped daemon is reported to the agent with the hits it did get.
- **`-this` needs a harness whose chats are indexed.** That is Claude
  Code and omp today. In Codex, Copilot CLI and pi, `-this` says so instead of
  searching, and no hit is tagged `THIS CHAT`.

## Install

```bash
flowspace3 hooks install            # every harness you have, for every repo
flowspace3 hooks status             # installed, missing or stale, per harness
flowspace3 hooks uninstall          # remove it again
```

`--harness claude|codex|copilot|pi|omp` picks one harness (`all` is the
default, and skips harnesses you do not have). `--scope project` writes into
this repo instead of your user config. Every write merges into what is
already there: other tools' hooks (git-ai's included) are left alone, a
changed config file is first copied to `<file>.fs3-backup-<time>` (the extension
files are wholly flowspace3's and are regenerated instead), and a second run
changes nothing. The envelope lists every file it touched. `flowspace3 doctor`
shows one `hooks:<harness>` row per harness you have, and its `next_action`
is the exact install command when one is missing or stale.

The hook is this binary: each harness runs `flowspace3 hooks prompt --harness
<h>`, which reads the prompt, searches, and prints the shape that harness
reads. It never blocks a prompt and prints nothing for prompts that are not
fs3 prompts. If you move or reinstall the binary, `hooks status` reports the
hook stale and `hooks install` repoints it.

| harness | where it goes (user scope / `--scope project`) | how the hits arrive |
|---|---|---|
| Claude Code | `~/.claude/settings.json` / `.claude/settings.local.json`, a `UserPromptSubmit` command hook | `additionalContext` |
| Codex | `~/.codex/config.toml` / `.codex/config.toml`, an inline `UserPromptSubmit` hook, pre-approved with its `trusted_hash` | `additionalContext` |
| Copilot CLI | `~/.copilot/extensions/flowspace3/extension.mjs` / `.github/extensions/flowspace3/` | the extension's `additionalContext` (Copilot drops command-hook output, so it is an extension) |
| pi | `~/.pi/agent/extensions/flowspace3.ts` / `.pi/extensions/` | a hidden session message from `before_agent_start` |
| omp | `~/.omp/agent/extensions/flowspace3.ts` / `.omp/extensions/` | a hidden session message from `before_agent_start` |

When to expect it: Claude Code picks up settings changes in a running session
(open `/hooks` once if it does not). Restart Codex. Copilot, pi and omp load
extensions when a session starts. Codex and pi load project-scope hooks only
in a project you have trusted.

Copilot CLI also runs a repo's `.claude/settings*.json` hooks. The Claude hook
recognises Copilot's input and steps aside at once, so a project-scope Claude
install costs Copilot nothing.

The old Python script (`integrations/claude-code/fs3-prompt-search.py`) is now
a shim that runs `flowspace3 hooks prompt --harness claude`; it will be
removed in the next release. `hooks status` reports a registration of it as
stale, and `hooks install` replaces it in place.
