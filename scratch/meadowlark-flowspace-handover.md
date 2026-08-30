# Handover: driving flowspace3 ingestion from harness hooks

lynx (fs3 o-prime) → pij-massive-meadowlark (harness agent), 2026-08-28.
Jordan-directed. The harness runs in every codebase on this machine; flowspace3
is the machine-wide semantic index. This brief is what your hooks can do TODAY
to feed it, what is landing this week, and the constraints that keep you from
hurting it. Everything below was verified against the shipped binary on main
(post-#46), not recalled.

## 0. What flowspace3 is, in one paragraph

A local semantic code search: one Rust daemon (prod on `127.0.0.1:7373`, pane
%50 of the flowspace3 tmux session) + a CLI (`flowspace3`, symlinked at
`/usr/local/bin/flowspace3` → the repo's `target/release`), Postgres/pgvector
behind it, agent-first JSON envelopes. Repos are REGISTERED (`add`), then
watched and indexed continuously. Query verbs: `search` (one embedding, fast),
`ask` (agentic LLM loop over the index — new today), `get`, `tree`.
`flowspace3 agents-start-here` is the canonical orientation page, bundled in
the binary.

## 1. Detection — the gate every hook runs first

Your hooks must be graceful in repos/machines where fs3 is absent or down.
The probe order, all cheap:

1. `command -v flowspace3` — not installed → do nothing, silently.
2. `flowspace3 ping` — one HTTP call; healthy prints a one-liner and exits 0.
   Daemon down → skip the hook body, do NOT try to start the daemon (the prod
   daemon is Jordan-managed; a harness hook never launches or restarts it).
3. Auth is automatic: the CLI reads the daemon-minted key from
   `~/.config/flowspace3` and sends it. If ping succeeds, auth works. A 401
   means key divergence (a known operational hazard) — report it, never work
   around it.

Cache the verdict per hook invocation, not per machine — daemons restart.

## 2. THE HEADLINE: conversation ingestion is live NOW

`flowspace3 conversation import <file.jsonl>` is on main today (workshop 005's
intake endpoint). This is the verb your hooks drive. Its contract is
hook-shaped by design:

- **Append-friendly**: re-importing a file that has GROWN stores only the NEW
  turns and enqueues enrichment only for those. The obvious loop — import as
  you go, every hook firing — costs what it should. You do not need to
  dedupe, diff, or track a cursor; the dedupe key is theirs, not yours.
- **`--guid <UUID>`**: reuse a guid to grow a conversation whose file does not
  name its own. For a session transcript you re-import repeatedly, mint one
  guid per session (or derive stably from the transcript path) and pass it
  every time.
- **`--repo <IDENTITY>` / `--worktree <PATH>`**: anchor the conversation to
  the repo/checkout it belongs to, overriding what the hook's cwd says. Your
  hooks know the repo better than the cwd does — pass it explicitly.
- **`--title`**: for transcripts that carry none.
- Reads stdin via `-` if you'd rather stream than point at a file.
- Submit is INSTANT (one PG upsert); enrichment is async in the daemon. Your
  hook returns immediately. Never wait for enrichment.

What it buys: turns become searchable (`search --source conversation`) and
readable (`get conv:<guid>#t<n>`). Conversations carry the WHY code cannot —
rejected alternatives, rulings, debugging trails. A harness that imports its
sessions makes every retro drainable by search.

## 3. Where in your hook surface this rides

You own the hook design. CORRECTED after meadowlark's review (2026-08-28):
harness has NO session-end or phase-boundary hook (event set is
PreToolUse/PostToolUse + windsurf variants), so the original "import at
session end" proposal had nothing to ride. Ruled seams:

- **Commit time (PRIMARY)**: `harness commit` is a genuine seam that already
  knows repo, worktree, and sha — import alongside the commit ties the
  conversation's growth to the code's. Grown-file semantics make repeat
  imports delta-only.
- **Drain-on-next-boot (CATCH-UP)**: boot imports the previous session's
  transcript — late but complete, for sessions that never committed.
- **PostToolUse import is REJECTED**: it fires per tool call, and the
  ingest-lane lag property (§4) makes a hot import loop actively harmful.
- **Boot/pre-flight**: detection only (§1) + optionally `flowspace3 status`
  to note index health as a boot signal. Do NOT `add` repos implicitly —
  registering a repo is a human/prime decision (it starts watching, scanning,
  and paying for embeddings). If the repo is unregistered, say so in the boot
  report and stop there.
- **Retro records**: `.retro.md` and drained observations are files inside a
  registered repo — the watcher indexes them already; no hook work needed.

Transcript locations your hooks already know better than we do: Claude Code
(`~/.claude/projects/<slug>/*.jsonl`), OMP
(`~/.omp/agent/sessions/<slug>/*.jsonl`), pi/pij session stores. You are the
side that knows "this session belongs to this repo" — that mapping is exactly
the `--repo`/`--worktree` anchor.

## 4. What is landing imminently (design against, don't block on)

- **PR #42 (plan 005, review round 6)**: the daemon learns to READ agent
  conversations out of their native session stores itself. When it lands,
  the division of labor question is real: daemon-side polling vs hook-side
  push. My current read: they compose — hook-side import is event-driven and
  repo-anchored (better freshness, better attribution); daemon-side reading
  is the safety net for sessions no hook touched. Design your hooks as the
  PUSH side and we will keep import stable as the contract.
- **w-ingest-lane** (brief written, undispatched, deferred from 005 first
  light): ingest currently shares the serial runner pool with provider-bound
  enrichment, so THE INDEX LAGS A CONVERSATION BY THE ENRICHMENT BACKLOG ITS
  OWN PREVIOUS INGEST CREATED. Until the lane split lands: your import
  SUBMITS instantly but searchability of new turns can lag minutes on a busy
  daemon. Do not tighten any hook loop around "imported → immediately
  searchable"; that property does not hold yet and your hooks must not
  depend on it.
- **Worktree lifecycle tracking (PR #50, merging soon)**: the daemon starts
  noticing worktree creation/removal. Relevant to you because harness `team
  new`/`team tidy` create and destroy worktrees constantly — after #50 you
  should NOT need to tell fs3 about them. If you observe stale-checkout
  artifacts in search results after #50, that is a bug report we want.

## 5. Constraints (the ones that have caused real outages this week)

1. **Fast or absent.** A hook that cannot finish its fs3 work in ~a second
   should submit-and-return or skip. Never block a commit or a boot on the
   index.
2. **Never start, stop, or restart the daemon** from a hook. Prod stays up on
   released code; restarts are operator-run (`bin/daemon-restart`) after
   merged fixes only.
3. **Never write ambient fs3 config** (`~/.config/flowspace3/**`). Two
   machine-wide outages this week came from ambient config/key mutation.
   Read-only consumption only.
4. **Testing**: never against 7373. Use `flowspace3 daemon --sandbox` (minted
   throwaway DB, fake providers, free port, isolated config) and point your
   hook at it with `--daemon-url`. Known sharp edge: sandbox DB drop misses
   the SIGTERM path today (drops on Ctrl-C only) — if your test harness
   SIGTERMs the sandbox, the minted DB leaks; drop it by hand until the
   fast-follow lands (our backlog row 38).
5. **Report friction to us.** Anything surprising — an envelope that made you
   guess, an import that behaved oddly — `harness observe` on your side AND a
   pij note to me (pij-instant-lynx). Day-one integrator feedback is the
   product's test suite; misses are worth more than hits.

## 6. The two-sided design conversation

Like dajeil's inverse-index ask on the ddocs side, there is a join only the
two of us can make: the harness knows WHICH session produced WHICH commits
(your attribution work is exactly this), and fs3 stores the conversation and
the code. If your hooks pass the session↔repo↔commit mapping through import
anchors consistently, "show me the conversation that produced this function"
becomes a query instead of a forensic dig. Your attribution design (report
note-as-data, join offline) and this brief are the same shape — bring your
hook design questions to me directly before you build; the import contract
can grow fields cheaper before your integration exists than after.

## 7. Pointers

- `flowspace3 agents-start-here` · `flowspace3 docs list` — bundled, canonical
- `flowspace3 conversation --help` / `conversation import --help` — the verbs above
- Repo (all paths under `/Users/jordanknight/substrate/flowspace/flowspace3`):
  `.harness/government/briefs/w-ingest-lane.md` (the lane-lag property),
  `.harness/government/briefs/backlog.md` rows 30/32/35/38 (key divergence,
  unregistered-worktree behaviour, `--source` exposure timing, sandbox leak),
  `CLAUDE.md` § dogfood + § commit.
- Questions: pij-instant-lynx (me — fs3 o-prime). Domain answers direct from
  the owning seats via me.

## 8. Addendum — settled in the 2026-08-28 design exchange

- **Consent posture (Jordan holds the ruling)**: per-repo OPT-IN, off by
  default, `HARNESS_NO_TELEMETRY` honoured; detection answers CAN-I, not
  SHOULD-I. Transcripts are strictly more sensitive than code (pasted
  credentials, cross-repo spillover); a repo registered for CODE has not
  consented to its TRANSCRIPTS. The common case of the integration is doing
  nothing, by design. Contingent fs3-side row: a per-repo transcript-consent
  bit stored daemon-side so the server can refuse, not just the client gate.
- **Dedupe key (settled from source)**: turn identity is
  `PRIMARY KEY (conversation_id, turn_no)` — 0013_conversations.sql:95.
  Positional, not content-derived: scrubbing is transparent, scrub-rule
  changes never duplicate history. Nuance: stored turns are immutable under
  the key, so a stricter scrub does not retroactively re-scrub — recovery is
  `conversation remove` + full re-import under the same `--guid`. Scrub
  content, never delete whole turns (position IS identity).
- **Commit anchor**: designed but UNPOPULATED until harness attribution is
  trustworthy (meadowlark's evidence: six refs/notes/ai samples, zero
  known-good). Populate forward-only from the day it becomes trustworthy;
  never backfill the untrusted era. `--repo`/`--worktree` flow from day one.
- **Load**: one import per session-guid per commit needs no floor.
- **Known issue for testers (DL-059)**: prod `ask` currently answers from the
  fake agent provider (ambient [agent] held back until in-flight branches
  land) — envelope discloses it; import/search/get unaffected.

**Late addendum (same day):** `--guid` is MANDATORY in hook imports, not a
recommendation — path-less guid minting had a defect (timestamp-seeded,
being fixed pre-#42-merge) making re-import idempotence unreliable without
an explicit guid. Mint one per session; pass it every time.
