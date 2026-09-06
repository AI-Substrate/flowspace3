# How flowspace3 gets Claude session state — a code review

**Asked:** "in the harness we use git-ai to save session state and telemetry. I
thought we read git-ai session state. Is it that we go direct to their own
implementations in flowspace3?"

**Short answer:** yes — for Claude and omp, fs3 reads the harness's own native
session files directly and git-ai is not in the path. git-ai IS a fourth fs3
source, but only as the store for copilot (which has no native transcript),
and that route has never worked on this machine because the daemon points at
a filename that does not exist. Nothing triggers any of it automatically.

Everything below is read from source at `3ebfd58` (main) and proved on prod
2026-09-06 00:15–00:30Z.

---

## 1. Three systems capture a Claude session; fs3 reads one of them

| writer | trigger | where it lands | what it holds |
|---|---|---|---|
| **Claude Code itself** | every content block | `~/.claude/projects/<slug>/<uuid>.jsonl` + `<uuid>/subagents/*.jsonl` + `<uuid>/tool-results/` | the full transcript: one line per content block, subagent sidecars, spilled tool output |
| **git-ai** | `PostToolUse` hook → `git-ai checkpoint claude --hook-input stdin` (`~/.claude/settings.json`) | `~/.git-ai/internal/metrics-db` (sqlite, 4.9 GB, 1.13 M rows, live) | `event_kind=5` rows, `tool='claude'`, `external_session_id` = the Claude uuid, git-ai's own `session_id` = `s_…`. My session `a5a5588f` has 50,741 rows there. |
| **engineering-harness** | `PostToolUse` hook → `harness-hook.sh hooks fire claude-code --phase post` | `~/.harness/hooks/<sha>.json` | one record per bash command (head, index state, command text) — telemetry, not transcript |

**fs3 reads the first.** `ClaudeSource` (`crates/providers/src/conversation_sources/claude.rs`):

- `resolve()` (`:199`) → `<root>/<session_id>.jsonl`, refusing if absent, then
  re-globs `<session_id>/subagents/*.jsonl` on every call so a subagent
  spawned mid-ingest is still found (`:255`).
- `read_incremental()` (`:224`) → `tail::read_lines` with a durable byte-offset
  cursor (torn-line, rotation and truncation handled once in `tail.rs`), then
  `merge_records` groups assistant blocks by `message.id` — 38 records become
  13 turns on the committed fixture — with the ordinal fixed as the FIRST
  block's uuid. That grouping rule is FROZEN: change it and every stored
  conversation silently doubles on the next poll (`:60-75`).
- Spilled tool results are re-attached from `<id>/tool-results` (`session_dir`, `:303`).

The native file is read because it is the richest of the three: git-ai's rows
are per-event metadata (345–3,176 bytes each on the newest rows) and carry no
subagent sidecars or tool-result spills.

## 2. git-ai IS an fs3 source — for copilot — and it is broken

`MetricsDbSource` (`metrics_db.rs`, 1,106 lines) is a careful reader: repo
scope is REQUIRED by the type (an unscoped read leaks every project on the
machine), the cursor is the `id` column (an `INTEGER PRIMARY KEY` alias for
rowid, so `VACUUM` cannot move it), prune is detected as cursor > max(id)
and re-read from zero with ledger dedupe, and it opens `mode=ro` twice over.
`conversation_join.rs:89` names its role: copilot has no native store; its
sessions exist only as git-ai metrics rows.

**The daemon opens the wrong file.** `convo_ingest.rs:538`:

```rust
Box::new(MetricsDbSource::new(
    home.join(".git-ai/metrics.sqlite3"),
    RepoScope::remote_url(remote),
))
```

git-ai writes `~/.git-ai/internal/metrics-db`. `~/.git-ai/metrics.sqlite3`
does not exist and, as far as I can find, never has. Proof on prod:

```
$ flowspace3 conversation ingest --harness metrics-db --session a5a5588f-… 
  "accepted": true            ← the route enqueues without checking
… daemon log:
  WARN FS3-E-QUERY-INVALID provider failure: metrics-db: cannot open
  /Users/jordanknight/.git-ai/metrics.sqlite3 read-only: unable to open database file
```

Root cause: the test fixture was harvested from the real path
(`crates/testkit/fixtures/conversations/metrics_db/PROVENANCE.md:4` —
`source /Users/agent/.git-ai/internal/metrics-db`) and saved as
`metrics_db/metrics.sqlite3`; the fixture's filename was then written into the
production constructor. Consequences:

- **github-copilot conversation ingest has never worked here.** Copilot seats
  are ~41 of the 908 rows in the pij registry.
- The git-ai route is not available as a fallback for claude/omp either.
- The failure is accept-then-fail: the CLI prints `queued`; the operator
  learns only from `status.last_error` or the daemon log. DL-004.

## 3. Nothing fires ingest — the design assumed hooks that were never installed

`convo_ingest.rs:249`:

> Ingest is fired from HARNESS HOOKS, which run often and must not wait: the
> route ENQUEUES and returns, and the daemon's own runner does the work.

That is the design, and the enqueue side is built (dedupe key = address, a
burst collapses to one job). The harness exposes the verb —
`.harness/extensions/convo/extension.ts`, `harness fs3-convo ingest`, a thin
passthrough to `flowspace3 conversation ingest`. **But no hook anywhere calls
it:**

- `~/.claude/settings.json` hooks: `SessionStart` → pij bind;
  `PostToolUse` → git-ai checkpoint, harness-hook fire; `Notification` →
  chainglass bell. None mention fs3, ingest, or fs3-convo.
- No `.claude/settings*.json` in this repo carries one.
- `grep -r "fs3-convo\|conversation ingest"` across `.harness/` and
  `~/.harness/` finds only the extension's own source.
- `crates/cli/src/conversation.rs:6`: "The live git-ai/harness submitter is a
  separate future packet against the same endpoint." It was never built.

So every conversation in the index today got there because a human typed the
verb. Measured on 2026-09-05: 106 Claude session files touched in 7 days; 109
conversations indexed in total, ever; newest before I intervened dated
09-02. DL-002.

## 4. The seat route (by pij id) — how a seat becomes a file

`IngestInput::Pij` → the daemon shells `pij sessions --json` (legacy daemon
only; an rs seat is refused naming req-0033) → `conversation_join.rs` routes on
the registry's `harness` FIELD, not the uuid shape (claude and copilot are both
v4, so shape cannot separate them; `pi` means omp) → `(harness, session_id)` →
the native store. The uuid shape is kept only as a consistency check that
reports a lying registry rather than guessing.

This is where the omp `harness_session` finding sits: pij rewrites that field
on every session switch and omp persists lazily, so the row can name a session
with no file yet. fs3 then reports NOT-FOUND with `fix: run ingest`, which is
the wrong instruction. The poller (§6) must key on files, not seat rows.

## 5. What I have NOT verified

- Whether git-ai's `event_kind=5` rows carry enough content to reconstruct a
  Claude turn faithfully (they are small; the native reader exists partly
  because they may not). Not needed for claude — the native store is read —
  but it is exactly what copilot ingest depends on.
- Why git-ai's newest row for my own session is `2026-09-04 22:14:53` while
  the session continued for 26 hours after. Could be the sandbox blocking
  git-ai's socket (CLAUDE.md warns of this), could be a git-ai gap. Not our
  product; flagged, not chased.
- omp: same native-file pattern (`OmpSource`, `~/.omp/agent/sessions/<slug>/<ts>_<uuid>.jsonl`), not re-reviewed here beyond the identity finding.

## 6. What to do

1. **Fix the path** — resolve from git-ai (probe `internal/metrics-db`, fall
   back to `metrics.sqlite3`), fail fast in the route on an unopenable store,
   add a doctor row for the store's existence and newest-row age. Small.
2. **Build the trigger that was designed** — either the hook (a `Stop` /
   `SessionEnd` hook calling `harness fs3-convo ingest -- --session
   $CLAUDE_SESSION_ID --harness claude`, plus the omp equivalent) or a
   daemon-side poller over the native stores keyed on FILES. The poller is
   more robust: it needs no per-harness install, survives a hook being
   removed, and does not depend on pij's seat row. Both can coexist — the
   enqueue is idempotent.
3. **Make quiet distinguishable from dead** — per-root `last_ingested_at`
   and per-store newest-session age in `status`/`doctor` (DL-001).
