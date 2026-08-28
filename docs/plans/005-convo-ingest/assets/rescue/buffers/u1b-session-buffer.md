- id: DL-001
  kind: difficulty
  description: "pij send has no --file/stdin option, so every multi-paragraph message (ack, numbered plan, done report) must be shell-quoted. A here-doc inside command substitution inside double quotes silently mangled a 10KB numbered plan into fragments plus stray 'command not found' lines, and pij DELIVERED the mangled text with a normal success receipt — the PM would have read garbage as my plan."
  severity: degrading
  workaround: "Wrote the message to a file and used pij send \"$(cat file)\", which is safe because command substitution output is not re-parsed."
  suggested_encoding: "Give pij send a --file <path> flag (or read stdin when no message argument is given). The protocol mandates long structured messages; routing them through shell quoting is a footgun the tool can remove outright."
  fp: 5b86898fe9ad
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:32:54.468Z"
- id: DL-002
  kind: difficulty
  description: "File-edit tooling resolves RELATIVE paths against the session cwd (the main clone) while shell commands resolve against the coder worktree. Same relative path, two different files, no warning. An edit rejection surfaced a 'pub mod claude;' line I proved absent from my worktree and from u1a's; it exists in the MAIN CLONE. One accepted edit would have written into the shared tree on the plan branch, which every coder packet forbids."
  severity: blocking
  workaround: "Verified with md5sum across both trees, then switched to absolute paths for every file tool call."
  suggested_encoding: "In a worktree-per-coder fleet, either (a) make the session cwd the coder's worktree at spawn time, or (b) have the harness refuse a relative-path write when cwd is not inside the seat's assigned worktree. Cheapest partial fix: state 'use absolute paths' in the restart note alongside the PIJ_SESSION_ID warning, since both are the same class of worktree-invisibility bug."
  fp: 0e0e4180f793
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:40:52.254Z"
- id: DL-003
  kind: difficulty
  description: "A test suite that creates shared filesystem state passed 'harness checks' green while carrying a ~10% race: two scratch dirs named from SystemTime nanos alone collided on parallel test threads, and one fixture's Drop deleted the other's live file. The failing test changed between runs. A single green run cannot detect it, so the gate certified it and it reached the composed tree."
  severity: degrading
  workaround: "Controlled A/B: 20 pre-fix runs gave 2 failures, 60 post-fix runs gave 0. Fixed by adding a process-static AtomicUsize counter plus pid to the scratch root name."
  suggested_encoding: "Give the harness a repeat mode for flake detection, e.g. 'harness checks --repeat N' or a 'flake' gate that reruns suites touching temp dirs. Cheaper partial fix: a testkit helper 'fs3_testkit::scratch_root(label)' that is collision-proof by construction, so no seat has to re-derive the counter+pid recipe and half-copy it like I did."
  fp: 0fa5636388c1
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:15:06.909Z"
