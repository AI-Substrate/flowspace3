# Cross-model review — 018-convo-poller (PR #118)

| Field | Value |
| --- | --- |
| plan | docs/plans/018-convo-poller/plan.dd.json |
| pr | https://github.com/AI-Substrate/flowspace3/pull/118 |
| head reviewed | `159ec8168360108b4b7ce68ea503605cb374b885` (immutable; detached worktree `/Users/jordanknight/substrate/flowspace/fs3-review-018`) |
| base | `3ebfd587b73a68a4584705fce2554b10a698d96b` (merge-base) |
| coder | pij-flaky-jerusalem (gpt-5.6-sol) |
| reviewer | pij-relieved-grasshopper (claude-opus-5), cross-model |
| date | 2026-09-06 |

## Verdict

**DELTA APPROVE** — four deltas, none of which touch LLM spend.

The thing Jordan asked about is **safe**: re-indexing cannot re-spend, and I proved it
on real data, not on the author's fixture. What is wrong is the *health signal* the
plan added — on this machine it would report `stalled` within three minutes of the
bounce and never recover, while ingest is working perfectly. The defect is in the
rule that decides what "behind" means, not in the poller's scheduling or its money.

Deltas: **F1** and **F2** (both HIGH, one shared root, one shared fix), **F3** (MEDIUM,
latent), **F4** (MEDIUM, test soundness).

## Acceptance criteria

| AC | Judgement | Evidence |
| --- | --- | --- |
| ac-0001 first/unchanged/sidecar-append on durable per-file cursors | **TRUE** | `convo_poll_first_unchanged_and_sidecar_append_use_durable_file_cursors`; 13/13 poller tests green under my run |
| ac-0002 identity is file-derived | **TRUE** | `convo_poll.rs` reads no env, no `home_dir`, no seat row; only `pij_id: None` on constructed requests (:514, :638) and the phantom fixture the test seeds to be ignored |
| ac-0003 cold-start cap + stagger + lookback | **TRUE** | cap enforced per PASS: 6 passes × exactly 10, `jobs` count `pass*10` with every earlier job still pending (:1066-1085); cap mutation red |
| ac-0004 status/doctor say flowing / stalled / disabled | **FALSE** | see F1 and F2 — `stalled` fires on healthy catch-up and never clears for a record-less file |
| ac-0005 no-session-file distinct from not-indexed | **TRUE** | `require_native_file` (convo_ingest.rs:515-535) keys on path existence alone, independent of cwd/content; `convo_verify_no_file_is_distinct_from_present_but_unindexed`; 404 mapping present; `error-codes.md` diff is exactly one new code |
| ac-0006 metrics-db probe order + synchronous refusal | **TRUE** | one implementation `metrics_db_path` (:645-649) shared by doctor (doctor.rs:758); refusal at :338 precedes `enqueue_job` at :364 and names both candidates (:658-666) |
| ac-0007 two config knobs | **TRUE** | defaults 12/14 (config.rs:1146-1147), zero lookback refused with a Problem naming the key (:1108-1114), both rows + env overrides in `docs/reference/configuration.md`, both in the rendered template examples |
| ac-0008 prod receipt | **not mine** | o-prime's after the bounce. F2 predicts the receipt's "the following pass logs quiet" clause will NOT hold on this machine — see F2. |

## Findings

### F1 — HIGH — `stalled` fires during a healthy cold start

**Where**: `crates/daemon/src/convo_poll.rs:293-306` (lag bookkeeping), `:138-160` (`record`),
`:52-57` (the stall rule's own doc comment).

**Claim**: ac-0004 exists so that "quiet because nothing changed" and "quiet because dead"
stop looking identical (DL-001). The stall rule counts a session as making no progress
whenever its durable cursor is unchanged across three due passes — including a session
that has simply not been submitted yet because it is queued behind the per-pass cap.

**Proof** (probe test at the reviewed sha, 12 cold claude sessions, default `PollConfig`, cap 10):

```
pass=1 enqueued=10 behind=12 state=Flowing reason="catching up: 12 sessions behind; 10 submitted this pass"
pass=2 enqueued=10 behind=12 state=Flowing reason="catching up: 12 sessions behind; 10 submitted this pass"
pass=3 enqueued=10 behind=12 state=Stalled reason="a behind session file has made no durable cursor progress for three due passes"
```

Every pass submitted the full cap. Nothing was stuck. `cap + 1` files is the minimum
reproduction. The plan's own motivating scenario — ~100 cold claude files on this machine
— is ten passes, so `status` and `doctor` would read **stalled for the first ten minutes
of the first bounce**, which is the exact window an operator is watching.

**Why the suite is green**: `convo_poll_cold_start_caps_staggers_and_progresses_past_pending_work`
(:1049-1123) drives six passes across exactly this state and asserts `enqueued`, `behind`
and `skipped` — but never once asserts `state`. The fixture that would expose this already
exists and is not looking.

**Smallest fix**: accrue lag only for sessions the poller has actually *submitted* on a
previous pass. A session waiting its turn under the cap is catching up, not stalled.
`record()` already builds the correct "catching up: N sessions behind" reason at :149-153
— it is simply unreachable, because `stats.stalled` wins the branch above it.

### F2 — HIGH — a session file that yields zero records is behind forever

**Where**: `crates/daemon/src/convo_ingest.rs:799-806` (early return before `commit_poll`),
`convo_poll.rs:462-473` (`behind()` treats a `None` cursor as unconditionally behind),
`:293-306` (the same lag counter as F1).

**Mechanism**: when a read produces no records for a session that has never been stored,
ingest returns before committing a cursor — deliberately, and the comment explains why
(an empty header plus a cursor would be a row nothing can ever fill). The poller has no
way to learn that this happened, so the file is behind on every subsequent pass, forever.

**Proof** (probe test, fixture is the exact shape of a real abandoned session read off this
machine — `~/.claude/projects/-Users-jordanknight-substrate-harness-engineering/7e5d1607-….jsonl`:
`mode` / `system` / `cost-state` rows, cwd present on the `system` row, no turn ever). Ingest
was actually **run** between passes:

```
pass=1 enqueued=1 behind=1 state=Flowing  jobs=[ingest_session:1]
pass=2 enqueued=1 behind=1 state=Flowing  jobs=[ingest_session:2]
pass=3 enqueued=1 behind=1 state=STALLED  jobs=[ingest_session:3]
pass=4 enqueued=1 behind=1 state=STALLED  jobs=[ingest_session:4]
```

Three consequences, all permanent for the file's 14-day lookback:

1. one fresh `ingest_session` job **per cadence, forever**;
2. `behind > 0` forever, so the pass never takes the DEBUG branch at `:434-448` — it logs
   INFO every minute, and `status` reads "catching up" or "stalled" and never "quiet";
3. `state = Stalled` from pass 3 onward on a completely healthy system.

**Prevalence on this machine** (read-only census over the real stores, applying each reader's
own drop rules — claude accepts only `user`/`assistant` and drops a `user` row whose body and
items are both empty; omp requires `id`+`timestamp` and type `message`(role user|assistant) /
`compaction` / `custom_message`):

| | in-window files | zero-record files |
| --- | --- | --- |
| claude | 199 | 5 |
| omp | 411 | 2 |
| **total** | **610** (of 915 native jsonl) | **7**, all with a readable cwd |

Seven files ⇒ **7 `ingest_session` jobs/minute = 420/hour = 10,080/day** of pure churn at the
default cadence, and a `conversations` row that reads STALLED from three minutes after the
bounce onwards. This is a **lower bound**: I counted only files with no qualifying record at
all, not files whose every qualifying record is later dropped.

**Spend**: zero `summarize`, zero `embed` on every pass. This costs queue churn and a lying
health row, **not money**.

**Smallest fix**: the same negative acknowledgement F3 needs — after an ingest that read the
file and produced nothing, record `(path, FileStamp)` as *asked, nothing to do*, and treat it
as not-behind until the stamp changes.

### F3 — MEDIUM — no negative cwd cache (hunt 14)

**Where**: `convo_poll.rs:224-234` (`folder()` inserts into the cache only on `Some`),
`:388-392` (the miss path warns and `continue`s), `convo_ingest.rs:537` + `:550-552`
(`CWD_READ_LIMIT = 8 MiB`, 64 records).

A permanently cwd-less file re-pays up to 8 MiB of reading **per due pass**, forever, and
emits a `warn!` every pass. The author's own test proves the worst case is real: a
newline-free file reads exactly `CWD_READ_LIMIT` and returns `None`.

Additionally — and this is not in the retry cost, it is worse — `after_boot` is **sticky**
(`:245-249`: once `true`, `is_none_or(|seen| seen.after_boot)` keeps it `true`), and `fresh`
(`:322-328`) stays true while the file has no cursor. So a cwd-less file first seen *after*
the boot scan is pinned in `Priority::New` forever and consumes one of the capped priority
slots (`:366`, `cap - 1 = 9`) on every pass, instead of rotating out through the backlog tier.
Only files present at the boot scan degrade to `Backlog` and rotate.

**Graded MEDIUM, not HIGH, because prevalence today is ZERO**: 0 of 610 in-window files lack a
cwd. This is a missing guard with no current instances — latent cost, not current cost. I am
saying so explicitly because my first report of this hunt implied otherwise.

**Also**: `stats.skipped` conflates "outside the lookback window" (`:289`) with "no readable
cwd" (`:390`), so the pass log cannot tell an operator which one happened.

**Smallest fix**: one negative cache keyed on `(path, FileStamp)`, shared with F2 — retry when
the file changes, which is the only moment the answer could change.

### F4 — MEDIUM — unsound `set_var` in a 16-test binary; the SAFETY comment names the wrong invariant

**Where**: `crates/daemon/tests/conversation_query.rs:653`.

```rust
// SAFETY: this is the only test in this binary resolving native HOME.
unsafe { std::env::set_var("HOME", home.path()) };
```

The soundness condition for `set_var` under edition 2024 is that **no other thread touches the
environment concurrently** — not that no other test *reads HOME*. This binary has 16
`#[tokio::test]`s which libtest runs concurrently by default, and they call into sqlx, tokio
and tempfile, all of which consult the environment. `tempfile::tempdir()` reads `TMPDIR` inside
this very test. So the stated justification does not establish the property it claims.

By contrast `crates/cli/tests/convo_poll_isolation.rs:38` uses the identical pattern **soundly**,
because that binary genuinely contains exactly one test.

Worth noting the inconsistency with the PR's own design: t5 built the explicit-home seam
(`verify_at_home` / `ingest_at_home`) specifically to avoid mutating process HOME in tests, and
this is the one place that mutates it anyway.

**Why the change was needed at all** (hunt 12's question — why did an existing daemon test have
to change?): the diff is +12/−0, pure setup, no assertion touched. `conversation_verify_contract`
asserts NOT-FOUND for three session ids; after this PR, verify checks file presence first, so
with no file on disk those ids would now return the new NO-SESSION-FILE code. Seeding empty
session files preserves the *original* case (present-but-unindexed → NOT-FOUND). The change is
correct and required — only its HOME mechanism is unsound.

**Smallest fix**: move `conversation_verify_contract` into its own integration binary, which
restores the one-test-per-binary invariant its SAFETY comment already assumes.

## The headline: spend (hunt 11)

**Re-indexing cannot re-spend.** Proven three ways.

**The oracle is sound** — checked before trusting any number from it. `convo_spend.rs`'s
`job_counts()` sums `fs3_store::queue_depth_history`, and `HISTORY_QUEUE_DEPTH_SQL`
(`jobs.rs:571-575`) carries **no state predicate**: it counts every row ever created, `done`
included. "No new jobs" therefore means no job was ever created, not "the queue happens to be
empty". That was the one way this test could have been quietly meaningless.

**The author's test, run by me** (exit 0):

```
initial = {embed: 3, ingest_session: 1, summarize: 2}
rescan  = {embed: 3, ingest_session: 2, summarize: 2}
append  = {embed: 5, ingest_session: 3, summarize: 3}
```

**My own proof, on real data** — a 16,097,495-byte claude transcript with 22 sidecar files,
copied into a sealed tempdir HOME, 12 session files ingested three times through the real
pipeline:

| run | records read | turns new | deduped | summarized | `summarize` jobs | `embed` jobs |
| --- | --- | --- | --- | --- | --- | --- |
| 1st | 4493 | 4493 | 0 | 3112 | 3109 | 286 |
| 2nd (unchanged) | 0 | 0 | 0 | 0 | **3109** | **286** |
| 3rd (forced rescan, new inode, identical bytes) | 3956 | 0 | 3956 | 0 | **3109** | **286** |

Zero new `summarize`, zero new `embed` on both re-runs, on real merged assistant groups and
real sidecars.

Two things worth keeping from that run:

- the rescan re-read 3956 records, not 4493, because only the rotated MAIN file rescanned —
  the 21 sidecars kept their own durable cursors and read nothing. That is risk-1's per-file
  cursor requirement working, demonstrated rather than asserted.
- **one 16 MB session = 3,109 `summarize` jobs on first ingest.** That is the per-session cost
  of a first bounce, and the concrete shape of risk-2's burst. It belongs in the ac-0008 receipt.

## Mutations run, and what went red

Every one re-derived by me at the reviewed sha, then restored.

| # | Mutation | Result | Red at |
| --- | --- | --- | --- |
| M-spend | empty `BTreeSet` for `view.seen` at the `prepare_batch` call site (convo_ingest.rs:863) | **RED**, exit 101 | `convo_spend.rs:186` — "ledger dedupe must prevent NEW summarize jobs on rescan", left 4 right 2 |
| M-home-1 | removed the HOME pin from `sealed()` (spawn.rs:222) | **RED** | `convo_poll_isolation.rs:67` — the *env-var* assertion |
| M-home-2 | removed the pin **and** bypassed that env assertion | **RED** | `convo_poll_isolation.rs:111` — "outside the sealed home is not a session source", left `Number(1)` right `0` |
| M-stagger | `effective_stagger` returns a fixed 6s, cadence ignored | **RED** | `convo_poll.rs:857` — left `6s` right `500ms` |
| M-tiering | every candidate forced to `Priority::Backlog` | **RED** (2 tests) | `:684` "live OMP must be enqueued on the NEXT due pass", left `("claude","backlog-010")` right `("omp","tracked-omp")`; `:756` "newest mtime wins, not path order", left `"cold-03"` right `"new-0-2"` |

**On i12 specifically** — the sealed-HOME mutation had to be run in two stages, because one
stage would have lied. M-home-1's red proves only that the pin is *set*; had I stopped there I
would have reported an env-var tautology as a ratchet. M-home-2 removed the assertion as well,
so the behavioural check had to do the work — and it still went red, on the daemon actually
seeing and tracking the ambient session. **The guard is genuinely defended.** (The mutation was
safe to run because the test sets HOME to its own fake ambient tempdir at `:38` before spawning,
so the unpinned child inherits the fake home; it never had a path to the developer's real
`~/.claude`. Checked before running, not after.)

## Hunts that produced no finding

| Hunt | Verdict |
| --- | --- |
| 1 identity file-derived | clean — no env, no `home_dir`, no seat/pij read in the poller |
| 2 `size != offset` OR identity change | both branches present (`:462-473`); truncation and `#[cfg(unix)]` same-size-replacement tests exist; `tail.rs` diff is exactly two `fn`→`pub fn` changes with identical bodies, and the doc comment at `:160-165` names the non-unix `(0,0)` degradation and says rotation is then caught only by the size-below-offset rule |
| 3 cap per pass | enforced per pass, and later candidates progress while every earlier job is still pending |
| 4 sealed HOME | pinned, asserted, and behaviourally defended — see M-home-2 |
| 5 metrics-db probe | one shared implementation, native-first, refusal precedes enqueue and names both candidates |
| 6 verify/ingest asymmetry | presence keyed on path existence alone; a present file lacking cwd is not classed absent |
| 7 status states | boot flowing-pending, ticks=0 disabled, 179s/180s staleness boundary tested on both sides. The *mechanics* are right; the stall *rule* is F1/F2 |
| 8 stagger | doc comment at `:27-30` names the `GREATEST(not_before)` upsert as the reason; mutation red |
| 9 404 mapping | one new code, `error-codes.md` diff is exactly one row block, no other mapping moved |
| 10 priority tiering | reserved backlog slot at `:366`; path order used only *within* the backlog tier; mutation red on both tests |
| 11 spend | see above — safe, proven independently |
| 12 shared-test content | `status.json` golden gains only the additive `conversations` block, every other field byte-identical; `envelope_goldens.rs` +26/−0, two new cases only; `conversation_query.rs` explained above (correct and required; see F4 for its HOME mechanism) |
| 13 runner.rs | +9/−0, a single `INGEST_SESSION` arm inside `subject_of`, mirroring the existing pattern. Nothing touching claim order, `SKIP LOCKED`, concurrency, retry, parking or terminal policy |

## Note for prime (outside this PR's scope)

A **green** `cargo test -p fs3-daemon` leaves a scratch database behind (`fs3_worktreelife_*`).
I dropped the one my run created. This is pre-existing test hygiene, not this PR's doing.

## Negative fence — what I did not touch

- **Shared test database `:5434/flowspace3_test`: exactly 53,416 job rows before my first cargo
  invocation and exactly 53,416 after all of them, including a full `cargo test -p fs3-daemon`
  under an empty temp HOME.** Native ingest rows (`dedupe_key like 'ingest:claude/%'` or
  `'ingest:omp/%'`): **0 before, 0 after**. The suite wrote nothing to the shared database at all.
- **Scratch databases I created, I destroyed**: 3 `fs3_convopoll_*`, 1 `fs3_convospend_*`,
  2 `fs3_convohomeiso_*`, 1 `fs3_worktreelife_*` — all dropped. The 12 databases that were on
  the server when I arrived (attributed by embedded timestamp, all predating my first cargo run)
  are **untouched and still present**.
- **Code**: `git diff --stat -- crates/` is empty. Every mutation was red-then-restored; the four
  files I temporarily edited (`convo_poll.rs`, `convo_ingest.rs`, `testkit/src/spawn.rs`,
  `convo_poll_isolation.rs`) are byte-identical to `159ec81`, verified per-file after each restore
  and again at the end. The three probe tests I appended are deleted.
- **Never went near**: `:5433`, `:7373`, the production daemon, production credentials, the real
  `~/.claude` and `~/.omp` as a poller home (read-only census only — I never pointed the poller or
  a daemon at `~`), the main checkout, and any other seat's worktree.
- **Author's tests: not modified.** Every proof of mine is a separate probe, so no green in this
  record comes from a test I relaxed.
- **Gates I did not run because they are not mine to hold**: `harness checks` (fmt / clippy `-D
  warnings` / full `cargo test --all` / migration guards / architecture). I ran only targeted
  suites and one full `-p fs3-daemon`. The final gate verdict is o-prime's.
- **Not re-found, per the known-open list**: copilot polling via metrics-db, the `cwd_of` bounded
  read as a packet correction, ac-0008's production receipt.

---

# Delta re-review — 2026-09-06

| Field | Value |
| --- | --- |
| head reviewed | `ed4698e673ed78dbde9c733b09a665050626ed87` |
| baseline | `159ec8168360108b4b7ce68ea503605cb374b885` (the sha reviewed above) |
| scope | `git diff 159ec81..ed4698e` only — 8 files, +1539/−230 |
| coder gate | exit 0, 11/11, 2026-09-06T10:53:55Z |

## Delta verdict

**DELTA APPROVE** — F1, F2, F3 and F4 are all genuinely fixed, each re-derived by me rather
than read out of the receipt. One new finding, **D1**, introduced by the F1 fix.

## The four originals, re-derived

**F1 — FIXED.** My own 100-file cold-drain probe, jobs deliberately left pending:
`state=Flowing` on every pass. The named split landed as ruled —
`convo_poll_cold_start_healthy_catch_up_stays_flowing`,
`convo_poll_cold_start_pending_queue_is_fair_and_stays_flowing`, and
`convo_poll_11_and_12_cold_files_remain_flowing_for_six_healthy_passes` on the cap boundary.

The (2b) ratchet I asked for is real, and stronger than I specified —
`convo_poll_health_counts_terminal_attempts_before_resubmission_and_recovers` drives pending
for six passes and *running* for six more with `no_progress_attempts` pinned at 0, then one
completed job → 1, then two failed jobs → 2 and 3 with **`updated_at` forced to a constant**
so a timestamp collision cannot hide an attempt, `Stalled` at exactly the third, then restart
amnesia → 0 and `Flowing`. My worry that F1 could be "fixed" by never saying stalled is closed
by that test, not by argument.

**F2 — FIXED**, and fixed at the right layer. My own probe, the same real abandoned-session
shape as before, ingest actually run between passes:

```
pass=1 enqueued=1 behind=1 state=Flowing jobs=[ingest_session:1]
pass=2 enqueued=0 behind=0 state=Flowing jobs=[ingest_session:1]
pass=3 enqueued=0 behind=0 state=Flowing jobs=[ingest_session:1]
pass=4 enqueued=0 behind=0 state=Flowing jobs=[ingest_session:1]
conversations rows=0
```

Quiet from pass 2, the job count flat at one, and **zero conversation rows** — the ack was taken
without minting a header, which was the constraint that made this hard. The sharp check I named
in advance passes: `NoContentAck` is published from the **ingest outcome**
(`convo_ingest.rs`, both the headerless early-return path and after `commit_poll`), not from the
poller's cwd resolution — so the zero-record route is covered and not merely the cwd-less one.
The ack is stamped before the read and re-verified after it
(`stamp.identifies(&batch.cursor) && FileStamp::read(path) == Some(stamp)`), so a file appended
or rotated mid-read is never falsely acknowledged. `ack_epoch` scopes acks to one poller
lifetime, so the cost is one ingest job per file per process, then silence.

**F3 — FIXED.** `CachedFolder::Missing(stamp)` caches the negative, the retry happens only when
the stamp changes, the first miss warns and subsequent ones log DEBUG, and the unchanged-miss
case is skipped *before* it consumes a priority slot. The sticky-`New` pin is gone: `new_pending`
is cleared on a probe or submission, so a cwd-less file no longer occupies a capped slot forever.

**F4 — FIXED.** `conversation_verify_contract` moved into `crates/daemon/tests/conversation_verify.rs`,
which contains **exactly one** test; the old binary now has 15 tests and **zero** `set_var` calls.
Both counted, not assumed. Original assertions preserved.

## The new store helper — my four named checks

| Check | Result |
| --- | --- |
| index-backed, not merely key-bounded | **PASS** — `WHERE id = ANY($1)`, and the test asserts on an `EXPLAIN (FORMAT JSON)` that the plan contains `jobs_pkey` and does **not** contain `Seq Scan`, with 4096 unrelated rows and an `ANALYZE` first so the planner has a reason to choose a scan |
| read before submit | **PASS** — outcomes are fetched near the top of `poll()`, above the submission loop, with the reason in the comment; the (2b) test asserts the terminal state is observed *before* the upsert revives the row (`row.state == "pending"`, `attempts == 0` afterwards) |
| missing row = no information | **PASS** — `observe()` returns early on `None` ("Retention is not progress"); the unit test proves a purged row neither increments nor clears, and the store test proves a deleted id and `i64::MAX` return nothing |
| double-count, both directions | **PASS** — revision key is `(id, attempts, is_failed)`, never the timestamp; both tests pin the inverse direction I was worried about, `"equal timestamps must not hide distinct attempts"` |

Also confirmed: SELECT-only, no schema change, chunked at 256 with a 604-id test, and the
`enqueue_job_id` refactor is safe — the `ON CONFLICT … DO UPDATE` carries no `WHERE`, so
`RETURNING id` always yields exactly one row and the `execute` → `fetch_one` change on the shared
enqueue path cannot start erroring for other job kinds.

**My push on the public envelope was taken**: `QueuedIngest` is `pub(crate)` and wraps the
response; `IngestAccepted` is unchanged and `envelope_goldens` still passes 2/2, so no agent-facing
contract moved to carry a queue row handle.

## D1 — MEDIUM — `behind` now counts submitted work, and climbs while the backlog drains

**Where**: `convo_poll.rs:497-509` (`summary.behind` counted from the `lag` map), against
`crates/core/src/views/status.rs:79` and `crates/cli/src/render/surfaces/status.rs:88`.

The F1 fix keys lag on submitted work — correct, and exactly as ruled. But `summary.behind` was
re-derived from that same `lag` map, so the agent-facing `behind` field no longer means what its
own doc comment says. `views/status.rs:79` still reads *"Parent sessions needing an append,
truncation, or replacement read"*, the CLI still renders *"N tracked · N behind"*, and the plan
goal still defines behind as *"size > cursor"*. None of those moved in the delta.

**Proof** (my probe, 100 cold claude files, cap 10, jobs left pending):

```
pass=1 enqueued=10 tracked=100 FIELD_behind=10  actually_unsubmitted=90  state=Flowing
pass=2 enqueued=10 tracked=100 FIELD_behind=20  actually_unsubmitted=80  state=Flowing
pass=3 enqueued=10 tracked=100 FIELD_behind=30  actually_unsubmitted=70  state=Flowing
pass=4 enqueued=10 tracked=100 FIELD_behind=40  actually_unsubmitted=60  state=Flowing
```

The field is **inverted**: it rises 10 → 20 → 30 → 40 as the real backlog falls 90 → 80 → 70 → 60.
An operator or agent reading `status` during a cold start is told the backlog is 10 when it is 90,
and watching it "grow" while the daemon is in fact catching up. `state` is correct throughout —
this is a gauge defect, not a health defect.

It is also what makes the new reason string read oddly: *"10 behind, 10 in flight"* is one set
counted twice, because after the redefinition `behind` and `in_flight` are nearly the same thing.

To be fair to the coder: this is the ruling implemented faithfully. The gap is that the ruling's
consequence for the envelope was never carried out to the doc comment, the CLI label, or the plan
text — so the *number* changed meaning while everything describing it did not.

**Smallest fix** (preferred): let `summary.behind` count every eligible session for which
`behind(file, cursor, ack)` is true — the true backlog gauge, restoring the documented meaning —
and surface the lag set separately. `stats.in_flight` is already computed and already in the
reason string, so nothing new has to be derived, and the stall rule is unaffected because it keys
on `lag`, never on `summary.behind`. The alternative — keep the value and update the doc comment,
the CLI label and the plan goal — is honest but strictly worse, because it leaves the operator
without any number for "how much is left".

## Delta evidence

| Suite | Result |
| --- | --- |
| `fs3-daemon --lib convo_poll` | 19 passed |
| `fs3-daemon --test conversation_query` | 15 passed |
| `fs3-daemon --test conversation_verify` (new binary) | 1 passed |
| `fs3-store --lib ingest_outcome` | 1 passed |
| `fs3-cli --test convo_spend` | 1 passed — counts **identical** to the baseline sha: initial `{embed:3, ingest_session:1, summarize:2}`, rescan `{3,2,2}`, append `{5,3,3}` |
| `fs3-cli --test envelope_goldens` | 2 passed |
| ledger-bypass mutation (required: `convo_ingest.rs` moved) | **RED**, exit 101, at `convo_spend.rs:186`, left 4 right 2 — then restored green |

The sealed-HOME two-stage re-run was **not** required: `spawn.rs` and `convo_poll_isolation.rs`
are not in the delta, so by my own rule the earlier proof still stands.

## Delta negative fence

- **Shared `:5434/flowspace3_test`: 53,416 job rows at the start of the delta pass and 53,416 at
  the end; native ingest rows 0 and 0.** Identical to the baseline pass — two full review passes
  have now left the shared database bit-for-bit unchanged.
- **Scratch**: one database leaked by my deliberately-red bypass run
  (`fs3_convospend_1788692592…`), dropped. Databases created inside the coder's own gate window
  (20:49–20:52) and its implementation window (20:29–20:31) were attributed by the epoch embedded
  in each name and **left alone**.
- **Code**: `git diff --stat -- crates/` empty; the one mutation restored and re-verified; both
  delta probes deleted. The author's tests were not modified in this pass either.
- **Untouched**: `:5433`, `:7373`, production, the real `~/.claude` and `~/.omp` as a poller home,
  the main checkout, other seats' trees.
- **Gate not run**: `harness checks` in full remains o-prime's.

---

# D1 hunk re-review — 2026-09-06

| Field | Value |
| --- | --- |
| head reviewed | `3d35555dd72386e3e4bc26fa9da404c279f95780` |
| base | `ed4698e673ed78dbde9c733b09a665050626ed87` |
| scope | `git diff ed4698e..3d35555` — 2 files, +33/−28 (`convo_poll.rs`, `execution.log.md`) |
| coder gate | exit 0, 2026-09-06T11:28:39Z |

## Verdict

**APPROVE.** D1 is fixed, the fix is pinned by a mutation, and nothing else moved.

This closes the review: every finding from both passes — F1, F2, F3, F4, D1 — is resolved and
each resolution was re-derived here rather than accepted from a receipt.

## The four checks I named in advance

**1. `summary.behind` sourced from `behind()`, not the lag map — PASS.** The increment now sits
inside the true backlog predicate (`convo_poll.rs:368-376`, the `eligible(file) && behind(file,
cursor, ack)` block) and the twelve-line lag-derived re-derivation after the submission loop is
deleted outright. One gauge, one source.

**2. Both fixtures assert their two shapes — PASS**, and as the shapes I specified rather than as
first ruled. `cold_start_state_proof(healthy)` now runs 100 files over ten passes:

- pending branch: `(behind, in_flight) == (100, pass * 10)` — flat backlog, climbing in-flight
- healthy branch: `(behind, in_flight) == (100 - (pass-1) * 10, 10)` — falling backlog, capped in-flight

with `Flowing` asserted on every pass of both. The fixture also grew from 60 files to 100, so it
now exercises the plan's actual motivating scale rather than a scaled-down proxy.

**3. Stall rule untouched — PASS.** `stats.stalled` still reads `lag.values().any(|pending|
pending.no_progress_attempts >= STALL_PASSES)`, unchanged and not in the diff; `in_flight` and
`unknown` still derive from `lag`. The gauge and the health signal now read from different
sources, which is the separation the fix exists to create.

**4. cwd-negative first probe reads `(0, 1)` — PASS**, and I agree with the reading: on that pass
the file has never been read, has no cursor and no ack, so it is genuinely behind. See the NOTE
below for the one thing this leaves unsaid.

## My own re-derivation

The same 100-file cold drain that found the inverted gauge, re-run independently of the coder's
fixture — first four passes with jobs left pending, then three passes draining for real:

```
pending  pass=1 behind=100 in_flight=10 Flowing  "catching up: 100 behind, 10 in flight, 0 outcomes unavailable; 10 submitted this pass"
pending  pass=2 behind=100 in_flight=20 Flowing
pending  pass=3 behind=100 in_flight=30 Flowing
pending  pass=4 behind=100 in_flight=40 Flowing
draining pass=5 behind=60  in_flight=10 Flowing
draining pass=6 behind=50  in_flight=10 Flowing
draining pass=7 behind=40  in_flight=10 Flowing
```

The gauge no longer inverts: it holds at the true backlog while nothing drains, and falls only as
cursors advance. The reason string now carries two different numbers measuring two different
things (`100 behind, 40 in flight`) instead of one set counted twice.

## Mutation — is the fix a ratchet?

I reintroduced D1 exactly: put the lag-derived re-derivation back after the submission loop.
**16 passed, 3 FAILED**, at the assertions that exist to pin this:

| Test | Failure |
| --- | --- |
| `convo_poll_cold_start_healthy_catch_up_stays_flowing` | `:1663` "backlog falls only as cursors advance; in-flight stays capped", left `(10, 10)` right `(100, 10)` |
| `convo_poll_cold_start_pending_queue_is_fair_and_stays_flowing` | `:1669` "pending work remains behind while unique in-flight jobs grow", left `(10, 10)` right `(100, 10)` |
| `convo_poll_cwd_negative_cache_reads_once_per_stamp_and_demotes_new` | `:899`, the `(0, 1)` first-probe tuple |

Restored; 19/19 green. A future refactor cannot silently put the inverted gauge back.

Worth noting which test did *not* go red: `convo_poll_11_and_12_cold_files_remain_flowing_for_six_healthy_passes`
stayed green, because its assertion moved to `in_flight <= 10`. That is correct, not a gap — that
test guards the health state under the cap, and the gauge is guarded by the three above.

## NOTE — the two different zeroes (backlog row 199)

Not a finding, recorded so the record points at the follow-up rather than leaving it implicit.

`behind = 0` now means two different things. After a no-content ack it means *"I read it and there
was genuinely nothing to index."* After a cached cwd miss it means *"there is something to index
and I cannot read it."* Both render identically in the envelope, so a store holding a permanently
unreadable session is byte-indistinguishable from a healthy quiet one — DL-001's shape in
miniature.

It does not earn a finding: the census found **zero** cwd-less files across 610 in-window files, so
it has no reachable instances on this machine, and the first miss does warn. But `stats.skipped`
still conflates out-of-window with unreadable and never reaches the reason string, so nothing an
agent reads through the envelope can separate them. Tracked as **backlog row 199** — split
`skipped` into `out_of_window` / `unreadable`, put the unreadable count in the reason string, and
give it a doctor line.

## Evidence

| Suite | Result |
| --- | --- |
| `fs3-daemon --lib convo_poll` | 19 passed |
| `fs3-cli --test convo_spend` | 1 passed — counts identical across all three shas: initial `{embed:3, ingest_session:1, summarize:2}`, rescan `{3,2,2}`, append `{5,3,3}` |
| `fs3-cli --test envelope_goldens` | 2 passed |
| D1 regression mutation | **RED** on 3 named tests, restored green |

Suites outside the hunk were not re-run: only `convo_poll.rs` moved, and by the rule I set for
myself that does not oblige the store, verify, query or isolation binaries, whose proofs at
`ed4698e` and `159ec81` still stand.

## Hunk negative fence

- **Shared `:5434/flowspace3_test`: 53,416 job rows and 0 native ingest rows — at the start and at
  the end, for the third consecutive review pass.** Three passes, zero perturbation.
- **Scratch**: five databases leaked by my deliberately-red mutation runs
  (`fs3_convopoll_1788694419…` ×3, `fs3_convopoll_1788694440…` ×2), all dropped. Databases from
  the coder's own windows were attributed by the epoch in each name and left alone.
- **Code**: `git diff --stat -- crates/` empty; mutation restored and re-verified by a green
  19/19; my probe deleted. The author's tests were not modified in this pass either.
- **Untouched**: `:5433`, `:7373`, production, the real `~/.claude` and `~/.omp` as a poller home,
  the main checkout, other seats' trees.
- **Gate not run**: `harness checks` in full remains o-prime's. The `streaming.rs` timing flake on
  the coder's first gate run (backlog row 200) is outside this hunk and I did not investigate it.
