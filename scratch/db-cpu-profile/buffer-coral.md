- id: DL-001
  kind: difficulty
  description: "o-prime answered Jordan's worktree-scanning question from grep and got it WRONG (said auto-add/auto-tidy did not exist); a single flowspace3 search for 'where does the daemon detect new git worktrees' returns crates/daemon/src/worktrees.rs at the top. The repo's own rule (CLAUDE.md: search first for meaning-shaped questions) was skipped by the seat that enforces it."
  severity: degrading
  workaround: "re-answered after reading the module"
  suggested_encoding: "o-prime's own orient ritual: any 'how does X work' answer to Jordan starts with the search envelope pasted, then the code"
  fp: 0e746c976aa5
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:41:06.359Z"
- id: DL-002
  kind: difficulty
  description: "CPU diagnosis hit two avoidable probe failures: docker top rejected host ps %cpu fields, and the database has no postgres role, requiring container-specific discovery before pg_stat_activity could be queried"
  severity: degrading
  workaround: "Used docker top supported fields and read only POSTGRES_USER/POSTGRES_DB from container env"
  suggested_encoding: "Add a harness diagnostic that reports hot containers and active PostgreSQL queries using discovered container credentials"
  fp: c8ef2603948f
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:43:05.451Z"
- id: DL-003
  kind: difficulty
  description: "flowspace3 search on prod takes 1.7-2.7s of near-pure CPU: EXPLAIN ANALYZE shows the admission EXISTS filter runs the smart_content text_hash index probe 962k-1.7M times per search (Nested Loop Semi Join over a Materialized Seq Scan of elements), 3.8-6.8M shared buffer hits, while the HNSW vector scan itself costs only 12ms. No pg_stat_statements to see this; had to hand-roll a pg_stat_activity sampler."
  severity: degrading
  workaround: "Sampled pg_stat_activity at 1s for 5min and histogrammed normalised query shapes; extracted the SQL from crates/store/src/embeddings.rs and ran EXPLAIN (ANALYZE,BUFFERS) with real binds."
  suggested_encoding: "harness command 'harness db profile' that runs the pg_stat_activity sampler + top-shape histogram + EXPLAIN on the search path, so the next agent does not rebuild it"
  fp: cf51096f3af2
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:58:08.497Z"
- id: DL-004
  kind: difficulty
  description: "GET /status and the daemon progress reporter both call queue_depth(), which is 'SELECT kind,state,count(*) FROM jobs GROUP BY kind,state' with no WHERE — a Parallel Seq Scan over all 1.016M jobs rows / 892MB, 114,185 block READS (not hits) on 3 cores, every ~6.5s. Done jobs are never purged (1,009,934 of 1,016,092 rows are state='done'). The doc comment on jobs_remaining claims 'jobs_claim_idx leads on state so this never touches the settled history' but jobs_claim_idx no longer exists."
  severity: blocking
  workaround: "Measured with EXPLAIN (ANALYZE,BUFFERS) and a 65s pg_stat_user_tables counter delta (seq_tup_read +10.1M in 65s on jobs)."
  suggested_encoding: "Retention/purge for done jobs + a covering index or a cheap live-only queue_depth; and fix the stale doc comment in crates/store/src/jobs.rs"
  fp: b1e0503f164f
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:58:15.931Z"
- id: DL-005
  kind: difficulty
  description: "Postgres container log shows 917 'checkpoint starting: immediate force wait' in 6h (bursts of 65/min) vs only 54 timed checkpoints. Each DROP DATABASE in the test suite forces an immediate checkpoint; 60 databases exist, 56 are leaked fs3_* test DBs. Result: wal_fpi 1.9M / wal_bytes 11.9GB in 2h, and 739 requested vs 11 timed checkpoints in pg_stat_bgwriter — a full-page-image death spiral driven by test DB churn against the SHARED prod container."
  severity: degrading
  workaround: "docker logs --since 6h flowspace3-db, grep checkpoint reasons; pg_stat_bgwriter + pg_stat_wal."
  suggested_encoding: "Give tests their own postgres container/instance, or use template-based CREATE DATABASE with a dedicated throwaway cluster; add a leaked-test-DB reaper to harness checks"
  fp: d757d83ef636
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:58:23.042Z"
- id: DL-006
  kind: difficulty
  description: "rs delivery to an omp coder seat (pij-mad-crocodile, pane %2644) silently STOPPED after the OrbStack/disk incident: pij-rs send returns queued, the seat receives nothing (no turn since 02:13Z), five rulings sat unread for ~40 min; the same daemon delivers to sibling omp seats fine. Recovered by tmux pane-paste into my own worker."
  severity: blocking
  workaround: "tmux send-keys into the worker pane (never a prime's), text then Enter after a settle"
  suggested_encoding: "pij-rs send must report delivery, not queueing, or expose a per-seat liveness probe; a seat whose inbox socket died should be marked unreachable in pij-rs list"
  fp: 14107eea1ff6
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:48:53.592Z"
- id: CONF-001
  kind: confusion
  description: "macOS du without -k reports 512-byte blocks, not KiB; two agents produced 2x-divergent home-directory totals from the same du -xd1 command during a disk emergency before the discrepancy was caught by cross-checking."
  severity: degrading
  workaround: "Re-ran with du -xsm / du -k and halved the earlier figures."
  suggested_encoding: "A harness disk/space diagnostic that emits normalised bytes (harness doctor disk or similar), so no agent hand-rolls du during an incident."
  fp: 64671323f312
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:53:20.234Z"
- id: DL-007
  kind: difficulty
  description: "Two agents reaped the same cargo target/ directories concurrently during the disk emergency; hundreds of 'rm: No such file or directory' lines and df deltas that cannot be attributed to either agent."
  severity: degrading
  workaround: "Guarded each rm with a -d test and reported the delta as shared rather than claiming it."
  suggested_encoding: "A claim/lock primitive for destructive cleanup (harness reap --claim <path>) or a single owner named in the brief when more than one seat is reaping."
  fp: 98122b83dfaa
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:53:20.440Z"
- id: CONF-002
  kind: confusion
  description: "Freeing docker volumes inside OrbStack does not free host disk: 113 GB reclaimed inside the VM returned only 14 GB to APFS because data.img.raw is a sparse btrfs image that only shrinks on trim. During a disk emergency this makes a correct action look like it failed."
  severity: annoying
  workaround: "Measured data.img.raw directly (142G -> 128G) and reported the lag explicitly instead of re-running the prune."
  suggested_encoding: "Name the trim lag in any runbook step that says 'docker system prune to free space', with the expected host-visible delay."
  fp: 3d4580e0e0cf
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:53:20.624Z"
