# Successor context — watcher-ignore seat 2 (predecessor pij-uptight-horza died 2026-08-28T00:30Z)

Read the BASE BRIEF FIRST: .harness/government/briefs/w-watcher-ignore.md —
it is unchanged and binding. This note adds only what happened since.

- Predecessor died in a machine-level mass-seat event mid-implementation (pid
  gone, unrequested — not a work failure). Its worktree survives:
  ../fs3-watcher-ignore on branch w-watcher-ignore off 4a3f1d9, with
  substantial UNCOMMITTED work across crates/daemon/src/{watch,enrich,scan}.rs,
  crates/daemon/tests/{watcher,embed_batch,embed_dedupe,support}, 
  crates/parsers/src/discovery.rs, crates/store/src/{lib,roots}.rs.
- RULING ALREADY MADE (binding): Option A — fence extended by
  crates/parsers/src/discovery.rs + its unit tests + docs/services/discovery.md
  for ONE public fn discover_subtree(root, directory, settings) that answers
  reachability by running the SAME WalkBuilder configuration from root,
  depth-limited to the ancestor chain; watch.rs relist() consults it before
  walking a settled directory. Options B (full walk per settle) and C
  (re-implementation in watch.rs) were REJECTED.
- Also ruled: the regression test must cover BOTH a gitignore-dir case AND a
  hidden-dir case (.claude/.harness rows were hidden prunes, not gitignore).
  Embed guard: new store helper referenced_embedding_hashes with the raw +
  smart legs mirroring job_is_still_wanted!, filtering AFTER hash dedupe and
  BEFORE the provider call, mutation-checked via fake-embedder call count.
- Predecessor's plan steps 5-7 (skipped-count if cheap; GC-by-citation +
  cheap look at 3 failed embed jobs; service docs + gate + PR-no-merge) stand.
  NOTE since written: the 3 failed embed jobs are EXPLAINED (oversized >8192-token
  inputs permanently failing the lane) and queued at o-prime level — just cite
  that in your report, do not investigate.

Your first moves: inventory the uncommitted diff against the brief's
deliverables 1-4 and the Option-A ruling; keep what the evidence supports,
finish or fix the rest; gate green in the worktree; PR into main, DO NOT MERGE.
Ack via pij send to pij-instant-lynx (export PIJ_SESSION_ID=<your id> if
sending from the worktree) with your read + numbered plan against the
INHERITED diff before writing code.
