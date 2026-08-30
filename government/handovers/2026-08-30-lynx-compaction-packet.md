# Compaction packet — pij-instant-lynx (o-prime, flowspace3) · 2026-08-30

**The recovered-memory rule now applies to YOU**: this session (7,679+ turns,
guid `conv:f3a6f4d9-f037-864a-a824-c436aa5febb2`) is fully ingested and
searchable. For ANY detail this packet lacks:
`flowspace3 search "<question>" --source conversation` (post-#80 the default
mixed search sees it too) and `flowspace3 get "conv:f3a6f4d9-...#t<n>"` for
verbatim turns. Jordan's exact words are retrievable — prefer them over this
summary. Subagent transcripts ingested too. Re-ingest to catch the tail:
`flowspace3 conversation ingest --harness claude --session a5a5588f-0979-439f-a1bf-ddf185a089c7`.

## Identity & standing law

- You are **pij-instant-lynx**, o-prime of flowspace3. Jordan directs by
  voice; one-question-at-a-time (1 sentence context + 1 sentence ask);
  numbered lists; he rules, you record — now on THIS BRANCH
  (prime-governance, standing worktree ../fs3-governance) where you COMMIT
  AND PUSH DIRECTLY, no PRs (ruling 2026-08-30-prime-governance-branch.md).
- Main is branch-protected; product changes ride PRs + merge trains you run
  IN BACKGROUND (Jordan: "don't block main prime thread with reviews").
- **Bounce on every merge** (ruling 2026-08-30-always-bounce-daemon-on-merge):
  `git pull --ff-only && harness daemon bounce` (the verb shipped, #77; its
  verify can time out on heavy boots — row 90 — daemon usually up ~2min
  later; verify with the 401 tell on :7373/health).
- Models: PM opus-5 medium · coders sol-fast-1m high · reviewers sol high —
  settings.dd.json here in government/. Spawn → break-pane to own window
  IMMEDIATELY. Ack-before-code, verify-then-relay, receipts-or-not-done.
- Prod daemon: pane %50, :7373 — NEVER tested against; read-only dogfood
  encouraged for every seat.

## Board at compaction (2026-08-30 ~05:00Z)

LIVE SEATS:
- **pij-associated-owl** — PM for plan 009-embed-split (opus-5, worktree
  fs3-embed-split, branch 009-embed-split). Plan+impl-guide+packet all
  authored BY YOU and pushed there. Awaiting its numbered ack — RULE IT.
  Units: u1 store chunk_no key + poison heal, u2 enrich hygiene+chunking,
  u3 read dedupe + tail-retrieval anchor. Jordan's task: "our next task".
- **pij-unhappy-mollusk** — w-ask-budget-honesty (row 71: ok:true/
  answer:null on budget exhaustion becomes honest terminal + citation
  salvage). Plan was acked; mid-build.
- **pij-light-bovid** — w-ask-conv-scope (row 85: ask --conversation
  pinning + --source filters). Was HOLDING for #80 to merge — #80 IS
  MERGED; it should now be cutting its branch. Check on it.
- **pij-cloudy-krill** — standing read-only queue/ingest-efficiency monitor
  (Jordan ordered it live-monitoring the incoming ingest load). Its audit +
  live alerts: scratch/queue-waste-audit.md (this worktree). Verdict MIXED:
  137,410 repeat summarize generations but $0 duplicate LLM spend; the
  empty-string poison alerts fed plan 009.
- **pij-double-halibut** — packet DONE (#77 merged) but seat stuck on a
  provider auth error; stand down + tidy fs3-daemon-bounce worktree.

MERGED TODAY (all live in prod post-bounce): #77 bounce verb, #78 dup-root
fix + 0020 heal (PROD FULLY HEALED: 0 failed scans/embeds, no last_error),
#79+#81 governance cutover (two PRs — #79's git rm silently no-opped on
modified files, #81 completed it), #80 conversations in default mixed
search + composition facet + honest body-less get. Earlier waves: #72-76.

## In-flight decisions / held items

- Row 81 (adopt pij's raw-event preservation + arch-gate hardenings) —
  UNRULED, the standing quiet-window item.
- Row 88 (archived docs outrank source — 2 observers 6 instances), row 89
  (query/ask ledger — Jordan asked for it conceptually, not yet ruled
  dispatch), row 86 (conv-guid trap + whale-lag docs), rows 76/77 (compose
  collision, envelope truncation) — all dispatch-ready.
- Meadowlark (harness-engineering prime): adopting our whole working model
  (plan 091, corrected + approved by you); building `harness convo sync`
  per your brief (scratch/meadowlark-ingest-command-brief.md); its
  operational interview answers: scratch/meadowlark-operations-interview.md.
- Vicuna (pij prime): got the bounce-mechanism answers + arch-compare
  report (scratch/arch-compare-pij.md); ingesting its own + ermine's
  transcripts (copilot store NOT native — import path or backlog a reader).
- Platform evidence row 75 (omp delivery/E-AMBIG) still awaiting any live
  ermine seat.

## Everything defect-shaped lives in ONE place

`government/briefs/backlog.md` HERE — 90 rows, rows 68-90 are today's.
Every brief w-*.md beside it. EXPERIENCES/TENETS in
.agents/skills/pij-team/ (main repo). Worker-roster needs a refresh pass —
today's seats are recorded in this packet, not yet rostered.

## Gotchas that will bite you first

- Governance files are GONE from main (pointer stub only) — never edit
  .harness/government in the main clone; edit HERE and push.
- pij delivery to omp seats can stick at queued (row 75): pane-paste via
  tmux send-keys is the authoritative fallback; same-spawnId = same seat
  (alias rotation, never close phantoms).
- Trains: use the patient loop (update-branch, re-watch, THEN merge);
  gh update-branch → immediate check races GitHub state (UNKNOWN).
- lean-ctx is DEAD fleet-wide (vicuna eradicated it); if a seat cites it,
  the seat predates the purge.
- `harness team tidy --force` deletes dirty files with no rescue — sweep
  `git -C <wt> status --short` + scratch/ BEFORE any force tidy.
