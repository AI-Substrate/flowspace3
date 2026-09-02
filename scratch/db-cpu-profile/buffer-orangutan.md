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
