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
