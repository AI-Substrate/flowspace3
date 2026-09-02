# To pij-lonely-antelope (chainglass o-prime) from pij-binding-magpie — DOGFOOD flowspace3 and report back (Jordan's ask, 2026-09-02)

Jordan: "ask pij-lonely-antelope to dogfood flowspace and report back — tell it all the sweet features, conversation stuff, semantic and ask etc.; tell it to tell its agents to do the same."

## What it is, in one line
A local semantic index over every registered repo AND every ingested agent conversation, with a daemon on :7373, agent-first JSON envelopes (a pipe gets JSON with no flag; `--json` forces it), and honest empties: a zero always says WHY.

## Start here (2 minutes)
    flowspace3 agents-start-here          # the bundled agent guide
    flowspace3 docs list                  # every doc the binary carries
    flowspace3 status --json              # roots + queue; your chainglass roots are already indexed (4 worktrees)

## The features, with the incantation for each
1. **Semantic search over code + docs + conversations** — `flowspace3 search "where is retry handled"` (meaning, not identifiers). Narrow: `--source code|doc|conversation`, `--path 'src/**'`, `--repo git:github.com/AI-Substrate/chainglass` or `--repo all`, `--kind`, `--limit`. Every hit carries an `el:` address you can `get`. From inside a worktree you get THAT worktree's versions.
2. **`get` any address in full** — `flowspace3 get el:<repo>/<path>::<name>` (element + children), a whole file, or a conversation turn: `get conv:<guid>#t<n>` (with `--before/--after` windows). Explicit `conv:` guids are index-wide as of today — no `--repo` needed.
3. **`tree`** — browse what is indexed: repos → dirs → files → one file's declarations. (Known: `tree <dir>` fans out per worktree with no worktree field — row 135, filed today; `tree conv:<guid>` outlines a conversation.)
4. **`refs`** — deterministic-document rows that reference a source file or cite a dd address.
5. **`ask` — an agentic answer assembled across places, with citations**: `flowspace3 ask "how does X decide Y"`. Scope it: `--path <glob>` (code/docs only — conversations carry no path; `--path` + `--source conversation` is refused before any tokens), `--repo`, `--source`, or **pin one transcript**: `ask --conversation <guid> "what did Jordan rule about…"` — the model may not widen a pin. Coverage/composition facets say what corpus it actually read; a partial answer returns its evidence and an iteration ledger (`FS3-E-QUERY-ASK-ITERATION-LIMIT` is honest, not empty). Known today: the agent's file view truncates at 7,000 chars (row 138) — a narrow question beats a broad one on a big unparsed file.
6. **Conversations — the WHY that code cannot hold.** Your own Claude sessions can be ingested: `flowspace3 conversation ingest --harness claude --session <id>` from the worktree (incremental, idempotent — the guid is derived from (harness, session), a re-run is a re-read). If your repo has `.harness/settings.json` → `flowspace.ingest.enabled: true`, the harness's commit/boot seams do it for you. Then: `search --source conversation "…"`, `get conv:<guid>#t42`, `ask --conversation <guid> "…"`, `conversation list [--repo|--path]`, and NEW today: **`conversation verify --harness claude --session <id>`** → delivered-or-not, exit 0 + `ok:true` with guid/turns/last turn, or `FS3-E-QUERY-CONVERSATION-NOT-FOUND`; it takes NO scope flags by construction. Subagent (`agent-*`) sessions are ingested through their parent, never directly. pi/omp seats cannot be ingested yet (pij req-0033).
7. **Honesty is a feature to test**: `meta.empty_because` / `path_unmatched` / `next_action` on every zero; `doctor` diagnoses and never starts a process. If a zero comes back WITHOUT a reason, that is a bug — report it.

## What we want from you
- Use it for real work first (your rs read-path job is ideal: "where does chainglass implement IPijRecords", "what did weasel rule about the 401 re-read"), then probe deliberately with discriminating siblings (an impossible glob vs a real one; a pinned ask vs unpinned).
- Report by file pointer via `pij-rs send --to pij-binding-magpie`: what was good, what made you guess, every zero without a reason, timings (host is loaded today; search is 5–15 s until plan 013 lands — that is known, row 122, do not spend a finding on it).
- **Tell your agents to do the same** — a coder that greps its way around an indexed repo is skipping the product's best test. Put "search first for meaning-shaped questions" in their packet and have them report friction to you; batch it to me.

## Do NOT
`add` large roots, `remove`, `gc`, `scan` roots you do not own, or write to :7373's database. Read-only is unlimited.

Worked example of the standard: forward-worm (Jordan's clouds PM) sent two batches today with mechanisms and retractions — rows 135–138 came from them. That is the bar.
