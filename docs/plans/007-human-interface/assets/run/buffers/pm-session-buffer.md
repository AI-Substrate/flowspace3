- id: DL-001
  kind: difficulty
  description: "harness boot --json reports compose stage failed (\"service db is not running\") inside a git worktree, even though the shared flowspace3-db container is up and healthy on :5433 — docker compose ps is scoped to the worktree dir, so the compose project resolves empty. Boot reads 'degraded' for every worktree seat, which trains seats to ignore a red boot stage."
  severity: degrading
  workaround: "Verified liveness out-of-band: docker compose ls shows project flowspace3 running from the main clone, and flowspace3 status returns ok:true against :5433."
  suggested_encoding: "Boot's compose probe should resolve the compose project by -p/--project-directory pointing at the main clone (or probe TCP :5433 + a db ping) rather than the cwd's compose context, so a worktree boot is honestly green."
  fp: 83f7415fdb8b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:19:28.782Z"
- id: DL-002
  kind: difficulty
  description: "pij report now rejects a did-string over 280 chars only AFTER the command runs (E-ARG), with no length hint in pij report --help; cost one retry mid-dispatch."
  severity: annoying
  workaround: "Shortened the did-string and re-ran."
  suggested_encoding: "Name the 280-char limit in pij report --help usage text, or truncate with a warning instead of erroring."
  fp: e27928e6dd56
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:21:43.048Z"
- id: DL-003
  kind: difficulty
  description: "A hand-rolled test stub server copied from the established crates/cli/tests/ping.rs pattern blocks forever in listener.accept() when the case under test never connects (clap rejected the args and exited 2). The suite gave no clue which case stalled — it burned a 900-second timeout and then a 240-second one before instrumentation found it."
  severity: degrading
  workaround: "Made the accept loop nonblocking with a 20s deadline in crates/cli/tests/envelope_goldens.rs, and added an expected exit code per case so a usage-error case is asserted rather than silently hanging."
  suggested_encoding: "Move the stub-daemon helper into fs3-testkit with a bounded accept built in, so the next author cannot copy the unbounded version; ping.rs should adopt it too."
  fp: d52050318f60
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:54:33.817Z"
- id: CONF-001
  kind: confusion
  description: "ddocs schema discovery does not walk up to the repo root: running 'ddocs set' from inside a plan folder failed with E401 schema pij-team/impl-guide not found, listing only the doc dir, doc/.dd, doc/.harness/.dd and ~/.dd as roots — while .dd/schemas/pij-team exists at the repo root. The same command from the repo root with a relative path works."
  severity: annoying
  workaround: "Ran ddocs from the repo root with the full relative path to the document."
  suggested_encoding: "Walk up from the document (or cwd) to the git root when discovering .dd/schemas, or name the repo-root candidate in the E401 message so the fix is obvious from the error."
  fp: f08281210a7d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:14:07.560Z"
- id: DL-004
  kind: difficulty
  description: "pij canary replies from three fresh seats all mis-reported their own identity: two said pijId=unknown/unavailable, one reported its WORKTREE NAME as pijId and its BRANCH as spawnId. The seat cannot see the id the registry minted for it (that id arrives in the ready-ping the PARENT receives), so the canary's identity check is verifiable only from the parent side."
  severity: degrading
  workaround: "Cross-checked each seat against the ready-ping the parent received (id + spawnId + cwd) and corrected each seat's record by message."
  suggested_encoding: "Export the minted pij id and spawnId into the spawned seat's environment (PIJ_SESSION_ID is already there; PIJ_SPAWN_ID too) and say so in the spawn banner, or have 'pij whoami' be the canonical canary answer the packet asks for — a seat that cannot state its own id makes the canary a formality."
  fp: 265bd20c492e
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:24:25.350Z"
- id: DL-005
  kind: difficulty
  description: "A plan-critical feature landed on main mid-flight (daemon HTTP authentication, #43) and no signal reached the working branch: the branch's CLI silently could not talk to the running daemon, and the first symptom was a coder's live smoke test getting FS3-E-DAEMON-UNAUTHORIZED after building the wrong thing against it. Three parallel seats were doing live-daemon work at the time."
  severity: degrading
  workaround: "Verified against main directly (git fetch + log), merged main into the plan branch immediately, fixed the sealed-spawn precondition in the goldens harness, and relayed the base move plus the key-writing requirement to all three coders."
  suggested_encoding: "A staleness check in the gate or in harness boot: warn when the current branch's merge-base with origin/main is behind by commits that touch crates/ — a plan branch that is behind on PRODUCT code should say so before a seat spends an hour proving something against a base that moved."
  fp: 608c31a56b33
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:53:40.390Z"
- id: DL-006
  kind: difficulty
  description: "harness checks failed for two coder seats with a production-write-shaped message (migration guard: version 13 -> blank) whose real cause was that the SHARED user config ~/.config/flowspace3/config.toml had gained an [agent] section for an in-flight PR, and this repo's config parsing rejects unknown fields, so the guard's after-probe could not parse it. Both seats correctly stopped work believing they might have written to production."
  severity: blocking
  workaround: "Reproduced the guard directly, found the section already commented out by another seat with a restore-when-PR-45-merges note, confirmed the guard now prints version=13, and released both coders to re-run."
  suggested_encoding: "(1) Make config parsing tolerate unknown sections the way the envelope tolerates unknown fields, so a newer daemon's config cannot kill an older binary's gate. (2) Make the guard distinguish 'the probe could not parse the config' from 'the production version changed' — one is an emergency and the other is not, and today they read identically."
  fp: ce51a8d12fc3
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T04:12:00.541Z"
- id: DL-007
  kind: difficulty
  description: "A ruling I escalated and received (prime approving a dependency for u-w) was recorded in the impl-guide but never relayed to the WORKER who was blocked on it, so the coder sat on a blocked seam for over an hour and had to ask again. The ruling existed; the relay did not."
  severity: degrading
  workaround: "Relayed the ruling with its rationale and the exact allowlist-row wording the moment the coder asked a second time."
  suggested_encoding: "Make relay part of the ruling's definition of done: an escalation is not closed when the PM has the answer, it is closed when the worker who is blocked has it. A packet-level rule ('every ruling you receive on behalf of a worker is relayed to that worker before you do anything else') would have caught this; so would a ruling log with a 'relayed-to' column."
  fp: 401598cb3f80
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T04:49:20.173Z"
- id: CONF-002
  kind: confusion
  description: "flowspace3 agents-start-here returned only mocha: 0 passed through lean-ctx instead of the documented agent orientation envelope"
  severity: degrading
  workaround: "used flowspace3 docs get agents and docs get search directly"
  suggested_encoding: "add a deterministic agents-start-here contract check to harness boot"
  fp: eefe9a492d97
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T05:14:32.161Z"
- id: DL-008
  kind: difficulty
  description: "flowspace3 semantic search from the composed 007 worktree searched other registered checkouts because this worktree is unregistered, so results omit the code under review"
  severity: degrading
  workaround: "used search for conceptual prior art, then read the composed worktree directly without mutating the shared daemon index"
  suggested_encoding: "have pij-team composition register its review worktree or teach search an explicit read-only checkout scope"
  fp: f00de668d4d6
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T05:15:23.039Z"
- id: CONF-003
  kind: confusion
  description: "harness checks raced the PM latest-main merge and reported production database changed version 13 to blank while details showed Rust conflict markers; the top-level verdict misdiagnosed a concurrent shared-tree conflict"
  severity: degrading
  workaround: "discarded the run as invalid and paused working-tree verification until PM supplies a clean pinned SHA"
  suggested_encoding: "checks should detect an unmerged/conflicted working tree before starting and report that directly, before the production-database sensor"
  fp: 932364ab7438
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T05:25:02.229Z"
- id: CONF-004
  kind: confusion
  description: "A cross-model reviewer found two HIGH defects that the full gate could not: a missing broken-pipe contract (the test proving it had never been on this branch, so its ABSENCE was invisible to a gate that only runs what is present) and standing PRD-59 messages silently swallowed by the human renderer (nothing asserted that a standing condition survives rendering)."
  severity: degrading
  workaround: "Fixed both and added the test that would have caught each; routed the four MEDIUM findings back to the units that owned them."
  suggested_encoding: "Two gate-level checks fall out of this: warn when a merge REMOVES test files that exist on the merge-base (both of this session's silent losses took a test with them), and treat 'no test asserts this promise' as a reviewable gap rather than a green."
  fp: faa9bac13c64
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T05:45:07.234Z"
