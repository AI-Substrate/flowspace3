# fs3 prompts: search before the agent's turn (Claude Code)

Type a prompt that starts with `fs3` and a Claude Code hook runs a flowspace3
search first, then hands the hits to the agent together with your message. The
agent never has to decide to search. It is search only, never `ask`.

```text
fs3 "<switches> <search text>" <your message to the agent>
```

The quotes hold the search. Everything after the closing quote is your message,
and the agent reads it with the hits in front of it.

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
| `-raw` | | keep short tool-output turns, which are dropped as noise by default |
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
snippet. Alongside the hits the agent is told to:

- judge relevance first and say which hits it ignored;
- read around the strongest hits with `flowspace3 get <address>` before
  answering (`-quick` turns this off), and treat conversations as history to
  check against current code;
- dig further itself if it needs to: `flowspace3 search` with `--repo`,
  `--source`, `--path` and `--offset`, `flowspace3 tree conv:<guid>`, and
  `flowspace3 docs list`.

With no message after the quotes, the agent summarises what the hits say. You
do not see the raw hits yourself; the agent does. A hook cannot rewrite your
prompt, so the agent also sees the `fs3 "…"` line as you typed it.

One thing is left out and one is marked, each with a note to the agent saying
how many:

- **Noise is left out.** Conversation turns with no summary that are tool
  output or under 160 characters (process exit lines, pane names). `-raw` keeps
  them.
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
- **Claude Code only.** The hook reads Claude Code's `UserPromptSubmit` input
  (`prompt`, `cwd`, `session_id`).

## Install

The hook is a single Python 3 script in the GitHub repository AI-Substrate/flowspace3, at
`integrations/claude-code/fs3-prompt-search.py`.

```bash
mkdir -p ~/.claude/hooks
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/integrations/claude-code/fs3-prompt-search.py \
  -o ~/.claude/hooks/fs3-prompt-search.py
chmod +x ~/.claude/hooks/fs3-prompt-search.py
```

Then register it in `~/.claude/settings.json` (every repo) or a project's
`.claude/settings.local.json` (one repo), next to any hooks already there:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [ { "type": "command",
                     "command": "$HOME/.claude/hooks/fs3-prompt-search.py",
                     "timeout": 40 } ] }
    ]
  }
}
```

Claude Code usually picks up settings changes in a running session; if it
does not, open `/hooks` once or restart the session. Prompts that do not start with `fs3`
pass through untouched.
