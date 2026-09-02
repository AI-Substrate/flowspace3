# Retro 2026-09-02 — plans 012 (fresh-db-serialise, +012b) and 014 (jobs-retention): the drain

Drafted for o-prime (READ-ONLY drain; nothing cleared). Sources, 46 observations: shared harness buffer at
`harness observe --list` on the main clone (11: DL-001..009, CONF-001..002 — o-prime, the db-cpu investigator,
the two disk agents); vendored seat buffers under `scratch/`: closeout-012/session-buffer-012.md (16, coder
junglefowl → resumed as mite, DL-001..014 + CONF-001..002), review-012/session-buffer.md (6, reviewer cheetah),
closeout-014/session-buffer.md (11, coder arach → resumed as barnacle, DL-001..009 + CONF-001..002),
review-014/session-buffer.md (2, reviewer takin). Ids below are `<seat>/<id>`; `shared/` is the harness buffer.

## The run in one paragraph
Two plans born from one profiling report (rows 126/141 and 139): 012 put a process-wide permit under
CREATE/DROP DATABASE and 014 gave the jobs table a 1-day retention plus an index-only live depth. Both shipped
(#95, #99, #98) with cross-model review that found real defects (014's absorbed re-fire that made a failed
scan_file permanently unindexable; 012's checkpoint gate measuring the wrong invariant). The run was fought
on a box that ran out of disk (docker socket died mid-review), a shared prod postmaster that crashed under a
probe script another seat published, a pij wire bump that killed every omp inbox from birth, and an LSP that
returned empty references for the exact symbols under change. Sixteen of the 46 observations are about the
environment lying (empty-but-wrong, ok-but-not-served, queued-but-not-delivered), not about the code.

## Observations, grouped

### A. Tools that answer "empty"/"ok" when the truth is "unknown" — 9 obs
- rust-analyzer `references` returned NO callers for exported `FreshDatabase` methods, `create_database`, and
  `queue_depth` — every symbol the packets changed (012/DL-001, 012/DL-011, 014/CONF-001); diagnostics kept a
  stale type after reload (014/CONF-002). What was done: exact grep after every LSP query, anchored reads as
  authority. Third run in a row (retro 001 §B) — the LSP is a null signal for callers in this workspace.
- `harness commit` "confirmed" path reported connected ingress and then no `refs/notes/ai` note, three times
  (012/DL-003, 012/DL-007, 014/DL-009). Named, not claimed — the design held; the cause is row 156.
- `flowspace3 status` timed out at 60 s while doctor and search were healthy (012/DL-010); search returned
  `FS3-E-STORE-QUERY-FAILED` pool timeout with no occupancy (014/DL-004); search took 45.9 s so grep won on
  reflex (rev-012/DL-003). What was done: doctor + a scoped search used as the index receipt.

### B. The shared postmaster and the test slot — 10 obs
- Probe script published with `store` as an example target; another seat ran it and the prod container went
  into crash recovery (rev-012/DL-004); the same author had never executed the guard (rev-012/CONF-001); the
  012 handoff copied the obsolete :5433 unattributed version whose `--check` would have run bare cargo test
  (012/DL-012). What was done: refused, re-issued the :5434 attributed probe with `--check`.
- Reviewer could not run `harness checks` from a review worktree because `fs3-test-suite` sweeps the whole
  `fs3_*` namespace destructively at head (rev-012/DL-001); ac-0005 named a read-only listing with no callable
  surface (rev-012/DL-002); orphan-sweep test selected ambient DBs (012/DL-002); checkpoint gate counted
  foreign DDL and failed against 25 (012/DL-005 → ROW 126 CORRECTION, attributed baseline).
- Bringing :5434 up: `harness boot` names the prod compose db, not the test postmaster (014/DL-002); first run
  timed out acquiring a connection (014/DL-005); then pg_hba rejected the exact host URL workers were given
  (014/DL-006); :5433 answered "shutting down" mid read-only count (012/DL-004).
  What was done: stop, no container action, ask o-prime — correct every time, ~40 min of seat time each.

### C. Disk, cargo and the box — 6 obs
- ENOSPC inside the exclusive gate (012/DL-008); five worktrees × per-seat CARGO_TARGET_DIR = 45 G of the same
  crates (rev-012/DL-005); 153 s on cargo's package-cache lock for a 1.68 s test (014/DL-007). Incident hygiene:
  macOS `du` in 512-byte blocks gave 2× divergent totals (shared/CONF-001), two agents reaped the same target
  dirs (shared/DL-007), OrbStack prune returned 14 G of 113 G to APFS (shared/CONF-002).

### D. The wire (pij) — 6 obs
- rs seat delivery silently stopped after the OrbStack incident (shared/DL-006); `pij inbox` E-RS wire v1/v2
  after the pij plan-128 cutover (012/DL-014 → row 153); `pij inbox --wait` unsupported for rs and the route
  text does not say so (012/CONF-001); `pij whoami --json` refuses on rs (012/DL-009); legacy `pij report now`
  E-RS (shared/DL-008). What was done: file channel + pane-paste; `omp -c` restart in place.
- Canary handoff twice asserted a path (`assets/reviews/review-012.dd.md`) that did not exist in the worktree
  (012/CONF-002). What was done: refreshed verdict + replies 010-015 taken as the binding record.

### E. Records and rendering — 5 obs
- Three parallel `ddocs set` on one document raced the shared `.tmp` rename; one lost with E452 (012/DL-006).
- Reviewer filed "ddocs build truncates at 768 chars" (rev-014/DL-001) and RETRACTED it the same session: the
  cut was the harness READ tool footer plus a broken `\[+` regex used as confirmation (rev-014/CONF-001 → row
  152 CLOSED). The lesson the reviewer wrote: reproduce a tooling defect with a DIFFERENT tool before filing.
- `harness doctor` degraded for repo-wide reasons (scratch conventions, git-ai, dd siblings) that a packet
  cannot fix, indistinguishable from packet blockers (014/DL-001); whole-repo grep for a config key timed out
  at 30 s over generated probe output (014/DL-003); `.serena/` litter (012/DL-013, 014/DL-008 → row 150).

### F. O-prime's own — 3 obs
- Answered Jordan's worktree question from grep and got it wrong; one search would have been right
  (shared/DL-001). Bounced prod while 013 held the gate slot → false CRITICAL STOP (shared/DL-009 → row 158).
  The profiling itself had to hand-roll a pg_stat_activity sampler and a container-credential dance
  (shared/DL-002, DL-003) — the findings became rows 139/141, the tooling did not.

## Encode next (ranked by seats-per-day it saves; smallest deterministic change first)
1. **LSP reference sanity probe in the coder packet / harness boot.** `harness doctor lsp` (or a packet i-line)
   asks `references` for one known heavily-called exported symbol (`fs3_store::create_database`) and marks
   the LSP UNTRUSTED for callers when the set is empty; packet text then says "grep is the caller authority".
   Where: harness (probe) + pij-team coder template (line). Retires 012/DL-001, 012/DL-011, 014/CONF-001,
   014/CONF-002, and retro-001 §B's three LSP obs.
2. **`harness checks --no-sweep` (or `fs3-test-suite` sweeps only with `--sweep`).** A reviewer must be able
   to prove the gate without dropping siblings' databases. Where: fs3 repo, `crates/testkit/src/bin/test_suite.rs`
   + harness checks flag. Retires rev-012/DL-001; rev-012/DL-002 goes with it if the same packet adds a
   read-only `list-orphans` verb.
3. **Test-slot preflight that runs `select 1` through the EXACT URL the worker receives.** o-prime's "slot
   granted" message is emitted by a script (`bin/test-slot-grant`) that fails unless
   `psql "$FS3_TEST_DATABASE_URL" -c 'select 1'` succeeds from the host, and `harness boot` names :5434 as
   the test target when compose `db` is prod. Where: fs3 repo `bin/` + harness boot layer. Retires 014/DL-002,
   014/DL-005, 014/DL-006, 012/DL-004.
4. **Shared-probe guardrail: any script under `.harness/temp/agent/` that drives the DB must ship with
   `--check` (runs every guard, exits 0 before work) and a blast-radius line in usage; o-prime runs `--check`
   before relaying it.** Where: pij-team skill (dispatch ritual line) + fs3 `bin/` template. Retires
   rev-012/DL-004, rev-012/CONF-001, 012/DL-012.
5. **Canary handoff probes every path it names.** The restart/handoff template ends with
   `for p in <paths>; do test -e "$p" || echo MISSING $p; done` pasted with its output. Where: pij-team skill
   (restart canary template). Retires 012/CONF-002; halves the 012 handoff's two corrections.
6. **Pre-gate disk floor + one shared cargo cache.** `harness checks` refuses to start compilation when free
   space on the target volume is under N GB and prints the disposable-target order; scaffolding sets one
   `CARGO_TARGET_DIR` (or sccache) per box. Where: harness (check) + pij-team `harness team new` scaffold.
   Retires 012/DL-008, rev-012/DL-005, 014/DL-007; makes shared/DL-007 and CONF-001 unnecessary.
7. **`ddocs set` atomic-write with a unique temp file (or a lock error).** Where: dd. Retires 012/DL-006;
   until then the coder template says "never parallelise ddocs writes to one document".
8. **`harness db profile`** — the pg_stat_activity sampler + shape histogram + EXPLAIN on the search path,
   using discovered container credentials. Where: harness (fs3 extension). Retires shared/DL-002, DL-003 and
   the next investigator rebuilding both from scratch.

Routed elsewhere, not re-ranked here: rs delivery liveness / wire-skew named outcome (shared/DL-006, 012/DL-014,
012/CONF-001, 012/DL-009, shared/DL-008) → pij government req-0042 and row 153; `harness commit` note miss
(012/DL-003, 012/DL-007, 014/DL-009) → row 156 to the harness prime; status/search bounded envelopes
(012/DL-010, 014/DL-004, rev-012/DL-003) → rows 122/131 family, fs3 product backlog candidates.

## Already encoded during the run — do not re-encode
- Row 126 / #95 + #99: process-wide DDL permit, seed-age clamp; ROW 126 CORRECTION taught the attributed
  baseline (retires 012/DL-005's premise; the 16 was foreign DDL). Row 124b / #97: dedicated `db-test` on
  :5434 is live — the B-group setup pain was the migration cost of that fix landing mid-run.
- Row 139 / #98: retention 1 day, index-only depth, failed-row revival + boot sweep; prod receipt taken.
- Row 150: `.serena/` in common-dir `info/exclude` (local); `.gitignore` line owed by the next root-touching packet.
- Row 152: CLOSED as viewer-side; the reviewer's "reproduce with a different tool" lesson is the encode.
- Row 153: pij wire bump cause + `omp -c` restart-in-place; fleet-notice-before-merge lesson recorded by pij.
- Row 156: commit note miss handed to the harness prime. Row 157: reviewer records must pass GLOBAL
  `ddocs validate`; `harness team collect <seat>` named. Row 158: gate slot and prod bounce mutually exclusive;
  guard prints migrating application_name + installed_on.
- Row 131 (checks stage streaming) stands from retro 001; 012/DL-008 is the same opacity plus ENOSPC.

## NOT drained — buffer left intact; o-prime clears after review
The shared buffer (11 obs) and the four vendored seat buffers were LISTED only. No `harness observe --clear`,
no `harness record`, no pij message was run by this drafter. O-prime reviews this record, runs
`harness record retro` if it wants the harness-side pointer, then clears its own buffer.
