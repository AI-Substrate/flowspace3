- id: DL-001
  kind: difficulty
  description: "Build failed with 'No space left on device' (rustc-LLVM IO failure) mid-cargo-build in my coder worktree. Disk is at 100%: 478Mi free of 1.8Ti. The wave-1 fleet has six sibling worktrees each carrying its own target/ dir: flowspace3 36G, fs3-convo-ingest 8.6G, fs3-watcher-ignore 7.7G, fs3-team-ext 5.9G, fs3-convo-u2 4.8G, fs3-convo-u1b 2.8G, fs3-convo-u1a 184M — ~66G of largely duplicated dependency builds. This blocks every coder seat, not one."
  severity: blocking
  workaround: "Reported to PM before touching anything outside my own fence; reclaiming only my own worktree's target/."
  suggested_encoding: "worktree-per-coder needs a disk story: either a shared CARGO_TARGET_DIR (noting concurrent builds serialise on the target lock), or a 'harness worktree tidy' that cargo-cleans landed packet worktrees, or a boot-stage free-space check that fails loudly at setup instead of as an opaque rustc-LLVM IO error 20 minutes into a packet."
  fp: abe23fd09c49
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:08:44.584Z"
- id: DL-002
  kind: difficulty
  description: "harness commit reported status=degraded with verify=missing on commit 43fe2e1 (probe said connected, buffer null, no note within 5000ms). The refs/notes/ai note DID land afterwards, but it attributes all 141 lines to a HUMAN hash (h_9e71e8b09f7cf2 1-141, humans block only, no agent_id block). A verified commit from the same seat minutes earlier (023dcea) carries agent_id entries with model names. So the degraded path does not merely delay the note: it can produce a note that misattributes agent-written lines as human-authored. This is the DL-011 hazard concretely reproduced, and it is silent unless someone diffs two notes."
  severity: degrading
  workaround: "None available to the seat: the note already exists and rewriting git notes is destructive and not mine. Reported to PM3 instead. telemetry-nudge is inapplicable here because buffer was null - nothing was buffered to drain."
  suggested_encoding: "Either widen the harness commit verification window past 5000ms so a slow collector is not reported as a miss, or - better - make the verification assert the note CONTENT rather than its existence: a note carrying only a humans block for a commit the agent just authored should be reported as a FAILED attribution, not merely a late one. The current check cannot tell 'no note' from 'wrong note'."
  fp: ef315faef54d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:54:03.113Z"
- id: DL-003
  kind: difficulty
  description: "CORRECTION AND ESCALATION of DL-002. My earlier claim was too narrow: I said the DEGRADED commit path can produce a note that misattributes agent lines as human. Evidence now shows the failure is NOT tied to the degraded path. Commit 1e65fea reported status=ok, verify=landed, VERIFIED - and its note attributes exactly my three edited hunks (13-35,86-93,145-163) to h_9e71e8b09f7cf2 = 'Jordan Knight', with NO sessions block at all. Commit 43fe2e1 (degraded) shows the identical humans-only shape. By contrast 023dcea (verified) attributes every changed range to sessions carrying agent_id tool+model, including github-copilot-cli/claude-opus-5, which is this seat. So: harness commit's VERIFIED proves a refs/notes/ai note EXISTS; it does not prove the note attributes the work to the agent. Verified and degraded commits from the same seat, minutes apart, produced both correct and humans-only notes. git-ai DID see the diff regions (the ranges are exactly my edit hunks) but no agent session claimed them. I cannot determine the cause from the seat - I can only show that the reported status does not predict the outcome."
  severity: blocking
  workaround: "None. Do not repair: rewriting git notes is destructive and not the seat's. Detected only by reading note CONTENT and comparing against a known-good note from the same seat."
  suggested_encoding: "harness commit must verify the note's CONTENT, not its existence: assert that a sessions block exists and that the changed ranges are claimed by an agent session, then report attribution as FAILED when the ranges come back human-only. Today the check cannot distinguish 'no note', 'wrong note' and 'correct note', and it reports the wrong-note case as success - which is the case that silently rewrites who built the thing. Ownership questions already route to o-prime rather than git (DL-011); this is the mechanism that makes refs/notes/ai untrustworthy as a fallback."
  fp: 1f877c158f92
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:57:43.594Z"
