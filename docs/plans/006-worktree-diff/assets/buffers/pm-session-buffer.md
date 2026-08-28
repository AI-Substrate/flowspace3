- id: DL-001
  kind: difficulty
  description: "flowspace3 status reports one GLOBAL queue with no per-root breakdown, so any script that waits for 'indexing finished' is hostage to every other seat on the shared daemon: my probe harness sat 15+ minutes behind another seat's 7.8k-job root add, and its global row deltas were contaminated by that seat's rows"
  severity: degrading
  workaround: "rewrote the wait to poll only the probe worktree's own scan jobs (jobs.dedupe_key like 'scan:<worktree_id>:%') and attributed enrichment by content reachability instead of global counts"
  suggested_encoding: "give status a per-root queue breakdown (jobs pending/running grouped by worktree) so 'is MY root indexed yet' is answerable without SQL"
  fp: faf0cee1d25c
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:22:06.521Z"
- id: CONF-001
  kind: confusion
  description: "A doc comment above a Rust function is not searchable: elements carry the item's own span, so text in /// lines above a fn never enters raw_text and no vector covers it. I wrote a probe marker into a doc comment, searched its exact distinctive phrase, got nothing, and briefly believed worktree content was unindexed - the wrong conclusion from a correct search"
  severity: degrading
  workaround: "moved the marker phrase into the function BODY, where it is indexed and scored 0.77"
  suggested_encoding: "either attach leading doc comments to the element they document (they are the highest-signal text in the file) or say so in the search envelope's steer when a query looks like prose that only exists in comments"
  fp: 0460c1927377
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:46:55.222Z"
- id: DL-002
  kind: difficulty
  description: "Search scores carry float jitter across identical calls (0.7730240968 vs 0.7730564295 for the same query seconds apart, same hits and order) because the query is embedded afresh per call. Any test or probe that diffs whole search envelopes reports a false difference - my first version of the P3 comparison did exactly that and claimed version-resolution existed when it does not"
  severity: annoying
  workaround: "compare result IDENTITIES only (address/path/name/kind/span/snippet), never scores or meta"
  suggested_encoding: "a documented compare-answers helper (or a --deterministic/--explain-scores note in the envelope) so every future contract test uses the same non-flaky predicate"
  fp: 0af94568dc4e
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T02:46:55.227Z"
- id: DL-003
  kind: difficulty
  description: "PM sequencing error: I created the coder worktrees from the plan branch BEFORE committing their packets, so both seats booted into trees where their own dispatch file did not exist — one seat correctly refused to guess and reported ENOENT, costing a round trip each"
  severity: annoying
  workaround: "told each seat to git merge --ff-only the plan branch; clean fast-forward since neither had work yet"
  suggested_encoding: "harness team should mint coder worktrees at the CURRENT plan-branch head at dispatch time, or the dispatch verb should refuse when the packet path is absent in the target worktree — the packet is the interface to the worker, so its absence should be a refusal, not a discovery the worker makes"
  fp: 2fdc0eb4907d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:02:59.459Z"
- id: CONF-002
  kind: confusion
  description: "A fleet-safety ruling I sent crossed with a coder's in-flight calibration report, and the coder correctly-but-wrongly withdrew good evidence: my rule said 'do not run against the live database', it meant 'do not WRITE to it', and read-only search was never at risk. Cost was one round trip and nearly an hour of rebuilding a synthetic corpus to re-derive an already-approved number"
  severity: annoying
  workaround: "immediate stop-and-reinstate message drawing the write/read line explicitly, plus the reason the live corpus was BETTER evidence than a synthetic one"
  suggested_encoding: "safety rules dispatched to seats should name the OPERATION CLASS they forbid (writes/migrations/daemon boots), never the resource, because a resource-shaped rule reads as a total ban and the conservative agent over-complies by discarding valid work"
  fp: 5bb45a3d6fbb
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:11:17.990Z"
- id: DL-004
  kind: difficulty
  description: "The shared 'throwaway' database flowspace3_test is not throwaway: it accumulates registered roots from every seat (15 roots, every worktree on the machine) and a 6,520-job backlog. Any daemon booted against it drains that backlog, and because ambient ~/.config/flowspace3/config.toml selects Azure providers, the drain is REAL PAID CALLS — measured 150 summaries and 2,475 vectors bought in a 15-minute window across two seats following my own isolation recipe"
  severity: blocking
  workaround: "unique database per seat plus FS3_CONFIG_DIR pointed at an empty directory so the built-in fake providers apply; verify via the daemon boot line printing embedder=fake summarizer=fake"
  suggested_encoding: "a first-class isolation verb (harness fs3 sandbox / flowspace3 daemon --sandbox) that mints a unique database, forces fake providers, picks a free port and drops the database on exit — the four-override recipe is tribal knowledge with a bill attached, and two independent seats got it wrong the same way within minutes"
  fp: e55448ac39c7
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:17:23.950Z"
- id: DL-005
  kind: difficulty
  description: "harness plan pr-body refuses to render while any AC is open (E457), which encodes the assumption that every acceptance criterion closes BEFORE the PR. Plan 006 breaks that assumption for a defensible reason: its last AC is a claim about live retrieval, and measuring it means running the feature against the production index — which IS the go-live event, ruled to happen at merge, not before it. So the plan whose evidence discipline is strongest is the one the evidence tool refuses to serve"
  severity: annoying
  workaround: "wrote the PR body by hand, naming ac-0003 as closing post-merge with the receipt that will close it"
  suggested_encoding: "let pr-body render open ACs as an explicit PENDING section with their notes (ac-0003 already carries its reason in its note) rather than refusing — a PR that names what is not yet proven is more honest than one that cannot be written"
  fp: 55ba7b413eb4
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T04:11:30.614Z"
