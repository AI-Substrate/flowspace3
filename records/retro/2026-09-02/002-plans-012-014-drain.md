---
record_kind: retro
harness_version: 0.14.0
branch: main
repo: https://github.com/AI-Substrate/flowspace3.git
created_at: '2026-09-02T04:28:54.958Z'
agent: o-prime (pij-binding-magpie)
plan_id: 012-fresh-db-serialise,014-jobs-retention
schema_version: '1.2'
retro_id: 2026-09-02T04:28:54Z-o-prime-magpie-674a
started_at: '2026-09-02T00:30:00Z'
ended_at: '2026-09-02T04:28:54Z'
summary: 'Two plans born from one profiling report (rows 126/141 and 139): 012 put a process-wide permit under CREATE/DROP DATABASE and 014 gave the jobs table a 1-day retention plus an index-only live depth. Both shipped (#95, #99, #98) with cross-model review that found real defects (014''s absorbed re-fire that made a failed scan_file permanently unindexable; 012''s checkpoint gate measuring the wrong invariant). The run was fought on a box that ran out of disk (docker socket died mid-review), a shared prod postmaster that crashed under a probe script another seat published, a pij wire bump that killed every omp inbox from birth, and an LSP that returned empty references for the exact symbols under change. Sixteen of the 46 observations are about the environment lying (empty-but-wrong, ok-but-not-served, queued-but-not-delivered), not about the code.'
entries:
- id: HB-DL-001
  kind: difficulty
  description: 'o-prime answered Jordan''s worktree-scanning question from grep and got it WRONG (said auto-add/auto-tidy did not exist); a single flowspace3 search for ''where does the daemon detect new git worktrees'' returns crates/daemon/src/worktrees.rs at the top. The repo''s own rule (CLAUDE.md: search first for meaning-shaped questions) was skipped by the seat that enforces it.'
  target: skill
  severity: degrading
  workaround: re-answered after reading the module
  suggested_encoding: 'o-prime''s own orient ritual: any ''how does X work'' answer to Jordan starts with the search envelope pasted, then the code'
  fp: 0e746c976aa5
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:41:06.359Z'
- id: HB-DL-002
  kind: difficulty
  description: 'CPU diagnosis hit two avoidable probe failures: docker top rejected host ps %cpu fields, and the database has no postgres role, requiring container-specific discovery before pg_stat_activity could be queried'
  target: infra
  severity: degrading
  workaround: Used docker top supported fields and read only POSTGRES_USER/POSTGRES_DB from container env
  suggested_encoding: Add a harness diagnostic that reports hot containers and active PostgreSQL queries using discovered container credentials
  fp: c8ef2603948f
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:43:05.451Z'
- id: HB-DL-003
  kind: difficulty
  description: 'flowspace3 search on prod takes 1.7-2.7s of near-pure CPU: EXPLAIN ANALYZE shows the admission EXISTS filter runs the smart_content text_hash index probe 962k-1.7M times per search (Nested Loop Semi Join over a Materialized Seq Scan of elements), 3.8-6.8M shared buffer hits, while the HNSW vector scan itself costs only 12ms. No pg_stat_statements to see this; had to hand-roll a pg_stat_activity sampler.'
  target: project
  severity: degrading
  workaround: Sampled pg_stat_activity at 1s for 5min and histogrammed normalised query shapes; extracted the SQL from crates/store/src/embeddings.rs and ran EXPLAIN (ANALYZE,BUFFERS) with real binds.
  suggested_encoding: harness command 'harness db profile' that runs the pg_stat_activity sampler + top-shape histogram + EXPLAIN on the search path, so the next agent does not rebuild it
  fp: cf51096f3af2
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:58:08.497Z'
- id: HB-DL-004
  kind: difficulty
  description: GET /status and the daemon progress reporter both call queue_depth(), which is 'SELECT kind,state,count(*) FROM jobs GROUP BY kind,state' with no WHERE — a Parallel Seq Scan over all 1.016M jobs rows / 892MB, 114,185 block READS (not hits) on 3 cores, every ~6.5s. Done jobs are never purged (1,009,934 of 1,016,092 rows are state='done'). The doc comment on jobs_remaining claims 'jobs_claim_idx leads on state so this never touches the settled history' but jobs_claim_idx no longer exists.
  target: project
  severity: blocking
  workaround: Measured with EXPLAIN (ANALYZE,BUFFERS) and a 65s pg_stat_user_tables counter delta (seq_tup_read +10.1M in 65s on jobs).
  suggested_encoding: Retention/purge for done jobs + a covering index or a cheap live-only queue_depth; and fix the stale doc comment in crates/store/src/jobs.rs
  fp: b1e0503f164f
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:58:15.931Z'
- id: HB-DL-005
  kind: difficulty
  description: 'Postgres container log shows 917 ''checkpoint starting: immediate force wait'' in 6h (bursts of 65/min) vs only 54 timed checkpoints. Each DROP DATABASE in the test suite forces an immediate checkpoint; 60 databases exist, 56 are leaked fs3_* test DBs. Result: wal_fpi 1.9M / wal_bytes 11.9GB in 2h, and 739 requested vs 11 timed checkpoints in pg_stat_bgwriter — a full-page-image death spiral driven by test DB churn against the SHARED prod container.'
  target: infra
  severity: degrading
  workaround: docker logs --since 6h flowspace3-db, grep checkpoint reasons; pg_stat_bgwriter + pg_stat_wal.
  suggested_encoding: Give tests their own postgres container/instance, or use template-based CREATE DATABASE with a dedicated throwaway cluster; add a leaked-test-DB reaper to harness checks
  fp: d757d83ef636
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:58:23.042Z'
- id: HB-DL-006
  kind: difficulty
  description: 'rs delivery to an omp coder seat (pij-mad-crocodile, pane %2644) silently STOPPED after the OrbStack/disk incident: pij-rs send returns queued, the seat receives nothing (no turn since 02:13Z), five rulings sat unread for ~40 min; the same daemon delivers to sibling omp seats fine. Recovered by tmux pane-paste into my own worker.'
  target: tooling
  severity: blocking
  workaround: tmux send-keys into the worker pane (never a prime's), text then Enter after a settle
  suggested_encoding: pij-rs send must report delivery, not queueing, or expose a per-seat liveness probe; a seat whose inbox socket died should be marked unreachable in pij-rs list
  fp: 14107eea1ff6
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:48:53.592Z'
- id: HB-CONF-001
  kind: confusion
  description: macOS du without -k reports 512-byte blocks, not KiB; two agents produced 2x-divergent home-directory totals from the same du -xd1 command during a disk emergency before the discrepancy was caught by cross-checking.
  target: infra
  severity: degrading
  workaround: Re-ran with du -xsm / du -k and halved the earlier figures.
  suggested_encoding: A harness disk/space diagnostic that emits normalised bytes (harness doctor disk or similar), so no agent hand-rolls du during an incident.
  fp: 64671323f312
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:53:20.234Z'
- id: HB-DL-007
  kind: difficulty
  description: 'Two agents reaped the same cargo target/ directories concurrently during the disk emergency; hundreds of ''rm: No such file or directory'' lines and df deltas that cannot be attributed to either agent.'
  target: infra
  severity: degrading
  workaround: Guarded each rm with a -d test and reported the delta as shared rather than claiming it.
  suggested_encoding: A claim/lock primitive for destructive cleanup (harness reap --claim <path>) or a single owner named in the brief when more than one seat is reaping.
  fp: 98122b83dfaa
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:53:20.440Z'
- id: HB-CONF-002
  kind: confusion
  description: 'Freeing docker volumes inside OrbStack does not free host disk: 113 GB reclaimed inside the VM returned only 14 GB to APFS because data.img.raw is a sparse btrfs image that only shrinks on trim. During a disk emergency this makes a correct action look like it failed.'
  target: infra
  severity: annoying
  workaround: Measured data.img.raw directly (142G -> 128G) and reported the lag explicitly instead of re-running the prune.
  suggested_encoding: Name the trim lag in any runbook step that says 'docker system prune to free space', with the expected host-visible delay.
  fp: 3d4580e0e0cf
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:53:20.624Z'
- id: HB-DL-008
  kind: difficulty
  description: 'legacy ''pij report now'' fails with E-RS: rs at 127.0.0.1:7461 answered with something this CLI cannot read (rs ping healthy; sends work) — the prime''s status card cannot be posted through the legacy CLI'
  target: tooling
  severity: degrading
  workaround: try pij-rs report directly
  suggested_encoding: legacy pij report must either parse the rs reply shape or fall back to legacy for this seat
  fp: 617c283fea7b
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T03:50:08.345Z'
- id: HB-DL-009
  kind: difficulty
  description: o-prime bounced the prod daemon while plan 013 held the exclusive gate slot; the gate's production migration guard saw 22→23 and raised a false CRITICAL STOP
  target: infra
  severity: degrading
  workaround: ruled false positive from _sqlx_migrations.installed_on vs the bounce timestamp; gate re-run
  suggested_encoding: bin/daemon-restart refuses while a harness checks gate is running on the box; the guard prints the migrating application_name and installed_on
  fp: 71243bf4956e
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T04:22:48.981Z'
- id: DL-012C-001
  kind: difficulty
  description: rust-analyzer references returned no callers for exported FreshDatabase methods; Serena cross-check timed out for create and sweep while finding cleanup callers
  target: tooling
  severity: degrading
  workaround: Keep public signatures unchanged; use Flowspace and exact identifier search only where caller inventory becomes necessary
  suggested_encoding: Add a harness diagnostic that verifies rust-analyzer workspace indexing before reporting an empty reference set
  fp: 60fc892d0d4b
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:46:40.640Z'
- id: DL-012C-002
  kind: difficulty
  description: orphan-sweep integration test using SystemTime::now selected pre-existing shared-server orphans in addition to its scratch names; an exact equality assertion failed after creating three scratch databases
  target: project
  severity: degrading
  workaround: Use a synthetic near-epoch clock so only names minted at epoch 1 are old; remove failed-run scratch databases explicitly
  suggested_encoding: Provide a testkit helper that scopes destructive orphan-sweep fixtures without depending on ambient shared-server contents
  fp: 0cf2e0c3a5df
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T00:53:24.920Z'
- id: DL-012C-003
  kind: difficulty
  description: harness commit 05d7d87cdfee reported connected ingress but no refs/notes/ai entry appeared within 5 seconds; commit stands with authorship unrecorded
  target: tooling
  severity: degrading
  workaround: Keep the named commit outcome in the coder report; do not claim attribution
  suggested_encoding: Make harness commit's missing-note remediation available without truncation and add a deterministic retry/check command
  fp: ada2eb1bfd2b
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:12:55.023Z'
- id: DL-012C-004
  kind: difficulty
  description: 'prod-safe test-server read-only count failed at 2026-09-02 during review delta: flowspace3-db returned FATAL database system is shutting down before list_orphans example could run'
  target: infra
  severity: blocking
  workaround: Stopped immediately; no retry, restart, compose action, sweep, or further database command
  suggested_encoding: Expose shared-postmaster lifecycle/recovery as a harness sensor that attributes the active seat and blocks mutation proofs before connection attempts
  fp: 8dbe70a85050
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:39:08.708Z'
- id: DL-012C-005
  kind: difficulty
  description: 'review-delta hard checkpoint target failed: cargo test -p fs3-store passed 137 tests, but window 2026-09-02T01:48:52Z..01:53:07Z contained 83 immediate forced-checkpoint starts and pg_stat_bgwriter checkpoints_req rose 1171 to 1299 (+128), versus reviewer baseline target 25'
  target: plan
  severity: blocking
  workaround: Stopped without retry or tuning; preserved window, test artifact, log count, and counter delta for o-prime ruling
  suggested_encoding: Add a harness checkpoint-window sensor that isolates the calling test processes and reports concurrent external DDL attribution before judging the threshold
  fp: 7fbe3cc75eb0
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:53:33.264Z'
- id: DL-012C-006
  kind: difficulty
  description: three parallel ddocs set operations against plan.dd.json raced on the shared .tmp rename; ac-0002 failed E452 while ac-0001/ac-0003 reported success
  target: tooling
  severity: degrading
  workaround: Never parallelize ddocs writes to the same document; rerun the failed address sequentially and rebuild
  suggested_encoding: ddocs set should use unique atomic-write temp files or reject concurrent writers with a clear lock error
  fp: a239920f08ee
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:56:58.840Z'
- id: DL-012C-007
  kind: difficulty
  description: review-delta harness commit f3aec311b569 returned degraded after connected ingress, repeating the missing refs/notes/ai attribution seen on 05d7d87
  target: tooling
  severity: degrading
  workaround: Name the degraded attribution outcome; continue using the committed SHA without claiming note delivery
  suggested_encoding: Make harness commit retry or deterministically verify note delivery after connected-ingress misses
  fp: 81831a08e9e8
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:00:02.653Z'
- id: DL-012C-008
  kind: difficulty
  description: exclusive harness checks failed while compiling fs3-store because the worktree target filesystem returned ENOSPC; migration guard then could not read a schema version
  target: infra
  severity: blocking
  workaround: Keep gate slot, remove this worktree's disposable cargo artifacts with cargo clean, rerun harness checks once
  suggested_encoding: Add pre-gate disk-space/target-size backpressure with an actionable cargo-clean recommendation before compilation starts
  fp: 3afd355211ec
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:09:36.631Z'
- id: DL-012C-009
  kind: difficulty
  description: pij whoami --json refused for rs because the rs response lacks verbs and capabilitySchema; plain whoami is required for restart canary identity
  target: tooling
  severity: degrading
  workaround: run pij whoami without --json
  suggested_encoding: give rs whoami a truthful typed JSON envelope or document the capability-gate refusal in the canary recipe
  fp: 888404dcb2b4
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:52:00.914Z'
- id: CONF-012C-001
  kind: confusion
  description: pij inbox --wait 30000 is unsupported for pushed rs seats and returns E-RS; the peer route presents it as the blocking path for external peers without identifying the rs exception
  target: skill
  severity: degrading
  workaround: use non-blocking pij inbox after rs push notification
  suggested_encoding: teach the pij peer route and CLI error next_action that rs seats must await push then run bare pij inbox
  fp: 940e648d67f7
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:52:10.541Z'
- id: CONF-012C-002
  kind: confusion
  description: Restart handoff twice asserted docs/plans/012-fresh-db-serialise/assets/reviews/review-012.dd.md existed in this worktree, but assets/reviews is absent; coding had to rely on refreshed verdict plus binding replies after prime declared nothing else owed
  target: plan
  severity: degrading
  workaround: use refreshed review-012-verdict.md and replies 010-015 as the complete binding record
  suggested_encoding: canary handoff should probe every named path before telling a restarted seat the packet is complete
  fp: 0f8b933397b2
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:54:43.768Z'
- id: DL-012C-010
  kind: difficulty
  description: flowspace3 status exceeded 60 seconds and timed out even though flowspace3 doctor was healthy and search subsequently returned current worktree hits
  target: project
  severity: degrading
  workaround: use doctor plus a successful scoped search as the index-health receipt
  suggested_encoding: status should return a bounded partial envelope or name which queue/root lookup is blocking
  fp: 728e87f5d475
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:58:14.413Z'
- id: DL-012C-011
  kind: difficulty
  description: rust-analyzer references on exported fs3_store::create_database returned no references even though flowspace and the review ledger identify many callers; exact identifier search is required for callsite inventory
  target: tooling
  severity: degrading
  workaround: use exact grep after LSP attempt, then verify edits by anchored reads
  suggested_encoding: add an LSP health probe that asks references for a known heavily-used exported symbol and rejects an empty result
  fp: 8c1b8ff1f7d8
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:59:20.877Z'
- id: DL-012C-012
  kind: difficulty
  description: The plan-012 handoff repeatedly required the revised attributed ac-0001-ddl-probe.sh with --check on :5434, but the copied script is the obsolete unattributed :5433 version and --check would accidentally run bare cargo test
  target: plan
  severity: blocking
  workaround: refuse to run it and request the reviewed script from o-prime
  suggested_encoding: make probe --check validate attribution, container, and test URL before any cargo invocation
  fp: db1aec7b51f3
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T03:08:28.888Z'
- id: DL-012C-013
  kind: difficulty
  description: The LSP diagnostics/reference workflow created an untracked .serena directory in a previously clean packet worktree, requiring explicit cleanup before the scoped commit
  target: tooling
  severity: degrading
  workaround: remove the tool-owned .serena directory and commit only packet paths
  suggested_encoding: configure language tooling to store session metadata outside Git worktrees or add the generated path to the standard ignore set
  fp: 8dfc67d1101f
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T03:27:05.887Z'
- id: DL-012C-014
  kind: difficulty
  description: pij inbox failed with E-RS because the rs daemon returned v1 wire data to a v2 extension; the queued prime reply became temporarily unreadable
  target: tooling
  severity: degrading
  workaround: retry bare pij inbox; do not fall back to legacy because claim state is unknown
  suggested_encoding: negotiate rs wire versions before claiming inbox messages and return a non-mutating upgrade diagnostic
  fp: b6545ec447b4
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T03:50:06.638Z'
- id: DL-R12-001
  kind: difficulty
  description: Reviewing plan 012 I could not run 'harness checks' to verify the gate, because the test gate binary (crates/testkit/src/bin/test_suite.rs:17) runs FreshDatabase::sweep_orphans_from destructively against the shared :5433 container at the head of every run — after this PR that sweep covers the whole fs3_ namespace, so gating from a review worktree would have force-dropped other seats' aged databases.
  target: project
  severity: degrading
  workaround: Ran cargo fmt --all --check and cargo clippy -p fs3-testkit --all-targets directly instead of the gate, and stated in the review that the gate was not exercised.
  suggested_encoding: harness checks needs a read-only/no-sweep mode, or fs3-test-suite should skip the sweep unless an explicit flag is passed; a reviewer should never have to choose between proving the gate and protecting siblings' state.
  fp: 430cf7a38394
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:26:51.354Z'
- id: DL-R12-002
  kind: difficulty
  description: ac-0005 of plan 012 asks for a read-only orphan listing against the prod server before the drop, but FreshDatabase::list_orphans_from has zero callers anywhere in the tree — it is a library function with no CLI or binary surface. The only production caller of the sweep is the destructive fs3-test-suite. The criterion cannot be executed as written without someone first writing a throwaway binary.
  target: plan
  severity: annoying
  workaround: Reported it as a composition-seam gap in the review rather than trying to run it.
  suggested_encoding: Any acceptance criterion that names a read-only operator action should be paired with the command that performs it; here, a 'flowspace3 doctor list-orphan-test-dbs' style verb (or a testkit bin) shipped in the same packet.
  fp: 5352fdcb5fac
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:26:51.524Z'
- id: DL-R12-003
  kind: difficulty
  description: flowspace3 search "who creates and drops throwaway test databases" took 45.9s to return. It found the right code (the sweep test, the refusal helper) but the latency is long enough that grep wins on reflex, which is exactly the habit the dogfood rule is trying to break.
  target: project
  severity: annoying
  workaround: Waited it out; used grep for exact-identifier lookups as the tool's own guidance allows.
  suggested_encoding: A latency budget/sensor on flowspace3 search, or a warm-path indicator in the envelope so the caller knows whether a slow response is cold-start or steady-state.
  fp: a2d0b78c2e2d
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:26:51.691Z'
- id: DL-R12-004
  kind: difficulty
  description: I published a measurement script (.harness/temp/agent/ac-0001-ddl-probe.sh) naming 'store' as an example target without warning that 'cargo test -p fs3-store' is a 107-database unguarded CREATE/DROP burst against the shared postmaster. Another seat picked it up, ran the store target, and the container went into crash recovery shortly after.
  target: plan
  severity: blocking
  workaround: Added an explicit caveat to the script and told o-prime to warn seats off the store target while :5433 is fragile; proposed re-baselining with a small store target (pg_lexical, 2 DBs) instead of the full crate.
  suggested_encoding: Any shared probe or repro script that drives load at the shared container should carry its blast radius in the usage block AND refuse targets above a database-churn threshold unless an explicit --i-know flag is passed. A script handed between seats is a command; commands need guardrails.
  fp: c7190221044b
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:09:44.734Z'
- id: DL-R12-005
  kind: difficulty
  description: 'Disk exhaustion took out the docker socket and froze the fleet mid-review. Root shape: per-seat CARGO_TARGET_DIR means every worktree carries a near-identical copy of the same dependency build — measured 45G across five flowspace worktrees (17G main clone, 8.3G, 7.8G, 7.1G, 5.0G), for what is largely the same set of compiled crates.'
  target: infra
  severity: blocking
  workaround: Reported a read-only du triage to o-prime and volunteered my own 5.0G review-seat target dir as first-to-delete, since a read-only reviewer's build cache is pure disposable.
  suggested_encoding: Share one CARGO_TARGET_DIR (or sccache) across seats in the worktree scaffolding, so N seats cost roughly one build cache instead of N. Failing that, a harness disk sensor that warns at a free-space floor BEFORE the docker socket dies, and a documented 'which target dirs are disposable' order so triage is not improvised during an outage.
  fp: d376d67932ab
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:12:50.031Z'
- id: CONF-R12-001
  kind: confusion
  description: I published a load-bearing probe script with a blast-radius guard I had never executed. Testing it later required deliberately letting the allowed path start, which meant briefly running cargo and docker during a period I had declared read-only.
  target: plan
  severity: annoying
  workaround: Bounded the test with 'timeout 3' and junk credentials so no DDL was possible, then verified no orphaned cargo or sampler processes survived, and disclosed the lapse to o-prime.
  suggested_encoding: Scripts that gate on environment should expose a --check/--dry-run that runs every guard and exits before doing any work, so the guard can be proven without paying for the guarded action. Guard code that can only be tested by triggering it will stay untested.
  fp: a03395fc0bca
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:23:48.131Z'
- id: DL-014C-001
  kind: difficulty
  description: 'harness doctor is degraded before jobs-retention work: transient scratch conventions unprotected, git-ai collection absent, two generated dd siblings missing, and standalone dd CLI unavailable'
  target: tooling
  severity: degrading
  workaround: Proceeding only with canary investigation; no code before o-prime ruling
  suggested_encoding: Make harness boot distinguish packet-local blockers from known repository-wide degraded layers
  fp: e14cbb23f825
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:40:26.338Z'
- id: CONF-014C-001
  kind: confusion
  description: rust-analyzer LSP references returned no queue_depth callers even though the profile and indexed code identify runner.rs and status.rs callsites
  target: tooling
  severity: degrading
  workaround: Cross-checking with Serena symbol references and exact identifier search before ack
  suggested_encoding: Add an LSP smoke probe that asserts cross-crate references resolve for a known exported store symbol
  fp: 7a5202c06a28
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:42:21.667Z'
- id: DL-014C-002
  kind: difficulty
  description: pre-work harness boot is degraded because Docker Compose service db is not running; build passed but integration proof cannot run yet
  target: infra
  severity: blocking
  workaround: Stopping before code as required; will request test DB readiness/gate slot from o-prime in the canary ack
  suggested_encoding: Have harness boot name the isolated :5433 test-postmaster command when production compose db is intentionally not the test target
  fp: aef3805b9beb
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T01:42:57.617Z'
- id: DL-014C-003
  kind: difficulty
  description: exact grep for worktree_reconcile_ticks across the whole repository timed out at 30s during retention config-doc cleanup
  target: tooling
  severity: degrading
  workaround: Narrowing searches to known config and documentation paths
  suggested_encoding: Expose a fast config-key reference command or make repository exact-text search exclude large generated probe output by default
  fp: 3f96a7918ee0
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:01:31.878Z'
- id: DL-014C-004
  kind: difficulty
  description: 'flowspace3 semantic search for the newly written retention path failed with FS3-E-STORE-QUERY-FAILED: pool timed out waiting for an open connection'
  target: project
  severity: degrading
  workaround: Following the envelope's re-run-once instruction; source correctness remains covered by focused tests
  suggested_encoding: Have search report pool occupancy and the blocking query or distinguish saturation from a stopped store
  fp: 254c80a9a736
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:06:49.017Z'
- id: CONF-014C-002
  kind: confusion
  description: rust-analyzer diagnostics continue to type StatusReport.retention as a non-Option after the worktree file was changed to Option and LSP reload completed; exact file reads disagree with LSP
  target: tooling
  severity: degrading
  workaround: Treating exact worktree bytes as authority and deferring compiler confirmation until o-prime clears the database freeze
  suggested_encoding: Make LSP worktree roots explicit and expose which Cargo workspace/file snapshot supplied a diagnostic
  fp: 39a4f5af684d
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:09:13.098Z'
- id: DL-014C-005
  kind: difficulty
  description: first focused store run against the newly mandated 127.0.0.1:5434/flowspace3_test timed out acquiring a connection in all three tests
  target: infra
  severity: blocking
  workaround: Stopped database-backed tests and did not touch flowspace3-db-test; asking o-prime to restore the separate postmaster
  suggested_encoding: Add a preflight that proves the dedicated test postmaster is accepting authenticated SQL before granting the test slot
  fp: ba10923b7ff5
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:10:44.914Z'
- id: DL-014C-006
  kind: difficulty
  description: 'the cleared :5434 test postmaster accepts TCP but rejects the binding host URL: no pg_hba.conf entry for host 192.168.97.1, user flowspace3, database flowspace3_test, no encryption'
  target: infra
  severity: blocking
  workaround: Stopped DB-backed tests without touching the container; o-prime must make the exact mandated host URL authenticate
  suggested_encoding: Make the dedicated-test health receipt execute select 1 through the exact host URL workers receive, not from inside the container
  fp: ab37eafce957
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:28:54.785Z'
- id: DL-014C-007
  kind: difficulty
  description: focused daemon test spent about 153 seconds blocked on Cargo's shared package-cache lock despite a per-seat CARGO_TARGET_DIR; test itself finished in 1.68 seconds
  target: infra
  severity: degrading
  workaround: Kept one local cargo invocation at a time and waited; o-prime still serializes only full gates
  suggested_encoding: Give seats an isolated CARGO_HOME or a harness-level package-cache slot in addition to isolated target directories
  fp: 37eb6a5b0a92
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:37:59.130Z'
- id: DL-014C-008
  kind: difficulty
  description: Serena code-intelligence initialized an untracked .serena directory in the packet worktree; it is outside the jobs-retention fence and must not enter the commit
  target: tooling
  severity: degrading
  workaround: Leaving the directory untouched and excluding it from harness commit paths
  suggested_encoding: Configure Serena project metadata outside task worktrees or root-ignore .serena deterministically
  fp: 151352c92f37
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T02:42:21.982Z'
- id: DL-014C-009
  kind: difficulty
  description: harness commit c5242ea reported connected ingress but refs/notes/ai verification was missing after the commit
  target: tooling
  severity: degrading
  workaround: Commit outcome is explicitly named; proceeding with required push and reporting degraded attribution to o-prime
  suggested_encoding: Make harness commit retry note verification or emit the exact telemetry-nudge command when connected ingress misses
  fp: 436163b2d6a3
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T04:06:17.344Z'
- id: DL-R14-001
  kind: difficulty
  description: ddocs build truncates long table-cell values at 768 chars in the rendered .dd.md sibling. My reviewer packet (docs/plans/014-jobs-retention/packet-reviewer.dd.md) silently lost least-confident hunts (d) and (e) and the ENTIRE disbelieve-the-receipts instruction from rows i6 and owed-1-least-confident; the full text existed only in the .dd.json. It then truncated my own review record's ac-0003 row the same way. A reviewer who read only the rendered .md would have skipped the work that found a CRITICAL defect.
  target: tooling
  severity: degrading
  workaround: Pulled the full row text with jq from the .dd.json source, and told o-prime to read the review .dd.json rather than the .dd.md
  suggested_encoding: Either stop truncating in the ddocs renderer for long cells (emit a fenced block or a footnoted section instead of a table cell), or make the truncation LOUD - append an explicit '[TRUNCATED - read the .dd.json]' marker so a reader knows text is missing rather than silently reading a shortened instruction as complete.
  fp: c5bc1dabccff
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T03:45:11.575Z'
- id: CONF-R14-001
  kind: confusion
  description: 'CORRECTION to DL-001 (same session): DL-001 blamed ddocs build for truncating long table cells at 768 chars. THAT IS WRONG AND RETRACTED. On disk the rendered files are whole — the ac-0003 row of my review record is 1007 chars, the reviewer packet''s longest line is 1313 chars, both tail phrases of the i6 row are present in packet-reviewer.dd.md, and there are zero literal truncation markers in either file. The 768-char cut was in MY OWN READER TOOLING: the harness read tool printed the footer ''[Some lines truncated to 768 chars]'' and the bash tool printed continuation markers on long jq output. The check I used to confirm DL-001 was also broken: I grepped \[+[0-9]*\] where \[+ means one-or-more literal ''['' rather than ''['' followed by ''+'', so it matched an ordinary [0] in the prose and I read one bogus hit as proof. Confirmation bias on a hypothesis I liked. Do NOT open a ddocs backlog item.'
  target: tooling
  severity: annoying
  workaround: Verified against the files on disk with awk line-length and phrase-presence checks after o-prime and the dd prime both failed to reproduce; retracted the claim in the review record and verdict
  suggested_encoding: 'Two things. (1) When a reader tool elides content it should say so in a way that cannot be mistaken for the FILE''s content — the footer is easy to misattribute to the artifact rather than the viewer. (2) Agent-side habit worth encoding: before filing a tooling defect, reproduce it with a DIFFERENT tool than the one that showed it — a single awk length check on disk would have killed this claim instantly.'
  fp: 83732ce6c8a1
  disposition: kept
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-09-02T03:53:59.119Z'
---

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
