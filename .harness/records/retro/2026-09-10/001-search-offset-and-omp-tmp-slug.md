---
record_kind: "retro"
harness_version: "0.14.0"
branch: "main"
repo: "https://github.com/AI-Substrate/flowspace3.git"
created_at: "2026-09-10T23:26:55.019Z"
agent: "claude-code/o-prime pij-instant-lynx"
plan_id: null
schema_version: "1.2"
retro_id: "2026-09-10T23:26:55Z-o-prime-122c"
started_at: "2026-09-04T22:00:00Z"
ended_at: "2026-09-10T23:27:29Z"
summary: "Plan 018 poller shipped and bounced (#118); its prod receipt (#119) exposed backlog row 204, fixed in #122 after a three-pass cross-model review (F1 deleted-cwd slug, F3 lost ratchet, F4 unpinned multi-component tail; F2 known-open). Search paging --offset shipped (#121). Prod bounced onto 852bce3: conversations row stalled -> flowing, backlog 37 -> 5, omp unreadable 0; 19/20 row-204 sessions re-ingested by hand (row 206). Unisphere first dogfood cross-checked 50/50 initiating requests against fs3 human turns. 18 observations drained here; encodings listed per entry."
entries:
  - id: DL-001
    kind: difficulty
    description: "Asked 'is the daemon ingesting my seat's work?' and status/doctor could not answer it. Both report healthy with an empty queue, which is the SAME shape whether the watcher is ingesting fine or has been dead for 17 hours. I had to grep a 3.5MB log for the last non-retention line to establish liveness, then write a probe file into a registered root and wait out the debounce to prove the pipeline end-to-end."
    target: tooling
    severity: degrading
    workaround: "tail the log, filter out hourly retention lines to find the last real scan; then write a unique file into a registered root and watch for 're-listed a changed directory' + a search hit"
    suggested_encoding: "status should carry per-root last_ingested_at (and a global last_scan_at), so 'quiet because nothing changed' is distinguishable from 'quiet because the watcher is dead' without reading logs. A doctor row 'watcher: last ingest 17h ago across 60 roots' would have answered this in one command."
    fp: "b4dc0f5fecf6"
    disposition: encoded
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "plan 018 health row (#118)"
  - id: DL-002
    kind: difficulty
    description: "Conversation ingest is PULL-ONLY and nothing schedules it, so the live agent transcripts silently stop being indexed. Files are watched automatically; conversations are not \u2014 the daemon has no conversation poller (grep of crates/daemon/src finds conversations only in ask.rs, the query side). Measured on prod: unicorn's live session cbe08128 sat at 22 turns / last_turn_at 2026-09-01T23:50 while its jsonl grew to 984 lines over three days. One manual 'conversation ingest' took it to 316 turns in ~30s. Fleet-wide: 106 claude session files written in the last 7 days, 109 conversations indexed in TOTAL, newest ingested 2026-09-02. The doctrine in crates/cli/docs/conversations.md is 'total recall, selective retrieval' \u2014 store every turn \u2014 but in practice nothing stores them unless a human remembers to type the verb."
    target: tooling
    severity: blocking
    workaround: "flowspace3 conversation ingest --harness claude --session <id> by hand, per session; incremental via the durable byte-offset cursor so it is cheap to repeat"
    suggested_encoding: "The daemon should poll the known session stores the way the file watcher polls roots \u2014 the tail reader already has rotation/truncation handling and a durable cursor, so the incremental machinery exists and only the scheduler is missing. Short of that, 'conversation ingest --all' plus a doctor row 'conversations: newest ingest 3d old, 106 live sessions unindexed' would make the rot visible instead of silent."
    fp: "e52eaaae405a"
    disposition: encoded
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "plan 018 poller (#118)"
  - id: DL-003
    kind: difficulty
    description: "index_config_formats is UNREACHABLE from outside the code \u2014 there is no config key and no CLI flag, so a repo whose content IS yaml/json cannot be indexed at all. DiscoverySettings has the field (discovery.rs:226) and verdict() honours it (:733), but From<&ScanConfig> (:271) fills every discovery-only knob from ..Self::default(), and ScanConfig is deny_unknown_fields with only max_file_bytes/min_file_bytes/respect_gitignore/include_hidden/follow_symlinks/standard_ignores. Proved: adding [scan] index_config_formats = true to a config is REFUSED with 'unknown field'. flowspace3 add has no flag either. Only tests ever set it. Measured cost on a real repo: /Users/jordanknight/github/novels skipped 105 files as config-format \u2014 102 .yaml + 3 .json \u2014 and those yaml files ARE the repo's content (character profiles, bestiary, factions, locations, species, plus their templates). The 307 .md chapters indexed; the structured canon behind them silently did not. The doc comment at discovery.rs:210 claims the daemon resolves this 'machine defaults < worktree override < flag, PRD req 40', which is aspirational for this knob and reads as though the lever exists."
    target: tooling
    severity: blocking
    workaround: "none available to a user \u2014 it requires a code change"
    suggested_encoding: "Add index_config_formats to ScanConfig and map it in From<&ScanConfig>, plus a per-root override so one prose/worldbuilding repo can opt in without turning package-lock.json into index noise everywhere else. A --index-config-formats flag on 'add' would match how include_hidden is already surfaced per-root. Also: the skip ledger gives a count and a reason but no sample paths, so 'config-format 105' cannot be recognised as 'your novel's canon' without hand-auditing the tree."
    fp: "23d4588e355b"
    disposition: deferred
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog DL-003 packet, parked on Jordan"
  - id: DL-004
    kind: difficulty
    description: "fs3's git-ai reader points at a database that does not exist. convo_ingest.rs:538 hardcodes home/.git-ai/metrics.sqlite3; git-ai on this machine writes home/.git-ai/internal/metrics-db (4.9GB, 1.13M rows, live \u2014 event_kind=5 rows landing today). Proved on prod: 'conversation ingest --harness metrics-db --session a5a5588f\u2026' was ACCEPTED by the route, then the job failed with FS3-E-QUERY-INVALID 'cannot open /Users/jordanknight/.git-ai/metrics.sqlite3 read-only: unable to open database file'. Consequence: the metrics-db route has never worked on this machine, and per conversation_join.rs it is the ONLY store copilot sessions exist in \u2014 so github-copilot conversation ingest is dead outright, and the git-ai route for claude/omp is unusable as a fallback. The accept-then-fail shape also means the CLI says 'queued' and the operator learns of the failure only from status.last_error or the daemon log."
    target: tooling
    severity: blocking
    workaround: "none for copilot; claude and omp still ingest from their native stores"
    suggested_encoding: "resolve the path from git-ai itself (its config, or probe internal/metrics-db then metrics.sqlite3) instead of hardcoding; add a doctor row 'git-ai metrics store: <path> <exists/missing> <newest row age>' so a missing store is visible before an ingest is queued; and make the ingest route fail FAST on an unopenable store rather than accepting and failing in the job."
    fp: "7a1bc79b46c5"
    disposition: fixed-now
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "metrics_db_path probe (#118)"
  - id: DL-005
    kind: difficulty
    description: "git-ai transcript capture is high-fidelity when it runs and silently unreliable about WHEN it runs. Measured on my own session vs the native jsonl: 6001/6002 assistant message ids, 27636/27638 record uuids, identical block counts, largest tool_result 55871/55881 chars, 15/15 subagent sidecars parent-linked \u2014 a raw copy of the native records. But: (1) my session's stream stalled at byte 84,661,106 on 2026-09-04 22:14:53 with processing_errors=0 and last_error NULL, 9.92 days after first record, and two hand-fired hooks (one with GIT_AI_TRANSCRIPT_STREAMING_LOOKBACK_DAYS=60) did not advance it; (2) a finished 47-record session (974459cf, novels) has ZERO rows while its sibling in the same dir is current to the minute; (3) git-ai's sweep only visits ~/.claude-alt and ~/.copilot, never ~/.claude/projects, so those sessions depend entirely on the PostToolUse hook path; (4) the record after the last PostToolUse of any session is never forwarded \u2014 the final assistant turn is structurally missing; (5) omp/pi rows stop 2026-09-02 (0 rows for a live 12MB omp session); cursor rows are turn_ended markers with no content. Payloads are each harness's RAW record \u2014 git-ai unifies the envelope (repo, tool, session, parent, ts), not the dialect."
    target: tooling
    severity: degrading
    workaround: "keep native-store readers as the primary path; treat metrics-db as the copilot store plus an optional cross-check"
    suggested_encoding: "if fs3 ever leans on git-ai as a source, a doctor row per tracked stream comparing watermark to actual file size would expose a silent stall in one line; the same check is what fs3's own conversation poller should carry for its cursors."
    fp: "3ec8cb05db4d"
    disposition: kept
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: ""
  - id: CONF-001
    kind: confusion
    description: "CLAUDE.md tells every agent to read .harness/government/how-we-work.md, worker-roster.md, rulings/ \u2014 none of those exist on main; git tracks only .harness/government/README.md. The documents live on the prime-governance branch in the fs3-governance worktree. A fresh coder seat (pij-flaky-jerusalem, plan 018) followed the instruction, found nothing, and had to stop and ask o-prime for a path \u2014 the first action of every new seat is a dead link."
    target: tooling
    severity: degrading
    workaround: "o-prime authorizes read-only access to /Users/jordanknight/substrate/flowspace/fs3-governance/government/ on request"
    suggested_encoding: "either commit the governance docs to main (they are the operating manual, not scratch) or make CLAUDE.md name the governance worktree path explicitly and say it is a separate branch; and have the coder packet template carry the resolved path so no seat has to ask"
    fp: "ca01c1c4749a"
    disposition: kept
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: ""
  - id: DL-006
    kind: difficulty
    description: "A coder seat that lives in the MAIN CLONE and is told to work in a worktree has a useless LSP: rust-analyzer roots at the spawn cwd, so every go-to-definition in the worktree returns 'No definition found' (pij-flaky-jerusalem, plan 018 t1, exact requests captured). The packet template mandates LSP-first navigation for omp seats (i8), so the instruction and the environment contradict each other on every worktree packet where the seat was not spawned inside the tree."
    target: tooling
    severity: degrading
    workaround: "time-boxed workspace-folder repair, then fall back to flowspace3 search/get + grep with exact-read verification, declared in the done report"
    suggested_encoding: "spawn (or re-spawn) coder seats with cwd = the worktree, never the main clone; and have the packet template's i8 say 'if your seat was not spawned in your worktree, expect LSP to be blind there \u2014 say so at ack' so the fallback is planned, not discovered"
    fp: "6d9924b01f08"
    disposition: encoded
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "pij-team i16 (#117)"
  - id: DL-007
    kind: difficulty
    description: "Flaky test on main: crates/providers/src/github_copilot.rs tests build their fake HOME as temp_dir()/fs3-copilot-credential-<pid>-<now_ms>; sibling tests in the same process + same millisecond share the directory and race on remove_dir_all. PR #115 (docs only) went red on github_copilot_file_is_used_before_the_omp_store panicking over a hosts.json it never wrote. Row 195."
    target: tooling
    severity: degrading
    workaround: "re-run the job"
    suggested_encoding: "tempfile::tempdir() per test, or an atomic counter in the name; a millisecond timestamp is not unique inside one process"
    fp: "c6bb2e45e13d"
    disposition: fixed-now
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "#116"
  - id: CONF-002
    kind: confusion
    description: "CORRECTION to DL-002 / row 194: a conversation-ingest trigger DOES exist \u2014 the engineering-harness CLI's convo sync act fires after {\"command\":\"boot\",\"status\":\"error\",\"timestamp\":\"2026-09-06T06:32:53.843Z\",\"error\":{\"code\":\"E_BUILD_FAILED\",\"message\":\"cargo build failed (exit 101)\"},\"next_action\":\"Fix the build failure above, then re-run `harness boot`.\"} and inside {\"command\":\"harness\",\"status\":\"error\",\"timestamp\":\"2026-09-06T06:32:54.386Z\",\"error\":{\"code\":\"E108\",\"message\":\"error: missing required argument 'message'\"},\"next_action\":\"Run `harness help` for usage.\"} when .harness/settings.json flowspace.ingest.enabled is on (it is, here) and identity resolves via CLAUDE_CODE_SESSION_ID or PIJ_SESSION_ID+legacy registry; log at .harness/temp/convo-sync.log, 7 firings since 09-03. I found it only because a tidy-up refused to delete a worktree holding that log. What stands: nothing fires on activity, only the committing session is covered, rs/omp seats are unresolvable \u2014 the rot and the poller diagnosis are unchanged."
    target: tooling
    severity: degrading
    workaround: "none needed; record corrected"
    suggested_encoding: "doctor should show this seam: 'harness convo sync: consent on, identity <resolved|unresolvable>, last fired <age>' \u2014 it fired silently for days and its existence was invisible to the person auditing ingest"
    fp: "4c0839992b03"
    disposition: kept
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: ""
  - id: CONF-003
    kind: confusion
    description: "CORRECTION to DL-002 / row 194 (supersedes the mangled entry just above, whose description embeds a boot error envelope because backticks in a double-quoted shell string executed): a conversation-ingest trigger DOES exist. The engineering-harness CLI convo sync act (harness/cli/src/acts/convo.ts) fires after harness boot and inside harness commit when .harness/settings.json flowspace.ingest.enabled is on (it is, here) and identity resolves via CLAUDE_CODE_SESSION_ID or PIJ_SESSION_ID plus the legacy pij registry; it logs to .harness/temp/convo-sync.log, 7 firings here since 09-03. Found only because a worktree tidy refused to delete that log. What stands: nothing fires on activity, only the committing session is covered, rs and omp seats are unresolvable, so the rot and the poller diagnosis are unchanged."
    target: tooling
    severity: degrading
    workaround: "record corrected in backlog row 194 and the review file"
    suggested_encoding: "doctor should surface this seam as a row: consent on/off, identity resolved/unresolvable, last fired age \u2014 it fired silently for days and was invisible to an ingest audit. Also: never put backticks inside a double-quoted harness observe message."
    fp: "cc022e15ca95"
    disposition: kept
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: ""
  - id: DL-008
    kind: difficulty
    description: "The harness checks gate, when it fails inside fs3-test-suite, leaves the operator with a one-line JSON error that truncates before the failing test names, the delivered-job snapshot omits error/result text, and no gate log is written to disk \u2014 the coder (pij-flaky-jerusalem, plan 018) had to re-run a crate-level cargo test just to learn WHICH tests failed. A red gate that cannot name its failures costs a second full run."
    target: tooling
    severity: degrading
    workaround: "rerun cargo test for the suspected crate with output tee-d to a file"
    suggested_encoding: "harness checks persists the full stdout/stderr of each stage to .harness/temp/checks/<stage>.log and the failure envelope names the file and the first N failing test names"
    fp: "74303c466dfa"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog 208-class gate diagnostics"
  - id: DL-009
    kind: difficulty
    description: "The release build fails at link after an Xcode update: ld cannot find clang_rt.osx because ort-sys build-script output cached in target/release/build/ort-sys-*/output still carries -L <toolchain>/clang/17/lib/darwin from before Xcode moved to clang 21 (Xcode.app mtime 5 Sep 11:43; last good binary 2 Sep 19:29). cargo does not rerun the build script because its declared inputs did not change, so every release build on this box was broken for two days with an error that names a directory that no longer exists. Found only because the ac-0008 bounce needed a fresh binary; the first attempt also masked the failure because exit=$? read tail, not cargo."
    target: tooling
    severity: blocking
    workaround: "cargo clean -p ort-sys --release, then rebuild"
    suggested_encoding: "harness boot should compare the active clang version against the clang path baked into cached ort-sys (and any *-sys) build outputs and say \"toolchain moved: cargo clean -p ort-sys\" before the build runs; and the build recipe must capture cargo exit status with pipefail, never $? after a pipe"
    fp: "ada9d01868f5"
    disposition: kept
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog 201"
  - id: DL-010
    kind: difficulty
    description: "Research subagent on \"how harness telemetry worked\" ran seven flowspace3 searches against the indexed harness-engineering repo: 4 hit, 3 missed, and the misses share a shape. (1) \"harness telemetry command\" lost to an unrelated E100 conversation turn; (2) \"telemetry collector ingress socket\" returned pij UDS transport turns and never the actual source services/doctor/collector/ingress.ts; (3) \"pij sessions join telemetry\" and \"telemetry counts-only push\" returned THE SUBAGENT OWN PROMPT TURN AT SCORE 1.0 as the top result (the live session is indexed and self-retrieves), then chatter. Across all seven, 70-85% of results were kind:turn from flowspace3 own conversations; harness-engineering CODE under harness/cli/src/ barely surfaced \u2014 the top code hit across every search was a flowspace3 Rust test. Every concrete answer came from reading the npm-installed source by hand."
    target: tooling
    severity: degrading
    workaround: "read the source directly; use --source doc/code to exclude turns"
    suggested_encoding: "default search should down-weight or exclude the CALLING session own turns (self-retrieval at 1.0 is never useful), rebalance conversation vs code when the query names a verb/command, and status should show per-repo code element counts so an under-indexed root (harness-engineering cli/src) is visible"
    fp: "23508ec6dc2e"
    disposition: kept
    system:
      compound:
        status: open
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: ""
  - id: DL-011
    kind: difficulty
    description: "harness boot run from a linked worktree reports prod db service stopped and suggests docker compose up -d, while flowspace3-db is healthy \u2014 false negative that invites a prod-touching action"
    target: tooling
    severity: degrading
    workaround: "prime verified with docker ps + flowspace3 ping; coder told to ignore"
    suggested_encoding: "boot db probe should test the configured port (:5433) not the worktree compose project state"
    fp: "e24cbd59d528"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog 209"
  - id: DL-012
    kind: difficulty
    description: "prod flowspace3 tree returned FS3-E-STORE-QUERY-FAILED pool timed out (retry succeeded); store pool is hardcoded max_connections(8) while [indexing] worker_concurrency=32 on this box \u2014 read verbs compete with 32 ingest/summarize workers for 8 connections"
    target: tooling
    severity: degrading
    workaround: "re-run once"
    suggested_encoding: "size the pool from worker_concurrency (+ headroom for read verbs) or reserve read-path connections; doctor row when pool saturation is observed"
    fp: "c040b1261555"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog 212"
  - id: DL-013
    kind: difficulty
    description: "harness checks timed out (exit 124 after 821s in fs3-test-suite, 9th pg_first_light test still running) with no competing cargo; Microsoft Defender at 170% CPU for 1h, load 37-55 on 16 cores"
    target: tooling
    severity: blocking
    workaround: "rerun; run the timed-out test alone"
    suggested_encoding: "gate should print box load + top CPU consumer when a step times out, so a seat can tell environment from code"
    fp: "776320e6fa6f"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog 208/209 family; Defender exclusions added by Jordan"
  - id: DL-014
    kind: difficulty
    description: "boot compose probe calls docker with an unsupported -T flag, so it reports Postgres unavailable while flowspace3-db-test is healthy \u2014 root cause of yesterday false-negative db row (two seats hit it, jerusalem log .harness/temp/agent/omp-tmp-boot.log)"
    target: tooling
    severity: degrading
    workaround: "seat checked the container directly"
    suggested_encoding: "boot extension: drop -T (or use docker compose exec -T only under compose v2 with a TTY check) and probe the configured port instead"
    fp: "dd0ece828c5b"
    disposition: task
    system:
      compound:
        status: suggested
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "backlog 209"
  - id: DL-015
    kind: difficulty
    description: "o-prime instructed a coder to vendor a real-store oracle (173 cwd->dir pairs = a map of the operator filesystem) as a committed fixture in a PUBLIC repo; reviewer escalated before commit"
    target: tooling
    severity: degrading
    workaround: "withdrawn; curated synthetic fixture ruled instead"
    suggested_encoding: "pij-team packet template: any fixture drawn from a real store is curated/synthetic by default; real tables stay in scratch and are cited, never committed"
    fp: "86ccb324d13e"
    disposition: encoded
    system:
      compound:
        status: encoded
        source: agent-self
        first_seen_at: "2026-09-10T23:27:29Z"
        note: "pij-team i20 (this PR)"
---

# Retro — plans 018 / #121 / #122 close-out (2026-09-04 → 2026-09-11)

## What hurt
- A write-time naming rule mirrored exactly and used as a read-time resolver (F1 on #122): the fixture that made the suite green was the one that hid it (o-prime ruling 4). Encoded as pij-team i20 and the deleted-cwd fixture in-repo.
- Fixtures prove the code agrees with its author; a real store proves it agrees with the world. The 173 real omp directories caught what every green suite missed (reviewer grasshopper). Where a rule mirrors an external tool, use that tool's own output as the oracle — but never commit it (DL-015).
- Box-level noise (Defender at 400%, Spotlight) turned a green gate into an 821 s timeout; the gate could not say whether it was code or environment (DL-013 → backlog 208 family).
- Two seats were misled by the same boot false-negative (-T compose probe, DL-011/DL-014 → backlog 209).
- omp seats spawned in the main checkout and told to work in a worktree edited main by relative path (i19 in this PR).

## What worked
- Three-pass delta review bound to immutable shas, mutation receipts red-then-restored for every fix, and a reviewer that named its own miss and proved it by experiment.
- Merge-time guards (head + CLEAN re-read) and a byte-identity check across a docs-only rebase.
- The health row did its job: row 204 was invisible before plan 018 and loud after it.

## Encoded this close-out
- Backlog rows 205–212 (governance, prime-governance 3ac51b9); pij-team template invariants i19–i21; this retro.

