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
