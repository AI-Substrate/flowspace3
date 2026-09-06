- id: DL-001
  kind: difficulty
  description: "018 coder worktree does not contain .harness/government/how-we-work.md required by AGENTS; packet supplies operating constraints but manual is unavailable within the absolute-path read fence"
  severity: degrading
  workaround: "Use dispatched packet; ask o-prime for the allowed manual path"
  suggested_encoding: "Include the required read-only operating manual in worktree bootstrap or name its permitted shared path"
  fp: 88c8c4f4548b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T06:08:55.351Z"
- id: CONF-001
  kind: confusion
  description: "harness boot invokes harness checks internally, conflicting with the coder packet requirement to boot before edits but request the shared checks slot first; canceled boot on discovery"
  severity: degrading
  workaround: "Request o-prime ruling and checks slot before boot"
  suggested_encoding: "Expose a no-checks boot mode or state boot also requires the gate slot in worker packets"
  fp: 2e46768ce5f5
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T06:10:17.200Z"
- id: DL-002
  kind: difficulty
  description: "018 bootstrap cargo build --all-targets passed but boot verdict is degraded because the default compose db is absent; packet permits test DB only on :5434, not the compose :5433 target. Boot also warned rs session identity cannot resolve for conversation ingest"
  severity: degrading
  workaround: "Do not start default compose; report build receipt and request ruling for :5434-only bootstrap proof"
  suggested_encoding: "Let boot accept the explicit test database endpoint and distinguish rs identity availability"
  fp: 419400f770d6
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T06:11:44.696Z"
- id: DL-003
  kind: difficulty
  description: "rust-analyzer ready but LSP definition on fs3-018-convo-poller/crates/store/src/ingest_cursors.rs line 103 symbol SourceCursor returns No definition found; earlier convo_ingest.rs line 274 AppState also empty. Seat cwd is main clone, not assigned worktree"
  severity: degrading
  workaround: "Try explicit worktree workspace registration; report exact request to o-prime"
  suggested_encoding: "Start coder seats and their language servers in the dispatched worktree"
  fp: a80d2e3910f6
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T06:13:26.578Z"
- id: CONF-002
  kind: confusion
  description: "flowspace3 search configuration template worktree_reconcile_ticks indexing --path crates/* returned only WorktreeSupervisor implementations, not the configuration template; scores around 0.59 but semantically wrong for the asked template location"
  severity: annoying
  workaround: "Use exact TEMPLATE identifier lookup for the constant owner"
  suggested_encoding: "Add template definition fixture query to search relevance evaluation"
  fp: 51b512b2b13d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T06:40:28.655Z"
- id: DL-004
  kind: difficulty
  description: "fs3_testkit::sealed pins FS3 config/database but inherits HOME; adding default-enabled native conversation polling makes old real-daemon tests read developer session stores unless the seal owns HOME too"
  severity: blocking
  workaround: "Hold actual-daemon suite; request HOME pin in sealed and use explicit tempdir homes in new tests"
  suggested_encoding: "Set HOME to config_dir/home in sealed and defend its recorded child environment"
  fp: 78580defc348
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T06:46:43.775Z"
- id: DL-005
  kind: difficulty
  description: "018 live smoke at conversation_poll_ticks=1 took 10.8s to deliver two appended sessions: a fixed 6s stagger exceeds its 5s cadence, and GREATEST(not_before) on re-enqueue can postpone pending work repeatedly"
  severity: degrading
  workaround: "O-prime approved min(stagger, cadence/cap), preserving six seconds at the default 60s cadence"
  suggested_encoding: "Cadence-bounded effective-stagger method plus real eligible-queue append delivery test"
  fp: 88958bb91340
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T07:01:02.240Z"
- id: DL-006
  kind: difficulty
  description: "Background harness checks failed in fs3-test-suite; its JSON error line was truncated before the test failure, and already-delivered hub job snapshots retain status but omit result/error text. No gate log artifact was written by checks"
  severity: degrading
  workaround: "Identify crate from retained test names and request a focused rerun with stdout/stderr persisted before printing"
  suggested_encoding: "Retain readable resultText/error text for settled failed jobs and persist gate JSON outside tool rendering"
  fp: e9bc73e8a436
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T07:08:49.821Z"
- id: DL-007
  kind: difficulty
  description: "Eval runtime lost all persistent globals after launching the final gate via subprocess.Popen; gate PID 60805 vanished and the streamed log remained empty, so no verdict was recoverable. No orphaned cargo or fs3-test-suite process remained"
  severity: degrading
  workaround: "Use a synchronous Python log-capture wrapper launched as a supervised bash job; completion and exit stay tool-owned and logs survive renderer/runtime loss"
  suggested_encoding: "Do not launch unawaited subprocesses in eval; expose reliable finite-job logging and retain completed error results"
  fp: 0788b42ff66f
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T07:29:24.770Z"
- id: INS-001
  kind: insight
  description: "Pre-PR review of plan 018 found that a fair capped path-order queue could still defer live OMP and fresh Claude behind 100 cold files, violating ac-0008 despite green cap tests; the approved correction adds live/new tiers, reserved backlog capacity, and a mutation-backed next-pass test"
  suggested_encoding: "Keep live-priority and sustained-arrival backlog-reservation scenarios in the poller suite; cap/fairness alone is not freshness proof"
  fp: cf2f54ac47a3
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T08:26:23.885Z"
- id: INS-002
  kind: insight
  description: "Jordan required job-level spend proof because correct cursors alone do not prove zero new LLM work; the ingest report was discarded by the job wrapper and ingest subjects were blank. The new actual-daemon test proves unchanged/rescan zero enrichment deltas, with ledger-bypass mutation red, and report counters now appear in logs/status/doctor"
  suggested_encoding: "Retain actual-job spend regression and report-derived counters; never infer spend from timestamps or cursor positions"
  fp: 89a4e56d177b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T09:25:02.879Z"
- id: DL-008
  kind: difficulty
  description: "Review confirmed that poller lag counted cap-deferred files and zero-record reads returned without acknowledgment; unchanged cwd-less files also lacked negative caching and retained New priority. Design now separates pending submission, successful no-content acknowledgment, and cached unresolved cwd; unconditional empty cursor commit is constrained by conversation FK and timestamp provenance"
  severity: degrading
  workaround: "Submit numbered design before edits; no cargo while reviewer owns slot; add permanent reviewer probes after ruling"
  suggested_encoding: "Submission-aware lag and stamp-keyed read/probe acknowledgments, with cold-backlog and zero-record lifecycle tests"
  fp: 56666b6d35fc
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T09:46:03.987Z"
- id: DL-009
  kind: difficulty
  description: "Mutation database cleanup attempted psql, but it is not on PATH"
  severity: degrading
  workaround: "Locate the installed PostgreSQL client explicitly"
  suggested_encoding: "Expose a test-database inspection command or client path in the harness briefing"
  fp: 676fe20b328c
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T10:44:15.244Z"
- id: CONF-003
  kind: confusion
  description: "Eval-routed todo updates reported 62/65 done, but the next native todo call retained 47/65; source and proof results unaffected"
  severity: degrading
  workaround: "Use native todo phase completion after the final push"
  suggested_encoding: "Make eval-routed tool state persist or explicitly report its isolated scope"
  fp: 80841ab05ab7
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T10:54:46.597Z"
- id: DL-010
  kind: difficulty
  description: "Delta harness commit verified attribution, but its optional conversation hook reported session identity unresolvable and ingested nothing"
  severity: degrading
  workaround: "Retain verified commit; file-derived poller does not require hook session identity"
  suggested_encoding: "Make optional hook identity resolution support the active OMP/pij seat explicitly"
  fp: b794ae98a75c
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T10:55:48.543Z"
- id: DL-011
  kind: difficulty
  description: "D1 review confirmed backlog gauge counted lag-map submissions instead of cursor-backed eligible backlog; pending backlog must remain flat until reads complete"
  severity: degrading
  workaround: "Restore existing eligible behind predicate accounting; keep lag and in-flight separate"
  suggested_encoding: "Paired pending and healthy backlog assertions distinguish submission from completed read progress"
  fp: e39e893a0896
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T11:07:43.174Z"
- id: DL-012
  kind: difficulty
  description: "D1 focused tests found one remaining submitted-only behind expectation on the first cwd probe; eligible pre-probe backlog is now one, subsequent cached misses remain zero"
  severity: degrading
  workaround: "Update only the obsolete expected tuple, preserving cache read/log/demotion assertions"
  suggested_encoding: "Keep explicit first-probe and subsequent-cached-pass gauge expectations"
  fp: f36bac1d3a30
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T11:09:34.583Z"
- id: DL-013
  kind: difficulty
  description: "D1 full gate failed streaming.rs:207 progress_is_reported_while_the_queue_is_still_draining: provider-line count zero instead of one; D1 poller19 passed"
  severity: degrading
  workaround: "Hold push; inspect and isolate the streaming failure without changing the D1 source fence"
  suggested_encoding: "Determine whether provider-group log publication needs an explicit observable synchronization boundary"
  fp: 7b64002199ea
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T11:16:53.270Z"
