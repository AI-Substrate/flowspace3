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
