# Retro — embed-split day drain (2026-08-30/31, drained by pij-instant-lynx)

Buffer drained: 11 observations (list below). The 009 coder/PM buffers were
separately rescued and committed by the PM at
docs/plans/009-embed-split/assets/run/buffers/ (main repo) — not re-drained here.

## Observations drained (verbatim ids)

- DL-001: team tidy --force removed a worktree carrying an uncommitted
  substantive file (008 review round). ENCODED SINCE: tidy now stash-rescues
  (proven twice today: halibut + mollusk buffers auto-rescued).
- CONF-001 / DL-008: lean-ctx corruption + unavailability. DEAD — vicuna
  eradicated fleet-wide 2026-08-30; seats spawned pre-purge carry stale
  mandates in-context only.
- DL-002: harness boot >120s with no intermediate verdict — same family as
  row 90 (BOOTING verdict) and row 110(c) (exit-124 must read NO VERDICT).
- DL-003: daemon --sandbox reads ambient auth key -> unauthorized. Related:
  coral's rig copies sandbox daemon.key explicitly (row 99 README).
- DL-004: sandbox benchmark auto-discovered nine sibling worktrees (959 ->
  9x files). Fixed lineage: #69 killed worktree scan churn; discovery
  behaviour in isolated rigs still worth a flag.
- DL-005/DL-006: fs2 benchmark tooling gaps (no graph-output override; DROP
  DATABASE not transactional). fs2-side; no fs3 action.
- DL-007: search returned QUERY-NO-INDEX for a just-written scratch report —
  landed-vs-findable gap; row 96 (per-source status) is the fix.
- DL-009: boot correctly refuses without FS3_TEST_DATABASE_URL in read-only
  review — the guard working; discoverability noted in zakalwe's buffer too.
- CONF-002: doctor said daemon down while a start spent ~108s on
  schema/ddocs/requeue — row 90's BOOTING verdict again, third+ sighting.

## The day in one paragraph

Nine PRs landed and bounced (#80-#88 era): conversations into mixed search,
ask budget honesty, ask conversation pinning, conv read-back P1 (row 100,
report-to-prod ~1h, reporter-accepted), CLI unbrick (row 106), ingest
enablement (Jordan ruling), and plan 009 (chunked embeddings + hygiene +
heal) under full packet discipline. Six external observers across two repos
filed ~12 rows; every friction became a numbered row with evidence.

## What to encode next (the recurring five)

1. Row 90/110(c): NO VERDICT and BOOTING as first-class gate/bounce verdicts
   — timeouts and boots are not reds, and today proved both repeatedly.
2. Row 110: reapers for every isolation mechanism (DBs AND target dirs AND
   worktrees) — "an isolation mechanism must ship with its reaper."
3. Row 109: deterministic clocks for batching-outcome tests — "a flaky test
   makes every green that contains it weaker than it looks."
4. Rows 98/107/111 family: writer-success/reader-failure — "an accept is a
   statement about the request, never about the thing requested"; envelopes
   name what they READ/RESOLVED, trains re-read heads before merging, and
   "a verdict command must never be able to lie."
5. Row 96: per-source indexing status — the landed-vs-findable gap, hit by
   three independent observers.
