- id: DL-001
  kind: difficulty
  description: "crates/daemon/tests/health.rs::the_real_binaries_agree_through_a_discovered_config fails DETERMINISTICALLY (not flakily) on any host whose shared flowspace3_test database has accumulated registered roots. Boot probes ddocs tooling for every registered worktree BEFORE it binds (boot.rs:478-492); with 19 roots the daemon published daemon.key after 25s, with 0 roots after 1s, and the test's ceiling is 10s. Two seats in a row (016 coder, then me reviewing) burned time treating this as an unexplained timeout or a load artefact."
  severity: degrading
  workaround: "Ran the same spawn against a fresh empty database on :5434 to get the A/B; reviewed against CI-green-on-the-sha instead."
  suggested_encoding: "Either bound/parallelise the pre-serve ddoc probe, or have the health test spawn against a fresh database rather than the shared flowspace3_test, or make the test's failure message name the registered-root count it booted against instead of blaming FS3_CONFIG_DIR."
  fp: 6e6da9250e28
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T08:01:41.546Z"
- id: DL-002
  kind: difficulty
  description: "A plan's real-usage receipt recipe (016 AC-0005 / BP-0005) tells the seat to kill its scratch daemon but never to unregister the roots it added, so the receipt permanently leaves its largest root registered in the SHARED flowspace3_test database. Row id 19 (/Users/jordanknight/pi-hacking/pij, 2463 files) is 016's leftover; ids 1-18 are earlier seats' leftovers. Each one adds a pre-serve ddoc probe to every subsequent daemon boot in tests."
  severity: annoying
  workaround: "Used freshly created scratch databases (fs3_rev016, fs3_rev016_mirror) on :5434 for the whole review and dropped them at the end, so I added nothing to the pile."
  suggested_encoding: "Receipt recipes that add roots should end with  (or use a per-receipt scratch database, which is one CREATE DATABASE and one DROP). Better: a harness command that reports/reaps orphaned roots in flowspace3_test, since no seat currently owns that cleanup."
  fp: 5d5e11aab0fb
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T08:01:49.942Z"
