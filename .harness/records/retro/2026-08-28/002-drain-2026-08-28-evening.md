---
record_kind: "retro"
harness_version: "0.13.0"
branch: "main"
repo: "https://github.com/AI-Substrate/flowspace3.git"
created_at: "2026-08-28T05:56:27.954Z"
agent: "pij-instant-lynx"
plan_id: null
schema_version: "1.2"
retro_id: "2026-08-28T05:57:10Z-pij-instant-lynx-drain2"
started_at: "2026-08-28T01:03:05Z"
ended_at: "2026-08-28T05:57:10Z"
summary: "O-prime drain of 23 shared-buffer observations spanning the day's merge train (#41-#53): external dogfood verdicts, the disk-full incident, two platform defects escalated to pij (spawn-cwd writes, subagent seat minting), the attribution verification gap, search-diagnostic gaps (explain verb, empty_because), tidy/team tooling gaps, and the ask fake-provider defect now dispatched to flea. Dispositions: 2 fixed-now (007 shipped the event stream; config verb exists), 2 encoded in-flight (#50), 12 task-shaped with backlog rows, 4 plan-shaped (briefs/designs), 5 kept."
entries:
  - id: DL-041
    kind: difficulty
    description: "dogfood research verdict (subagent, 14 queries over harness-engineering): flowspace CARRIED a real research task 7/10 top-3 hit rate, BUT: (1) gitignored blind spot is the top product gap - the richest difficulty records live in scratch/ and docs point at them, search follows the pointer to a wall; wants opt-in index-untracked or N-referenced-paths-outside-index notice; (2) docs outrank implementing code for function-shaped queries (claude-adapter.ts rank 11/12) - wants --kind filter on CLI; (3) get on file addresses buries content in raw_text with empty text field + no ranged get; (4) short keyword near-misses need a low-confidence hint (best score 0.55 - weak match); long NL queries ranked BETTER than keywords here (contrast narwhal repo-local miss)"
    target: tooling
    severity: degrading
    suggested_encoding: "DRAIN: external dogfood verdict; product gaps live as backlog rows 21-26"
    fp: "0f8901380f25"
    disposition: kept
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:03:05.004Z"
  - id: DL-042
    kind: difficulty
    description: "flowspace3 search query lost argument grouping when passed through lean-ctx -c and failed with unexpected argument records"
    target: tooling
    severity: degrading
    workaround: "quote the entire semantic query as one shell argument"
    suggested_encoding: "make the CLI accept multi-token trailing query text or emit a quoting-specific next action | DRAIN: lean-ctx arg grouping; quote-whole-query workaround stands"
    fp: "a96883649264"
    disposition: kept
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:28:44.655Z"
  - id: DL-043
    kind: difficulty
    description: "Machine hit 100% full mid-fleet; LLVM IO failures surfaced as compiler errors, not as 'disk full'. No harness command can answer 'what is eating the disk' \u2014 required a 25-minute ad-hoc du/find survey by a dedicated seat. Root cause found: an unbounded turborepo cache (255GB, 224 of 244 entries >30 days old) plus ~110GB reclaimable inside the OrbStack VM and ~30GB of duplicated cargo target/ across 5 worktrees."
    target: tooling
    severity: blocking
    workaround: "Manual du -x -d3 over $HOME, find for build dirs, docker system df, tmutil listlocalsnapshots; report written to scratch/disk-usage-report-2026-08-28.md"
    suggested_encoding: "1) harness boot free-space gate: fail/warn below ~50GB or 5% on the worktree volume BEFORE building. 2) harness doctor disk: print top space classes (turbo/target/node_modules/docker volumes/VM images) deterministically. 3) worktree teardown cleans target/ and orphaned docker volumes. 4) age-cap agent log/session stores (a single 2.95GB .copilot log exists). | DRAIN: w-disk-usage brief written; boot free-space gate + doctor disk"
    fp: "e7379692a8a2"
    disposition: plan
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:44:43.357Z"
  - id: DL-044
    kind: difficulty
    description: "PLATFORM DEFECT (silkworm DL-007/008, controlled-comparison proven): pij-spawned seat file-edit tools resolve relative paths vs SPAWN cwd while shell resolves vs worktree - silent cross-tree writes into the PM shared branch, success echoes indistinguishable, false green builds. Real fix belongs in pij/harness spawn (bind tool cwd to dispatched worktree or refuse relatives on disagreement); skill mitigated with absolute-paths-always + prove-in-tree done-bars"
    target: tooling
    severity: blocking
    suggested_encoding: "DRAIN: platform defect reported upstream to pij (spawn cwd binding)"
    fp: "0dfb2bd3b405"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:45:16.905Z"
  - id: CONF-001
    kind: confusion
    description: "stand-down instruction used adopted-idle but pij report state accepts only blocked question hold waiting ready failed cancelled done; activity idle is derived rather than writable"
    target: tooling
    severity: annoying
    workaround: "leave the adopted seat open and cease activity so liveness becomes idle"
    suggested_encoding: "document adopted-idle as a lifecycle/activity outcome and name the exact no-command stand-down procedure | DRAIN: document adopted-idle as the no-command stand-down"
    fp: "0f2c66c093ec"
    disposition: kept
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:48:53.243Z"
  - id: CONF-002
    kind: confusion
    description: "status envelope: a settled queue emits only state=done rows \u2014 pending/running rows are absent rather than zero, so any consumer (tui-poc) must know the kind\u00d7state matrix and default missing cells to 0"
    target: tooling
    workaround: "hardcoded the 3 kinds and defaulted missing states to zero"
    suggested_encoding: "status emits the full kind\u00d7state matrix with explicit zeros (or documents the sparse contract in the envelope) | DRAIN: sparse kind-state matrix contract; document or emit zeros"
    fp: "0db9eefc7c61"
    disposition: kept
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:51:56.618Z"
  - id: DL-045
    kind: difficulty
    description: "no daemon activity/event stream: tui-poc had to mock its activity feed from queue-count deltas \u2014 same gap will bite any web UI; a 'status --watch' NDJSON tail would serve both"
    target: tooling
    workaround: "derived feed from 2s polling deltas, tagged mock"
    suggested_encoding: "add a status --watch / event-tail verb emitting NDJSON operation events | DRAIN: shipped in #52: daemon event stream + status --watch (plan 007)"
    fp: "7d2a7041ac6b"
    disposition: fixed-now
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-08-28T01:51:56.739Z"
  - id: DL-046
    kind: difficulty
    description: "docker/scripts/run.sh mounts the legacy shared fs3-rustup volume while build.sh mounts per-arch fs3-rustup-arm64/-x64 \u2014 retro 008's per-arch fix landed in build.sh only. Nothing proves the two scripts agree on cache-volume names, so the drift is invisible and costs a third rustup home (~3.2 GB). harness docker lint proves engine-agnosticism, compose validity and no docker-exclusive features, but says nothing about cache-volume coherence."
    target: project-sensor
    severity: degrading
    workaround: "found by hand-diffing the two scripts during an unrelated disk survey"
    suggested_encoding: "extend docker/scripts/lint.sh with a cache-volume coherence check: parse the volume names every script mounts and fail when two scripts mount different volumes for the same role (rustup/registry/target) | DRAIN: docker lint cache-volume coherence check"
    fp: "1ef14d2a8ed9"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:56:10.767Z"
  - id: DL-047
    kind: difficulty
    description: "fs3-cargo-target is documented as the cross-build cache (4 release triples = 3.1 GB) but harness docker run mounts the same volume for cargo test --workspace, so 14.7 GB of host-arch debug artefacts accumulate in it \u2014 11.8 GB in debug/deps across 16,042 files, with unstripped ~250 MB test binaries and stale hash generations cargo never garbage-collects. Volume is 21 GB where its stated job needs 3. Nothing measures or bounds a cache volume's size or composition; the only way to see it was docker run --rm -v fs3-cargo-target:/t alpine du."
    target: project-sensor
    severity: degrading
    workaround: "manually inspected the volume with a throwaway alpine container"
    suggested_encoding: "harness docker disk: report per-volume size + composition with a budget that goes degraded past a threshold; split the test target dir into its own fs3-test-target volume so cross-build cache and test debris are separately prunable | DRAIN: harness docker disk + split test-target volume"
    fp: "8f92e7d6a169"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:56:10.903Z"
  - id: DL-048
    kind: difficulty
    description: "ATTRIBUTION VERIFICATION GAP, two seats independently + mechanism narrowed (silkworm DL-corroboration of u2 DL-003, demonstrating DL-011): harness commit direct-verified only proves a refs/notes/ai note EXISTS - VERIFIED commits minutes apart from one seat produced BOTH correct agent attribution (159cf61) and humans-only misattribution (c9a05d4, bf16677) where git-ai saw the exact hunks but no agent session claimed them so they fell to the human author. Editing-tool hypothesis eliminated (both tools present in both outcomes). Encoding: harness commit must verify note CONTENT (sessions block exists + changed ranges claimed by an agent session) and report ATTRIBUTION FAILED on humans-only; today wrong-note reports as SUCCESS. Seats cannot trust a green commit verdict for attribution"
    target: tooling
    severity: blocking
    suggested_encoding: "DRAIN: meadowlark attribution design (note CONTENT verification) awaiting Jordan authorization"
    fp: "0c8a42bc6f10"
    disposition: plan
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T01:59:16.835Z"
  - id: CONF-003
    kind: confusion
    description: "search relevance: Jordan query \"llm\" returned ZERO results from a fully-indexed repo whose provider/summarizer code plainly relates - bare-acronym short queries embed poorly and the envelope cannot distinguish weak-match-suppressed from nothing-exists; joins narwhal long-NL miss + subagent low-confidence-hint ask as the search-UX family"
    target: tooling
    severity: degrading
    suggested_encoding: "DRAIN: w-search-degenerate brief + 006 u-c weak-match floor in #50"
    fp: "68e2c3f6e832"
    disposition: plan
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T02:05:29.145Z"
  - id: DL-049
    kind: difficulty
    description: "harness team tidy refuses squash-merged branches with E_BRANCH_NOT_MERGED. Found by dogfooding tidy on its own worktree: PR #40 WAS merged, but this repo squash-merges, which rewrites history so the branch tip is never an ancestor of main and 'git branch --merged main' never lists it. Every correctly-merged packet branch in this repo therefore hits this refusal and must use --force, which also silences the dirty-tree and unpushed checks \u2014 so the safety rail trains people to bypass all three."
    target: project-sensor
    severity: degrading
    workaround: "harness team tidy <slug> --force"
    suggested_encoding: "add a squash-merge leg to isMerged(): 'git cherry main <branch>' marks patch-equivalent commits with '-', so an all-'-' result means the content is upstream even when the tip is not an ancestor. Failing that, at minimum name squash-merge in the E_BRANCH_NOT_MERGED next_action so the operator knows --force is the expected path, not a risk they are taking. | DRAIN: tidy squash-merge leg; superseded detail in DL-054"
    fp: "b02395f2ec1f"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T02:20:06.666Z"
  - id: DL-050
    kind: difficulty
    description: "There is no way to ask flowspace3 what a search actually DID. To root-cause a zero-result query I had to: call the Azure embeddings deployment by hand with an az token to dump the query vector, extract the search_elements SQL text out of the Rust source with a python script, PREPARE it in psql inside the docker container with hand-bound parameters, and EXPLAIN ANALYZE it. That is four tools and an afternoon to answer 'why was this empty'. A product whose core promise is semantic retrieval has no retrieval diagnostic."
    target: tooling
    severity: blocking
    workaround: "python to extract the SQL literal from embeddings.rs, PREPARE/EXECUTE via docker exec psql, plus a hand-rolled curl to the embeddings endpoint"
    suggested_encoding: "flowspace3 explain \"<query>\" --repo X: print the query vector norm, the model_key, the resolved filters, the chosen plan, candidates examined vs returned, and whether the scan hit its budget. Everything I reconstructed by hand is already known inside one function. | DRAIN: flowspace3 explain verb \u2014 backlog row 44 (added this drain)"
    fp: "67ab72ec0636"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T02:44:21.348Z"
  - id: DL-051
    kind: difficulty
    description: "Dogfooding flowspace3 to find its own similarity SQL, the one genuine code-question search of the session MISSED: 'similarity search min score floor filter over embeddings' returned testkit/contract.rs::assert_same_embedding and a SKILL.md heading, but NOT crates/store/src/embeddings.rs::search_elements, which is the only real answer. Fell back to grep. The miss is the w-search-degenerate bug itself (scoped search starved by a multi-repo HNSW index) - but the point that survives the fix is that NOTHING in the envelope told me the answer was degraded. I only learned it was wrong because I already knew what I was looking for."
    target: tooling
    severity: degrading
    workaround: "grep for search_elements, then read the file directly"
    suggested_encoding: "An empty-or-short scoped result must carry a reason in the envelope; shipped in this packet as meta.empty_because (below_floor | scan_incomplete). | DRAIN: meta.empty_because shipped in 006 u-c, lands with #50"
    fp: "7456c50a5477"
    disposition: encoded
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-08-28T02:44:21.371Z"
  - id: CONF-004
    kind: confusion
    description: "flowspace3 has no command that names its own active config file. doctor reports 'embedder=azure-embed (azure_openai)' but not WHERE that came from; I guessed ~/.config/flowspace3/config.toml. There is also no 'settings' verb (the CLI suggests 'ping' and 'status'). For a tool whose most dangerous failure mode is 'the index was written by a different embedder than the one searching', the config path and its resolution order should be one command away."
    target: tooling
    severity: annoying
    workaround: "guessed ~/.config/flowspace3/config.toml and read it directly"
    suggested_encoding: "doctor's providers row should carry the config file path it resolved from, or add 'flowspace3 config' that prints the resolved file and the merged settings. | DRAIN: flowspace3 config verb exists on main; doctor row remains open"
    fp: "8292ec64ccb8"
    disposition: fixed-now
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-08-28T02:44:39.914Z"
  - id: DL-052
    kind: difficulty
    description: "agentic-query POC: flowspace3 get on an address shared by struct+impl (WatcherSupervisor) errors and demands --span; the LLM agent lost a turn recovering. Fine for humans, costly for agent loops."
    target: tooling
    severity: degrading
    workaround: "returned the error text to the model as a tool result; it picked a child address instead"
    suggested_encoding: "get without --span on a multi-element address could return the first element or a combined outline instead of erroring | DRAIN: get multi-element default \u2014 backlog row 45 (added this drain)"
    fp: "ecb8d52b3dc8"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T02:53:26.093Z"
  - id: DL-053
    kind: difficulty
    description: "agentic-query POC: search JSON envelope is verbose for LLM consumption (spans, tags, repo echoed per hit); POC had to compact hits to address/path/score/gist to fit a cheap model's context"
    target: tooling
    severity: annoying
    workaround: "client-side compaction in the POC"
    suggested_encoding: "an agent-facing --compact search output (address, path, score, gist only) | DRAIN: --compact agent output \u2014 backlog row 45"
    fp: "771f6e1c3302"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T02:53:30.183Z"
  - id: DL-054
    kind: difficulty
    description: "team tidy squash-detection blind spot: git cherry patch-id equivalence fails when main moved between branch base and squash-merge (context drift) \u2014 PR #48 provably merged (gh state MERGED) yet tidy refuses E_BRANCH_NOT_MERGED. Fail-closed is correct; encoding wanted: accept an explicit --merged-evidence path or check PR merge state / merge-base content equality as a second signal"
    target: tooling
    severity: degrading
    suggested_encoding: "DRAIN: tidy --merged-evidence / PR-state second signal; row 33 family"
    fp: "6f19f215bc7f"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T04:27:32.908Z"
  - id: DL-055
    kind: difficulty
    description: "team new only fits PLAN worktrees (mints ordinal + plan scaffold); small w-* packet worktrees get hand-rolled git worktree add + mkdir docs/briefs + cp brief \u2014 done 4x today by o-prime. Missing: harness team new --packet <slug> (worktree + branch + vendored brief, no ordinal, no plan folder) so tidy/new are symmetric for packets too"
    target: tooling
    severity: degrading
    workaround: "raw git worktree add + manual brief vendoring"
    suggested_encoding: "team new --packet mode | DRAIN: team new --packet \u2014 backlog row 27"
    fp: "0e3b9435f371"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T04:39:24.635Z"
  - id: DL-056
    kind: difficulty
    description: "attribution forensics: omp eval-tool file creation is invisible to write/edit-based surveys AND git-ai \u2014 plan 001's entire 7-crate scaffold was written by pij-alternative-turtle (session 01a03bd3) via eval python FILES-dict batches; git-ai note on b812d4d credits lynx+bitter-swan instead. Also: omp edit tool hides the file path inside a patch-DSL string (arguments.input '[path#hash]'), and toolCall args are sometimes nested under arguments.input/.i"
    target: tooling
    severity: degrading
    workaround: "parsed eval code bodies + patch-DSL paths by regex; cross-checked flow-pair run ledger and PM spawn records"
    suggested_encoding: "telemetry adapter: unwrap nested toolCall args, extract paths from edit patch-DSL, and classify eval/bash file-writing patterns as writes; flag eval-heavy sessions attribution-opaque | DRAIN: meadowlark telemetry adapter design (eval-writes classification)"
    fp: "2c1768d31ecc"
    disposition: plan
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T05:00:21.861Z"
  - id: DL-057
    kind: difficulty
    description: "bin/daemon-restart refuses on multiple candidates with no way to select one \u2014 but multiple daemons IS the ruled steady state (prod on 7373 + seats testing on alt ports). Needs --pane/--port/--pid selector; today it can never restart prod while anyone is testing."
    target: tooling
    severity: degrading
    workaround: "manual C-c + relaunch in pane %50 via tmux send-keys"
    suggested_encoding: "daemon-restart --port <n> (match by listening port) defaulting to 7373 | DRAIN: daemon-restart --port selector \u2014 backlog row 37"
    fp: "ed10642bc9ef"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T05:24:35.684Z"
  - id: DL-058
    kind: difficulty
    description: "omp subagents inherit the parent's PIJ spawn env and self-register as full pij seats: one 'pij spawn --bin omp' of our 008 PM produced FIVE registered seats (parent + 4 scout subagents), all sharing spawnId s1787894747013-17144, each firing a ready-ping at o-prime. Registry pollution + misdirected ready-pings; a supervisor could canary or message a subagent believing it a real seat."
    target: tooling
    severity: degrading
    workaround: "identified by matching spawnId + pane agent count; ignoring phantom seats, will tidy registrations after scouts exit"
    suggested_encoding: "pij extension should strip/namespace PIJ_SPAWN_* env before launching subagent processes, or dedupe registration on (spawnId,pid) | DRAIN: reported upstream: pij#19 recurrence, ermine has repro"
    fp: "d4605d447c09"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T05:28:22.675Z"
  - id: DL-059
    kind: difficulty
    description: "PROD daemon ask answered via model fake@1 ('offline fake has no scripted answer', grounded:false) \u2014 ambient config has no [agent] section (deliberately reverted pre-#45), and ask silently DEFAULTS TO THE FAKE agent provider instead of refusing. Envelope honesty machinery worked (grounded:false + TREAT WITH SUSPICION next_action) but a prod daemon faking answers instead of saying 'no agent provider configured' is a misleading-zero-family defect."
    target: tooling
    severity: degrading
    workaround: "answering the question by code reading; [agent] ambient restore pending the merge train per standing ruling"
    suggested_encoding: "ask with no configured agent provider returns a config error naming the missing section, never the fake; fake providers are sandbox-only | DRAIN: dispatched to flea (backlog row 42) with vicuna's envelope sharpening"
    fp: "6a417417d798"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-08-28T05:38:45.295Z"
---

# Retro — 2026-08-28 evening drain (o-prime, post-007-close)

Snapshot preserved at scratch/retro-snapshot-2026-08-28T0556.json (17,968 bytes,
verbatim envelope). Every entry's DRAIN note names where its encoding lives.
Notable: DL-045 and CONF-004 were fixed by the day's own merges before the drain
reached them — the flywheel outrunning the buffer for the first time.
