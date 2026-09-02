---
record_kind: "retro"
harness_version: "0.14.0"
branch: "main"
repo: "https://github.com/AI-Substrate/flowspace3.git"
created_at: "2026-09-02T19:41:38.377Z"
agent: "pij-binding-magpie (o-prime)"
plan_id: "016-hidden-dirs, 017-daemon-key-after-bind"
schema_version: "1.2"
retro_id: "2026-09-02T19:41:38Z-magpie-016017"
started_at: "2026-09-02T07:20:00Z"
ended_at: "2026-09-02T19:45:00Z"
summary: >
  Two packets shipped in one session — 016 (per-root hidden-directory indexing, PR #107,
  merged 689ac27, proven in prod) and 017 (publish the daemon auth key only after a real
  bind, PR #108, merged 2d7f45f, foreign-cwd refusal proven in prod with the key file
  byte-identical before and after). Both reviews found real defects that would never have
  been visible in the shipped product: two lying agent-facing envelopes in 016, and a
  correct guard with no ratchet in 017. The session's dominant cost was NOT coding — it was
  communications: a pij composer veto misfiring on working seats delayed rulings by minutes
  to hours, two seats idled while blocked, and o-prime's own paraphrase silently inverted an
  acceptance criterion. The day also closed the long-running "flaky" health test, which was
  three real causes and no flakiness. Buffers from four seats (dormouse 14, knobbler 2,
  cod 2, hyena 1) were rescued to fs3-governance/scratch before their worktrees were removed;
  this record drains o-prime's own three.
entries:
  - id: DL-001
    kind: difficulty
    description: "A blocked seat idles silently. The 017 coder hit a provider 422 content filter mid-task and simply stopped — no message, no escalation; the 016 reviewer wrote 'Holding for the o-prime ruling' into a status FILE and idled. Both were found only by o-prime polling panes and file mtimes, ~18 minutes lost across two seats simultaneously."
    target: tooling
    severity: degrading
    workaround: "o-prime ran a watcher loop over tmux capture-pane and .harness/temp/agent mtimes, then pasted or sent the unblock."
    suggested_encoding: "Packet instruction: a seat that becomes blocked MUST message its prime FIRST, before writing any status file. And pij should surface an 'awaiting-ruling' anomaly (seat idle > N min whose newest *-status.md postdates its last outbound send) so the prime is told rather than polling."
    fp: "b3b5a68fb381"
    disposition: task
    system:
      compound:
        status: open
        backlog_row: 172
  - id: CONF-001
    kind: confusion
    description: "Re-wording a task to route around a provider content filter silently re-cut its acceptance criterion. o-prime paraphrased plan-017 t4 from memory and inverted ac-0004: the plan requires a foreign-cwd daemon to REFUSE the prod URL with FS3-E-PROD-NOT-DESIGNATED; the paraphrase said 'absent designation behaves exactly as today' and invented a private-token mechanism. The coder stop-and-asked before writing a line of t4."
    target: plan
    severity: degrading
    workaround: "Ruled the READY ddocs authoritative, voided the paraphrase's t4 paragraph, and issued a standing rule: where a prime message and the plan disagree, the ddocs win; only an explicit 'ddoc mutation: <field> := <value>' changes a contract."
    suggested_encoding: "A prime's unblock message must QUOTE the AC verbatim out of the ddoc, never restate it — add a `ddocs get <ac-id>` step to the prime-reply ritual so a re-wording physically cannot carry a contract change."
    fp: "2ece7577a236"
    disposition: task
    system:
      compound:
        status: open
        backlog_row: 173
  - id: DL-002
    kind: difficulty
    description: "A pij-rs send that returns queued (reason human-typing) with 'ok' is NOT delivered. o-prime told a coder NOT to run the full gate; the message landed 8 minutes later on attempt 101, after the seat had acted on stale orders and moved a PR head under an active delta review. Diagnosed by the pij prime as the composer veto misfiring on a WORKING omp — agent output read as a human draft."
    target: tooling
    severity: degrading
    workaround: "Query delivered_at in ~/.pij-rs/pij.sqlite after every send that returns queued; never let a queued ruling be the only thing between a seat and an irreversible action."
    suggested_encoding: "pij req-0054 / plan 134: still-queued and delivered-late notices to the SENDER, and no composer veto for a seat whose 'typing' is its own agent output. u1 and u3 shipped the same night; both arms measured from this seat (empty composer -> extension-stream, 0 s; real draft -> human-typing + true draft_sha, held until cleared)."
    fp: "db05fa987620"
    disposition: fixed-now
    system:
      compound:
        status: closed
        backlog_row: 180
  - id: WIN-001
    kind: win
    description: "Requiring the reusable half of a fix paid for itself in three hours. o-prime made merging #108 conditional on the health test's panic carrying the child's stdout+stderr, because 'exit status: 1' had thrown away the daemon's own diagnosis. That same evening the identical test failed again for a NEW reason and named it in one run: 'this flowspace3 binary is OLDER than its database ... migrating cannot fix this'. Before the change, that would have been another multi-seat mystery."
    target: project
    severity: annoying
    workaround: "n/a"
    suggested_encoding: "When ruling a fix, separate the specific repair from the reusable half (the error that explains itself) and require both — the reusable half is what pays."
    fp: "0000win001fs3"
    disposition: kept
    system:
      compound:
        status: closed
        backlog_row: 187
  - id: INS-001
    kind: insight
    description: "Agreement between independent reproductions confirms a MEASUREMENT and does nothing to catch a shared INTERPRETATION error. Three seats in three repos ran three scripts against the pij registry and got identical counts (839/93/744/716/28); all three then carried the same wrong claim ('nothing to branch on') until a fourth seat checked field VALUES where the rest had checked the field SET. A second correction followed when someone asked a POPULATION question nobody had asked before scoping."
    target: project
    severity: annoying
    workaround: "Corrected the record twice, re-measuring each claim locally before restating it."
    suggested_encoding: "When a finding will inform a decision, state separately what was MEASURED and what is INFERRED, and have the reviewer attack the inference specifically. Same shape as reviewer instruction i12 (mutate the guard) and the 013 lesson that a shape fixture is not a cost fixture: the number was fine, the claim on top of it was not."
    fp: "0000ins001fs3"
    disposition: kept
    system:
      compound:
        status: open
        backlog_row: 189
