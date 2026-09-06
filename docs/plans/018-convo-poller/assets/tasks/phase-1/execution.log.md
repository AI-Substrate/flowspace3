# 018-convo-poller — execution log

## Approved start

O-prime approved the acknowledgment at `.harness/temp/agent/convo-poller-ack.md` and released implementation to t1. `harness boot --json`: `cargo build --all-targets` passed; overall degraded only because default compose `db` is absent. Accepted explicitly; never start :5433. Test database URL is pinned to :5434. No production daemon mutation; no `flowspace3 add` of this worktree.

### Packet corrections and authorized additions

- **Noteworthy — composition fence:** minimal daemon module declaration and AppState field/constructor wiring authorized. Read fence includes wiring, module declarations, cursor/session definitions, relevant tests/testkit constructors, manifests (read-only), catalog exports and CLI status rendering. No dependencies or migrations authorized.
- **Noteworthy — cwd discovery:** `cwd_of` reads the entire transcript before examining 64 lines. Replace with bounded BufRead in t2, retain per-path cache; prove bounded reads with a counting reader including a multi-MB first line.
- **Noteworthy — ac-0005 amendment:** existing-but-unindexed **verify** returns NOT-FOUND; ingest of the present file stays accepted. Refusing initial ingest would defeat the feature.
- **Noteworthy — lookback amendment:** only old **cursorless** files are skipped; tracked files stay eligible regardless of age. The impl-guide sentence suggesting otherwise is superseded; ddoc unchanged by ruling.
- Cold-start policy accepted: cap 10 parent sessions/pass, six-second stagger; a PollConfig policy, not a third user knob. Fairness while earlier jobs remain pending must have a behavioral test.
- Bulk `fs3_store::ingest_cursors::load_cursors` is read-only; existing cursor/ledger semantics and reader identities remain frozen. Catalog name `QUERY_CONVERSATION_NO_SESSION_FILE` accepted.
- Missing template rendering is o-prime-owned; leave `.agents/skills/pij-team/templates/impl-guide.dd.*` untouched. Global `ddocs` and `harness commit` are the operating surfaces. Expected rs conversation identity warning ignored under req-0033.

## t1 — in progress

Read the existing Reconcile/pass cadence, retention tick pattern, single-key cursor lookup and cursor timestamp persistence, and AppState construction. Product search/get found `ingest_cursors::load_cursor`; verified bytes in the assigned worktree. rust-analyzer reports ready but worktree definition requests for SourceCursor and AppState return none; reported exact requests and captured DL-003. Explicit workspace registration through the LSP request adapter was refused (`-32601`, notification-only method); no silent edit of main or fallback to main-source mutations.

### t1 receipt

Added `ConvoPoller`, explicit-home discovery, the native-source sidecar resolver, durable per-file `load_cursors` reads, and tick/disabled behavior. `ingest_at_home` factors only the existing home lookup into a crate-local injection seam; tests run the real ingest pipeline without mutating process HOME. Minimal daemon module declaration added under approved fence.

- `cargo test -p fs3-daemon --lib convo_poll -- --nocapture`: 2 passed (first/unchanged/sidecar-append with real disposable Postgres and existing readers; disabled never connects).
- Mutation: `size > offset` → `size <= offset`; same first/unchanged test failed with `changed: 2` versus expected `0` (artifact://19). Restored comparison; 2 passed again.
- Dedupe keys asserted for Claude/OMP; sidecar has its own durable cursor and re-enqueues only its parent request. Bulk read omits a missing ID and returns durable UTC timestamps.
- Worktree-resident build output names `fs3-018-convo-poller/crates/{store,daemon}`. All databases use :5434; all native roots use injected tempdir homes.
- LSP fallback approved after the single failed repair; semantic flowspace navigation plus exact-identifier and exact-byte worktree reads used. No reader/ledger/guid semantics changed.

## t2 — in progress

### t2 receipt

File identity and parent-only enqueue are exercised through the real queue and existing ingest. A fake pij row naming `recorded-but-never-written` yields no request; the actual filename and recorded cwd are asserted. Successful cwd lookup is cached per file path and reused on later discovery; missing cwd is retried, not cached as permanently absent.

Packet correction implemented: `cwd_of` delegates to a bounded buffered reader (64 records, at most 8 MiB including read-ahead). Counting-reader test resolves a valid 3 MiB first record in a 32 MiB tempfile while reading under 4 MiB; a 32 MiB newline-free file reads exactly the 8 MiB cap and returns no cwd.

- `cargo test -p fs3-daemon --lib convo_poll`: 3 passed.
- `cargo test -p fs3-daemon --lib cwd_lookup`: 1 passed (artifact://23).

## t3 — in progress

### t3 receipt

Cap 10, stagger 6s, cursorless-only 14-day lookback; deterministic path rotation advances beyond pending jobs. Discovery now stats files before reading cwd; only selected sessions incur bounded content reads, so old archives and beyond-cap backlog do not defeat the cold-start bound.

- `cargo test -p fs3-daemon --lib convo_poll`: 4 passed.
- 60-file test: six passes of exactly 10 new queue keys while all previous jobs stay pending; exact 0/6/…/54s eligibility offsets; no cursor means `behind` remains 60 until actual ingest. Existing pipeline ingests 60, following pass submits zero.
- Old cursorless session counted skipped and absent from cwd cache; tracked file with old mtime and appended turn stays eligible.
- Mutation: removed `.take(max_per_pass)`; cold-start assertion failed at 60 versus 10 (artifact://27). Restored cap; all four poller tests passed.

## t4 — in progress

### t4 receipt

Shared in-memory `PollHealth` supplies additive status data, including o-prime-approved `state` and optional daemon-authored `state_reason`. A behind file with unchanged durable cursor for three successful due passes stalls; two passes still flow. Three missed configured cadences also stalls the snapshot. Boot begins flowing-pending; disabled has precedence. CLI and doctor render the daemon verdict without re-deriving it.

- Daemon `convo_poll` tests: 6 passed, including three-pass boundary, recovery after actual ingestion, and stale age boundary 179s/180s at default cadence.
- CLI `conversations_`: 4 passed, including bearer-authenticated `/status` probe and all three render states.
- `envelope_goldens`: 2 passed. Only status response/stdout extended; conversation goldens untouched.
- `doctor_daemon`: 7 passed; healthy fake daemon now supplies the newly-consumed status endpoint, and doctor tests use one injected tempdir HOME.
- **Noteworthy — authorized fence additions:** existing core views/mod.rs round-trip literal gains only `conversations: None`; existing wiring.rs `now()` becomes pub(crate), behavior unchanged. No second date formatter.
- **Noteworthy — interface amendment:** shared `metrics_db_path(&Path) -> Option<PathBuf>` is public because doctor is in the CLI crate. Read-only ordered native/legacy probe is shared, not duplicated; synchronous source-open refusal follows in t6.

## t5 — in progress

### t5 receipt

Native presence is checked independently of cwd/content parsing before delivery lookup or queue acceptance. Absent Claude/OMP files return `FS3-E-QUERY-CONVERSATION-NO-SESSION-FILE` and the lazy-persistence fix; a present empty file returns existing NOT-FOUND on verify but is accepted for ingest. Inaccessible discovery produces its real provider failure, not a fabricated absent-file diagnosis. Existing folder fallback reuses this lookup.

The public verify/submit wrappers retain their signatures; crate-local explicit-home variants carry the SAME bodies for poller/tests, avoiding process-HOME mutation in unit tests. Existing HTTP verification fixture now seeds its native files in a tempdir HOME.

- `cargo test -p fs3-daemon --lib convo_`: 19 passed.
- Existing `conversation_verify_contract` HTTP test: 1 passed.
- `envelope_goldens`: 2 passed; two new refusal cases added, existing conversation bytes untouched. Initial new expectations incorrectly included an empty details map; corrected those authored fixtures only to match the pre-existing omit-empty serialization rule.

### Additional requested coexistence proof

`convo_poll_and_boot_commit_seam_submit_share_one_live_job` passed: poll + native boot/commit-style submit share one queue row/dedupe key; one real ingest commits the cursor; the next poll submits zero.

**Noteworthy — evidence correction from o-prime:** engineering-harness already has an occasional boot/commit `convo sync` submitter. What is missing is activity-triggered scheduling and rs/omp identity coverage, not every hook everywhere. The poller coexists with that submitter; no packet rewrite requested.

## t6 — in progress

### t6 receipt

Actual `MetricsDbSource` construction uses the shared native-first path probe. Submission validates the existing read-only reader's scoped resolve before enqueue, off the async thread; absent candidates name both paths, invalid SQLite fails synchronously, and repository scope remains mandatory. Git scope discovery also inherits the explicitly injected home.

- Targeted metrics test passed using scratch copies of the committed SQLite fixture: legacy-only, native-only, both (native wins), neither, and corrupt preferred store.
- Missing store leaves zero queue rows; valid repeat submissions keep one live key; no reader implementation changed.
- Swapped-order mutation initially failed while constructing an incorrectly implementation-derived fixture. Strengthened the oracle to independently authored native/legacy paths; mutation then failed exactly at `native wins when BOTH stores exist` (artifact://48). Restored order; targeted test passed.
- Doctor row test covers missing, native, and legacy-only resolved paths.

## t7 — in progress

**Noteworthy — approved poll decision amendment:** enqueue on size unequal to cursor offset OR changed native identity. Status behind counts growth, truncation, and replacement; the existing tail reset/ledger path remains untouched. O-prime requires truncation, same-size replacement, unchanged identity quietness, and identity-check removal mutation proofs. Existing tail identity is private; requested visibility-only seam rather than copying it. Non-Unix identity is deliberately `(0, 0)` in the existing reader; same-size identity proof is Unix-only.

### Rotation correction receipt

Poller now compares file length AND the identity returned by the existing `tail::identity` helper. Truncation schedules the existing reader reset and persists offset zero; same-sized inode replacement schedules a rescan; the existing ledger keeps the turn count at one; subsequent same-size/same-identity polls are quiet.

- **Authorized fence addition:** `crates/providers/src/conversation_sources/tail.rs` changes only the two identity helper visibilities from `fn` to `pub fn`. The module was already public, so no export change needed. Both helper bodies and cursor/reader semantics remain identical.
- `cargo test -p fs3-daemon --lib convo_poll`: 10 passed, including truncation, Unix replacement, quietness, and disabled-before-first-poll.
- Mutation: removed only the identity comparison; Unix replacement assertion failed at zero submitted versus one expected (artifact://58). Restored check; all ten passed.
- Reader tripwire: `cargo test -p fs3-providers --test conversation_sources`: 16 passed, source test untouched (artifact://60).
- Non-Unix `(0,0)` identity limitation remains exactly the existing reader's; same-size replacement proof is `#[cfg(unix)]`, never claimed portable.

Configuration knobs and boot registration added; a disabled configuration does not require or inspect HOME. Configuration reference owner located: no dedicated TEMPLATE constant exists; defaults serialize through config show and Rustdoc examples; `config_reference.rs` requires every key/environment override in docs/reference/configuration.md. Awaiting the exact two-row documentation fence addition.

### Native-home seal and actual-daemon isolation

- **Authorized fence addition:** testkit/src/spawn.rs now creates `config_dir/home` and pins HOME in the common `sealed` command. Existing FS3 namespace scrubbing remains intact; the environment contract asserts both pins. Callers can still deliberately override after construction.
- Poller home has one production injection site in boot; `convo_poll.rs` never resolves HOME itself. Native readers and explicit-home submit/verify test seams retain the injected root.
- `cargo test -p fs3-testkit spawn::tests`: 2 passed.
- `cargo test -p fs3-cli --test convo_poll_isolation`: 1 passed. Actual daemon, poller enabled every five seconds, real-looking Claude file in a fake ambient HOME outside the sealed HOME; two completed polls observed, tracked sessions zero, total ingest jobs zero.
- Full existing daemon suite after registration: `cargo test -p fs3-daemon` under an additional empty temp HOME/FS3_CONFIG_DIR: **419 passed across 34 suites; 2 ignored** (artifact://71). No failed test was narrowed away.
- Configuration reference checks: 3 passed; two indexing rows/env overrides added under the explicit documentation fence. Defaults and Rustdoc examples carry 12 ticks/14 days; zero ticks is valid, zero lookback is refused.

### Live CLI smoke and fast-cadence correction

Actual `target/debug/flowspace3 daemon` ran under a worktree-local scratch HOME/FS3_CONFIG_DIR, fake providers, an ephemeral listener, and its own database on :5434. Claude and OMP each reached one indexed turn automatically; appended bytes reached two without a manual ingest. Actual TTY status rendered flowing, both harnesses tracked, zero behind; doctor rendered the daemon-authored flowing state and git-ai store missing. Missing OMP identity returned the new catalog code/fix through the real CLI. Responses retained under `.harness/temp/agent/convo-poller-smoke/`.

**Noteworthy — approved packet correction:** that smoke used ticks=1 (5s) and measured 10.8s for two appends. Fixed 6s staggering exceeded the cadence and repeated GREATEST(not_before) upserts could postpone work. Effective stagger is now `min(stagger, cadence / cap)`: default remains exactly 6s; five-second polls use 0.5s. The doc comment names the upsert semantics so this cannot be simplified back innocently.

`convo_poll_fast_cadence_delivers_append_within_cadence_plus_spacing` claims only actually eligible queue jobs, runs the existing ingest, and asserts the appended second session reaches two turns within 5s + 0.5s. Default 6s and fast 0.5s spacing also asserted. Restored complete poller suite: **11 passed** (artifact://82).

Post-full-daemon-suite audit on :5434 `flowspace3_test.jobs`: native `ingest:claude/%`/`ingest:omp/%` rows **0 before and 0 after**; no unseeded native rows observed. O-prime accepted the audit and granted the final checks slot. Gate uses a scratch HOME/config; its guard config explicitly names :5434 and never targets :5433 or :7373.

### First gate failure and corrected catalog contract

First `harness checks` reached fs3-test-suite and exited `E_CHECKS_FAILED`/101. The async tool truncated its JSON error line and then omitted failed result text on later snapshots; this tooling gap was observed and reported, not hidden. A separately authorized core-lib recovery passed 218/218; tokens/update tail names were not failing tests.

Exact integration failure recovered under an authorized short slot: `test the_generated_docs_page_matches_the_catalog ... FAILED` (core/tests/error_codes.rs), 0 passed/1 failed: the new catalog code lacked its generated reference row.

**Authorized fence addition:** docs/reference/error-codes.md is generator-owned. Ran only `FS3_UPDATE_DOCS=1 cargo test -p fs3-core --test error_codes` to regenerate; then reran without the flag: 1 passed. The generated diff contains only the new missing-session-file row. Full failure/regeneration/recheck logs are in `.harness/temp/agent/convo-catalog-{before,regenerate,after}.log`.

### Final persisted gate receipt

Supervised `harness checks` exited **0**, envelope `status: ok`, **11 gates passed**, **12 documentation links checked**. fmt, clippy with `-D warnings`, fs3-test-suite, both migration guards and architecture all report `ok: true, code: 0`. Full stdout/stderr: `.harness/temp/agent/convo-gate-full.log`. The gate does not emit an aggregate Rust-test count; no count is invented. Slot released to o-prime immediately.

The earlier unawaited eval-process launch was interrupted with its runtime globals lost and log empty; it supplied no verdict. Replaced it with a synchronous capture wrapper owned by a supervised bash job. Friction recorded as DL-007; only the successful supervised run above is claimed.

Latest compiled binary smoke after the cadence correction: both native streams advanced from two to three turns automatically in **1.246 seconds**; no manual ingest was called. Repeated subsequent pass logs show `enqueued=0 behind=0 skipped=0`. Actual HTTP absence response is **404**, with the new catalog code. Restarting the same scratch daemon with zero ticks produced `state=disabled`, `last_poll_at=null`, and doctor outcome `info`/disabled. Runtime receipts: `convo-poller-smoke/{final-append,final-status,disabled-status-doctor}.json`. Scratch daemon stopped.

### AC-0008 priority ruling requested

Conditional source-backed risk: with 100 behind Claude candidates preceding a tracked OMP path, current path-order rotation and a cap of ten delay OMP beyond two cadences. A post-boot new path can also fall behind the rotation cursor. Existing local proof establishes fair bounded draining, not live-session priority during cold backlog. Proposed tracked/new-file priority plus a reserved backlog slot and an explicit stress test; alternative is an explicit o-prime amendment to AC-0008's timing window. No change made without ruling; PR held at this checkpoint.

### Accepted live-priority correction

O-prime chose implementation, not weakening ac-0008. Each pass now fills the cap from known files changed since the previous scan, then post-boot new files awaiting first ingestion, newest mtime first within each tier. Boot uses existing durable cursors to recognize already-known sessions. At least one cold-backlog slot is reserved; only that tier uses path rotation. File observations are metadata-only and never replace durable cursors as the ingest decision.

- `convo_poll_priority_serves_tracked_omp_and_fresh_claude_ahead_of_100_backlog_files`: 100 cold Claude files plus a pre-ingested OMP session; after the boot scan, an OMP append and fresh Claude in a new directory sorting before the rotation cursor are both enqueued on the very next due pass, in live/new order. All 100 cold cursors eventually commit; backlog ends at zero.
- `convo_poll_priority_reserves_a_backlog_slot_during_continuous_new_arrivals`: continuous new files occupy newest-first priority slots but each pass still selects a cold file; both tiers eventually drain.
- Targeted priority tests: 2 passed (artifact://109). Mutation forcing all candidates to Backlog: both tests failed at the real selection assertions — `claude/backlog-010` instead of tracked OMP; `cold-03` instead of newest arrival (artifact://111). Priority restored.

No new knobs, dependency, migration, reader semantics, or runner claim policy. The fast-cadence test writes its non-target session last so the target remains in the delayed slot under newest-first priority.

### Post-priority final verification

`cargo test -p fs3-daemon --lib convo_poll`: **13 passed**, 0 failed (artifact://113). Final supervised `harness checks` completed at **2026-09-06T08:12:01Z**, exited **0**, `status: ok`, **11/11 gates**, **12 doc links**. fmt, clippy `-D warnings`, complete test suite, migration guards and architecture all passed. Full stdout/stderr retained in `.harness/temp/agent/convo-gate-full.log`; gate slot released immediately.

Dogfood: `flowspace3 search "how conversation polling prioritizes live file changes over cold backlog" --path crates/daemon/src/convo_poll.rs --limit 3` finds the newly written 100-backlog priority proof and ConvoPoller implementation in this worktree (top scores 0.665/0.650), without `flowspace3 add`.

Code acceptance ac-0001..0007 is complete. ac-0008 remains o-prime's real-production receipt after review, merge and bounce; the local fixture smoke is not substituted for it.

### Delivery preparation and cleanup

`git status --short` in the assigned worktree reported only the scoped source/test/generated-document changes; `git branch --show-current` returned `018-convo-poller` (artifact://121). No main-clone edits or commits.

Scratch daemon stopped. Its dedicated :5434 database was dropped. Seven databases left by intentional red mutation runs were identified by this unit's test labels, audited against the exact seeded native identities and tempdir folders, then dropped; audit retained at `.harness/temp/agent/convo-mutation-db-audit.json`. Unrelated databases were not touched. Post-final-gate shared `flowspace3_test.jobs` native-ingest count remains **0**.

All proof logs and harness observations are retained. The observation buffer was listed, never cleared; drain remains o-prime-owned.

## t8 — PR opened; production receipt blocked on o-prime

Implementation committed as `7f70291e3965618a9831cfb5b7afc9bfb9c3c285` (`feat: poll native conversations and surface ingest health`) through `harness commit`; reported mode `direct-verified`, ingress probe connected. Branch pushed and PR opened: https://github.com/AI-Substrate/flowspace3/pull/118.

Task t8 and ac-0008 are explicitly blocked awaiting o-prime's review, merge, production bounce and `assets/inputs/prod-after.md` receipt. Coder does not merge or mutate production. Receipt-only tracking updates follow the implementation commit; no source changes after the final green gate.

## Jordan spend-proof extension after PR #118 opened

Added an actual-daemon test on :5434 with sealed tempdir HOME and fake providers; it asserts database job counts by kind, not inferred cursor positions. Initial two-turn ingestion: ingest_session=1, summarize=2, embed=3 (one raw batch plus two smart embeddings). The unchanged next poll leaves all counts identical. Identity rescan runs one ingest job and yields read=2/new=0/deduped=2/summarized=0 with summarize/embed counts unchanged (2/3). One appended turn changes totals to ingest_session=3, summarize=3, embed=5: precisely one summary and two embedding jobs. Restart leaves counts unchanged and the new report explicitly unavailable until this process observes an ingest.

Mutation: temporarily passed an empty seen set to prepare_batch at its existing call site; the rescan spend assertion failed with 4 summarize jobs versus 2 expected (artifact://140). Restored the actual ledger input; no permanent normalizer, ledger, reader, ordinal or GUID change.

Status and doctor now cite a compact projection of actual IngestReport counters, not cursor timestamps. The latest report is in-memory and explicitly unavailable after restart; no migration. The bulk cursor helper now returns plain SourceCursor values and no longer allocates unused formatted last-read timestamps.

Authorized runner.rs fence addition is logging projection only: the existing subject_of function now derives an ingest address from the payload. No claim, execution, retry, parking or concurrency logic changed. Ingestion emits one INFO receipt per session file with address/subject, records_read, turns_new, deduped, summarized and rescanned. Quiet poll passes emit DEBUG enqueued/behind/skipped; work/behind passes remain INFO. The actual daemon test asserts these lines, populated done subjects, and no quiet INFO spam.

Focused verification after restoring the mutation: poller 13 passed; actual spend test 1 passed; envelope goldens 2 passed; diagnostic group 4 passed; newest-ingest renderer 1 passed (artifact://144). Only status goldens gained receipt fields; existing conversation goldens unchanged. The forced identity-rescan test is Unix-only, consistent with the unchanged non-Unix identity limitation.

Per the latest ruling, these changes are committed/pushed before the final supervised full gate. That final verdict and immutable review head will be reported in the retained gate log and coder report; no further push after naming the review head.

The failed spend-mutation database was audited before cleanup: only seeded `018-spend-session` under its tempdir HOME, with ingest=2/summarize=4/embed=6 exactly matching the intentional dedupe bypass. It was dropped; successful real-daemon spend tests stop both process instances and destroy their databases. No production resources or unrelated databases touched.

## Cross-model review delta F1–F4

The reviewed baseline was `159ec8168360108b4b7ce68ea503605cb374b885`. Full review copied byte-identically to `assets/reviews/cross-model-review.md`; both source/archive MD5 are `de1865aa43c2f842d83cc9fbc2a106d7`. The archive is not edited. The review independently confirmed no repeated LLM spend, and identified health/acknowledgment defects plus one test-environment race.

### Final ruled behavior (supersedes elapsed-pass aging)

- F1: only submitted work is behind. Pending/running jobs remain Flowing; their unique count is explicit in the reason. Lag increments only for observed done/failed attempts without cursor/read-ACK progress; failed counts once. Outcome revisions use job ID, attempts and terminal/resubmission transitions, never timestamp uniqueness. Missing/purged rows supply no information and cannot increment or clear counters. ID/lag state is process-local: restart requires three fresh observed attempts.
- Outcomes are read BEFORE any new enqueue can overwrite them. New SELECT-only `ingest_job_outcomes` uses `jobs_pkey` and chunks/deduplicates ID lists at 256; an EXPLAIN test with representative unrelated rows asserts the PK index and no Seq Scan. Enqueue surfaces the inserted/upserted ID through a shared implementation and the crate-local submission result only; public `IngestAccepted`/conversation goldens are unchanged. Queue claim/retry/parking/priority semantics remain unchanged.
- F2: a stable actual zero-record read publishes a `(path, FileStamp)` no-content ACK from ingestion. With an existing real header, the ordinary empty-ledger `commit_poll` stores the exact reader cursor. Without a header, the ACK is process-local: no fabricated header/timestamp and no cursor forced past a partial line. An unchanged headerless file costs one job per poller lifetime; a new poller over the same store costs exactly one more, then quiet. Stamp changes invalidate the ACK.
- F3: cwd misses are cached by stamp, warn once then log DEBUG, and re-read only on change. Actual probe/submission consumes New priority; cap deferral does not. Unchanged negative hits are skipped before spending a slot.
- F4: `conversation_verify_contract` moved out of the 16-test binary into `conversation_verify.rs`, with one synchronous test setting HOME before constructing runtime/server/database threads. Original assertions preserved.

### Delta evidence

- Poller suite: 19 passed. Named 60-file healthy catch-up now executes each batch and asserts Flowing every pass. Separate original pending-queue fairness case asserts Flowing with the growing in-flight count on all six passes. 11/12-file cases and the 100-file drain assert health as well.
- Seven zero-record fixtures (five Claude/two OMP) assert no invented rows, job count flat at seven, behind=0/Flowing from pass 2; a new poller adds exactly seven more jobs and returns quiet. Existing-header and torn-line tests preserve exact durable offsets and ingest the later completed turn once.
- Terminal attempt test covers pending/running over many passes, done and failed attempts, same-row revival observed before upsert, third-attempt Stalled, and restart amnesia. Revision test proves identical rereads count once, equal timestamps/different attempts count separately, and missing rows preserve counters.
- Store outcome/PK/chunk test: 1 passed; isolated verification binary: 1 passed; remaining conversation-query binary: 15 passed. Envelope goldens: 2 passed. Spend suite remains 1 passed with unchanged initial/rescan/append counts.
- Mutations: counting in-flight outcomes makes the health test red (Stalled vs Flowing, artifact://165); dropping the actual ingestion ACK makes pass 2 enqueue seven again (artifact://167); bypassing negative-cache hits reopens six times (artifact://169). All restored. Ledger bypass remains red at summarize=4 versus 2 (artifact://173), then restored spend is green.

No reader, normalizer, GUID, ledger-write, migration, or runner execution-policy change. Additional approved store fence is ID-return plumbing plus SELECT-only outcome projection/export/test. Final supervised harness gate is run under the same sealed HOME/:5434 discipline before the one delta push and immutable-head handoff.

Delta mutation cleanup: audited `fs3_convospend_1788690954_000000000000a70318d2b4f237c7f1e0` on :5434. It contained only seeded `018-spend-session` jobs under a temporary HOME, with ingest=2/summarize=4/embed=6 and zero other connections. Dropped only that database through `flowspace3-db-test`. The missing PATH client was captured as DL-009; observations and proof logs remain for o-prime, never cleared.

Final delta gate, `2026-09-06T10:53:55.389Z`: `harness checks exited 0`; envelope `"status": "ok"`, `"passed": 11`, `"doc_links_checked": 12`. All eleven gates have `"ok": true`, `"code": 0`, including fmt, clippy with warnings denied, the complete test-suite orchestrator, both migration guards and architecture. Supervised sealed HOME/config and :5434; full stdout/stderr retained in `.harness/temp/agent/convo-gate-full.log`. No source change after this verdict. One delta push follows for immutable-head rereview; production acceptance remains blocked on o-prime.

## D1 — restore backlog gauge after delta rereview

O-prime reported DELTA APPROVE for F1–F4 on `ed4698e673ed78dbde9c733b09a665050626ed87`, with one gauge defect in the earlier ruling. Corrected ruling: `behind` counts eligible cursor/stamp/ACK backlog, not submissions; lag and unique in-flight counts retain their independent meanings. Incremented the harness summary at the existing eligible `behind()` candidate predicate and removed the lag-map re-derivation. No lag/stall, scheduling, store, envelope, or golden change.

The shared cold fixture now holds 100 files. Pending-only passes assert `(behind, in_flight) = (100, 10), (100, 20), …, (100, 100)`; healthy passes execute batches and assert `(100, 10), (90, 10), …, (10, 10)`, then behind=0. Both remain Flowing, preserving fairness and delay assertions. Existing submitted-only gauge expectations were migrated: the 11/12-file cap and reason checks use in-flight; the first unprobed cwd miss has backlog=1, then unchanged cached misses remain 0. Focused poller suite: 19 passed (artifact://193); first run identified that last stale cwd expectation (18 passed/1 failed, artifact://190), not a cache/health regression. D1 and migration feedback retained as DL-011/DL-012. One supervised full gate follows under sealed HOME/config and :5434.

First D1 full gate, `2026-09-06T11:16:23.171Z`, exited 1: `streaming.rs:207` expected one `embed: sent batch` provider line and captured zero. No streaming/runner edit authorized or made. O-prime ruled three sequential sealed focused runs, then one full-gate rerun if all green. All three passed (one test each, 0.60s/0.64s/0.55s), with outputs in `.harness/temp/agent/d1-streaming-{1,2,3}.log`; the first gate failure is preserved in `d1-gate-streaming-failure.log`. The selector is the literal provider-line prefix, and the fixture calls `runner::drain` directly, not the poller. O-prime owns the flaky-test row; the conditionally granted full gate is rerun without source changes. Push awaits explicit approval. Failed cwd-probe fixture residue was identified by label/run time, audited empty with no other connections, and removed; unrelated databases untouched.

O-prime accepted the three receipts and filed the streaming timing flake as backlog row 200; no out-of-fence fix. Publication is pre-authorized if the one rerun exits 0; a second streaming red requires evidence and no third full rerun. The failed streaming database contained exactly four seeded `git:test` raw embed jobs, all done, with no other connections; only that owned :5434 residue was dropped.

Authorized D1 rerun, `2026-09-06T11:28:39.199Z`: `harness checks exited 0`, envelope `"status": "ok"`, `"passed": 11`, `"doc_links_checked": 12`; every gate `"ok": true`, `"code": 0`. Full workspace tests, fmt, clippy, guards and architecture passed. Same sealed HOME/config and :5434; no source change since the 19-test focused pass. Full output remains in `.harness/temp/agent/convo-gate-full.log`. One pre-authorized push follows, then immutable-head handoff and stop.

## Final review record — docs-only closeout

O-prime reported REVIEW CLOSED — APPROVE at `3d35555dd72386e3e4bc26fa9da404c279f95780`, with F1–F4 and D1 confirmed by the reviewer's own runs. Replaced the first review archive with the complete final source, including delta and D1 sections. Source and committed-path MD5 both verify as `0fb0061b8e4121dc75181c8ec8c5cbdc`; copied byte-identically, not edited.

Updated review-related task receipts t1–t5/t7 and t8 through `ddocs set`, regenerating the sibling task document. The cold-fixture description now matches 100 files, and health receipts describe terminal attempts rather than elapsed passes. t8 records PR/review completion; its production bounce/ac-0008 portion remains blocked pending o-prime merge and `assets/inputs/prod-after.md`, with no production acceptance invented. No code changed; approved code remains `3d35555`. Exact requested docs commit and one push preserve the final record before worktree teardown.
