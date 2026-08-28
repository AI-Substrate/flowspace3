- id: DL-001
  kind: difficulty
  description: "Coder packet path docs/plans/006-worktree-diff/packet-coder-uc.dd.md is absent in the assigned worktree; lean-ctx ls misleadingly fell back to listing the repo root."
  severity: blocking
  workaround: "Verified the absolute path with stat and reported the missing handoff to the PM."
  suggested_encoding: "Make packet dispatch verify the target worktree contains the packet before spawning the coder, and make lean-ctx ls fail on a missing path."
  fp: 0603b1bd0534
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:01:10.455Z"
- id: CONF-001
  kind: confusion
  description: "Builder implement/progress requires marking a dd task in_progress, but builder/plan schema only permits unchecked, checked, blocked, human-skipped, or na; ddocs correctly refused the write."
  severity: degrading
  workaround: "Leave tk-e203 unchecked while active and record work in the execution log; set checked only after proof."
  suggested_encoding: "Align builder progress guidance and builder/plan schema on whether active work has a representable state."
  fp: 2e514e303715
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:10:57.387Z"
- id: DL-002
  kind: difficulty
  description: "LSP find-references for SearchFilters, SearchHit, and SearchResults returned No language server found in the assigned Rust worktree."
  severity: degrading
  workaround: "Reported immediately; used exhaustive exact-identifier search to enumerate callsites after the required LSP attempt."
  suggested_encoding: "Provision rust-analyzer/LSP project detection in OMP worktrees and add it to harness boot diagnostics."
  fp: 72d75aae118c
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:11:43.416Z"
- id: CONF-002
  kind: confusion
  description: "harness commit landed refs/notes/ai for eeb5b0e, but the note records model unknown despite the canary-confirmed github-copilot/gpt-5.6-sol-fast-1m session."
  severity: degrading
  workaround: "Reported the exact commit and note mismatch to the PM; did not infer ownership from the note."
  suggested_encoding: "Propagate the bound OMP model into git-ai attribution and verify it in harness commit output."
  fp: 73f6d46b765b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:34:48.170Z"
- id: DL-003
  kind: difficulty
  description: "Full daemon first_light suite hit a fixture git commit collision: editing_one_file_re_indexes_that_file_and_only_that_file found an existing clean repo and git commit returned nothing-to-commit while concurrent fleet tests were running."
  severity: degrading
  workaround: "Re-run the full daemon suite sequentially; treat only a green full rerun as evidence."
  suggested_encoding: "Make test fixture temp paths collision-proof across concurrent processes/seats and remove stale paths before initialization."
  fp: d9c45a319797
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T04:36:39.762Z"
