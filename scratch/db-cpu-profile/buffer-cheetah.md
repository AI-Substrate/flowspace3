- id: DL-001
  kind: difficulty
  description: "Reviewing plan 012 I could not run 'harness checks' to verify the gate, because the test gate binary (crates/testkit/src/bin/test_suite.rs:17) runs FreshDatabase::sweep_orphans_from destructively against the shared :5433 container at the head of every run — after this PR that sweep covers the whole fs3_ namespace, so gating from a review worktree would have force-dropped other seats' aged databases."
  severity: degrading
  workaround: "Ran cargo fmt --all --check and cargo clippy -p fs3-testkit --all-targets directly instead of the gate, and stated in the review that the gate was not exercised."
  suggested_encoding: "harness checks needs a read-only/no-sweep mode, or fs3-test-suite should skip the sweep unless an explicit flag is passed; a reviewer should never have to choose between proving the gate and protecting siblings' state."
  fp: 430cf7a38394
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:26:51.354Z"
- id: DL-002
  kind: difficulty
  description: "ac-0005 of plan 012 asks for a read-only orphan listing against the prod server before the drop, but FreshDatabase::list_orphans_from has zero callers anywhere in the tree — it is a library function with no CLI or binary surface. The only production caller of the sweep is the destructive fs3-test-suite. The criterion cannot be executed as written without someone first writing a throwaway binary."
  severity: annoying
  workaround: "Reported it as a composition-seam gap in the review rather than trying to run it."
  suggested_encoding: "Any acceptance criterion that names a read-only operator action should be paired with the command that performs it; here, a 'flowspace3 doctor list-orphan-test-dbs' style verb (or a testkit bin) shipped in the same packet."
  fp: 5352fdcb5fac
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:26:51.524Z"
- id: DL-003
  kind: difficulty
  description: "flowspace3 search \"who creates and drops throwaway test databases\" took 45.9s to return. It found the right code (the sweep test, the refusal helper) but the latency is long enough that grep wins on reflex, which is exactly the habit the dogfood rule is trying to break."
  severity: annoying
  workaround: "Waited it out; used grep for exact-identifier lookups as the tool's own guidance allows."
  suggested_encoding: "A latency budget/sensor on flowspace3 search, or a warm-path indicator in the envelope so the caller knows whether a slow response is cold-start or steady-state."
  fp: a2d0b78c2e2d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:26:51.691Z"
- id: DL-004
  kind: difficulty
  description: "I published a measurement script (.harness/temp/agent/ac-0001-ddl-probe.sh) naming 'store' as an example target without warning that 'cargo test -p fs3-store' is a 107-database unguarded CREATE/DROP burst against the shared postmaster. Another seat picked it up, ran the store target, and the container went into crash recovery shortly after."
  severity: blocking
  workaround: "Added an explicit caveat to the script and told o-prime to warn seats off the store target while :5433 is fragile; proposed re-baselining with a small store target (pg_lexical, 2 DBs) instead of the full crate."
  suggested_encoding: "Any shared probe or repro script that drives load at the shared container should carry its blast radius in the usage block AND refuse targets above a database-churn threshold unless an explicit --i-know flag is passed. A script handed between seats is a command; commands need guardrails."
  fp: c7190221044b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:09:44.734Z"
- id: DL-005
  kind: difficulty
  description: "Disk exhaustion took out the docker socket and froze the fleet mid-review. Root shape: per-seat CARGO_TARGET_DIR means every worktree carries a near-identical copy of the same dependency build — measured 45G across five flowspace worktrees (17G main clone, 8.3G, 7.8G, 7.1G, 5.0G), for what is largely the same set of compiled crates."
  severity: blocking
  workaround: "Reported a read-only du triage to o-prime and volunteered my own 5.0G review-seat target dir as first-to-delete, since a read-only reviewer's build cache is pure disposable."
  suggested_encoding: "Share one CARGO_TARGET_DIR (or sccache) across seats in the worktree scaffolding, so N seats cost roughly one build cache instead of N. Failing that, a harness disk sensor that warns at a free-space floor BEFORE the docker socket dies, and a documented 'which target dirs are disposable' order so triage is not improvised during an outage."
  fp: d376d67932ab
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:12:50.031Z"
- id: CONF-001
  kind: confusion
  description: "I published a load-bearing probe script with a blast-radius guard I had never executed. Testing it later required deliberately letting the allowed path start, which meant briefly running cargo and docker during a period I had declared read-only."
  severity: annoying
  workaround: "Bounded the test with 'timeout 3' and junk credentials so no DDL was possible, then verified no orphaned cargo or sampler processes survived, and disclosed the lapse to o-prime."
  suggested_encoding: "Scripts that gate on environment should expose a --check/--dry-run that runs every guard and exits before doing any work, so the guard can be proven without paying for the guarded action. Guard code that can only be tested by triggering it will stay untested."
  fp: a03395fc0bca
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:23:48.131Z"
