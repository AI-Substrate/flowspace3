# review-010 VERDICT — plan 010-embed-cap-heal, PR #92

**Reviewer**: pij-fiscal-tick (github-copilot/claude-opus-5), cross-model.
**SHA reviewed**: `6377a1fe4b14bc27b7894bd3a997724a87763b7f`, detached in
`/Users/jordanknight/substrate/flowspace/fs3-review-010`.
**Review record**: `docs/plans/010-embed-cap-heal/assets/reviews/review-010.dd.json`
(+ built sibling `review-010.dd.md`, `ddocs build` → ok, 35,233 bytes,
0 adapter warnings, 0 file findings).

## VERDICT

**APPROVE WITH NOTES — merge is not blocked.**

ac-0001 through ac-0004 are all TRUE. Every one was re-derived from a command I
ran and an exit code I read; nothing was taken from the PR body. ac-0005/ac-0006
are correctly open (post-merge, post-bounce, on your known-open list — zero
findings spent).

Three defects, all with one-line fixes, none blocking: **f-0001** (MAJOR) the
heal is adapter-local and a live adapter with the same declared cap gets none of
it; **f-0002** (MINOR) `MAX_HEAL_ROUNDS` is only correct at its shipped value;
**f-0003** (MINOR) the cap number is a string literal inside the classifier.

## The three owed lists — discharged

**(2) I disbelieved the receipts.** I re-ran all three commands and read the exit
codes, and I **performed the mutation myself**. It came back **redder than the
author claimed**: guarding the `Error::InputTooLong` arm off turns **three tests
red, not one**, and the failure carries `retryable: true` — the retry-forever
mode. The sharpest evidence in the packet is the exhaustion test's red:
`Drained { completed: 0, retried: 1, failed: 0, parked: 0 }` — without the heal
the job does not even fail, it goes back on the queue. That is plan 010's
problem statement reproduced on demand. Restored, `git status --short crates/`
empty, clean 12/12 re-green.

**(1a) The bound is real.** `input_budget_bytes(7500)` = 15,000 bytes; halved
once = 7,500 bytes against a 7,500-token window = **exactly one byte per token**.
A token can never be fewer than one byte, so a chunk emitted at heal round 1
cannot legitimately exceed an 8,192-token cap. One round genuinely reaches the
provable floor; exhaustion is unreachable against an honest provider and
terminates against a dishonest one. It over-splits nothing — 7,500-byte chunks
are still large, and only a member the provider actually rejected is tightened.
Terminality verified in the SQL (`AND NOT terminal`, `jobs.rs:522`), not just in
the fixture.

**(1b) The bisect is sound.** Terminates (strictly halves a call of len > 1; the
len == 1 case falls to `None => 0` and heals instead). Preserves batch order
(`pending` is a `VecDeque`; both re-queue paths `pop_back` off the freshly
budgeted deque and `push_front`, restoring order at the head, left pushed after
right so it precedes it). Cannot mis-attribute (narrows to a singleton before
healing, so the healed member IS the rejected member by construction).

**(1c) No duplicate or partial rows.** Impossible at this sha. Vectors
accumulate in memory, every failure path returns *before* `put_embeddings`, and
`put_embeddings` is one transaction. The real design win is that chunk numbering
**moved to after all calls complete** — the old code took `chunk_no` from the
`chunk_plan` position, which a re-split *would* have duplicated.

**(1d) Classification is exact one way, brittle the other.** No false positives
(three-way gate, controls green on both adapters). But the cap number is a
string literal → a provider saying 8191 falls through to `Error::Provider` and
returns to retry-forever, silently. That is **f-0003**.

**(3) Known-open respected.** Zero findings on search latency/host load, the
compose container collision, the stale `node_modules/.bin/dd`, or the undrained
prod jobs.

## Findings

| id | sev | one line | fix |
| --- | --- | --- | --- |
| f-0001 | MAJOR | `OpenAiCompatEmbedder` declares the **same 8192 cap** but has its own `try_post` that never calls `embedding_input_too_long` → the heal never runs and the job retries identical bytes into permanent failure. Backs the live `openai_compat` and `github_copilot` kinds. | one line: `openai_compat.rs:202` mirrors `openai.rs:91-92`; the helper is already `pub(crate)` and the route literal is already `"embeddings"` |
| f-0002 | MINOR | `MAX_HEAL_ROUNDS` invites tuning but only 1 is correct. At round 2 the "ratio reached" is integer-divided to 0 and `.max(1)` prints a **false** `1 byte/token`. At round 5 the heal's unclamped 600-byte overlap trips a **release `assert!`** and panics the embed worker. | two lines: reuse `chunk_plan`'s `.min(window_bytes.saturating_sub(1))`; stop printing the ratio through integer division |
| f-0003 | MINOR | `detail.contains("maximum input length is 8192 tokens")` bakes the number into the string, next to a `MAX_INPUT_TOKENS` const documented as changeable. Drift or a differing cap = silent permanent failure. | parse the number instead of asserting it; report the provider's cap, change no constant (stays inside the non-goals) |

**f-0001 is the only one with live consequences.** It breaks no acceptance
criterion (ac-0004 says "openai and azure_openai") and sits outside impl-guide
u1's fence, so it is not a merge blocker — but it *is* plan goal 1's unqualified
sentence being narrower in code than in prose, which is the priority-1 category.
**It wants an explicit ruling from you, not silence**: fold in the one line, or
file it as plan 011's first row. Either is defensible; forgetting it is not.

## Things handed forward to you

1. **impl-guide risk #3's gate was never exercised.** Alignment costs **+51%**
   chunks (41→62), and risk #3 says "if the delta is large, ship the heal alone
   and report — o-prime rules". The measurement was taken and recorded; the
   *ruling* was not. I also bounded it so it is not over-read: **nothing under
   15,000 bytes changes cost at all**; 15,001–22,500-byte elements newly go 1→2;
   only already-oversized content pays the ~50%. The corpus is three synthetic
   items, two very large — 41→62 is the cost on oversized content, not on the
   index. My read: not large enough to withhold the alignment. Your call.
2. **A clean prod drain will NOT exercise the heal.** At 20,872 bytes the prod
   item splits by *alignment alone* (window is 15,000 bytes), and at ≤2.55
   bytes/token the 15,000-byte chunk lands near 6,000 real tokens — under the
   cap. Do not read a green drain as evidence the heal path works; the fixtures
   are what prove that.
3. **Capture `last_error` for the five dedupe_keys BEFORE the bounce.**
   `requeue_failed` overwrites it with "requeued at daemon boot: …", destroying
   the original cap-rejection text. Pre-existing, not this PR's doing — but the
   ac-0005 receipt is meant to show what they died of.
4. **`docs/plans/010-embed-cap-heal/impl-guide.dd.json` does not validate** —
   17 errors, all pre-existing at the reviewed sha (missing `name`,
   `responsibility`, `interface`, `test_strategy`, `wave` on `units[0]`;
   `fan_out`/`isolation`/`composition`/`review` are strings where the schema
   wants objects; `risks[]` rows missing `id`/`text`). My review doc contributes
   **zero** errors — I confirmed by grouping issue owners. Not a PR-92 defect;
   it will keep tripping `ddocs doctor` until fixed.
5. **Harness, from the crash:** `FreshDatabase` should serialise
   `CREATE`/`DROP DATABASE` (or the daemon test binaries should cap
   `--test-threads`) — a test helper that can take down the fleet's database is
   a harness defect. And `crates/testkit/src/fresh_database.rs:46` should
   distinguish "server closed the connection / in recovery" from "no server
   configured": its current advice, "Start it with: `docker compose up -d`", is
   both wrong when the server is up-and-recovering and is itself the known
   container-name collision. Captured as **DL-001** in the shared buffer —
   **captured, not drained**; the drain is yours.
6. **Docs:** `docs/services/enrichment.md` passes i7 — it describes what the code
   does and every number in it reproduces from source. Two small gaps, both
   downstream of f-0001: no row for the openai-compat *embedder* cap (its 6,000
   row is the *chat* cap), and "OpenAI and Azure classify only the 8192-token
   input-cap 400" does not tell the reader a third configured adapter classifies
   nothing. If f-0001 is deferred, that page is where to say so out loud.

## Evidence — every command I ran, with its exit code

| command | exit | result |
| --- | --- | --- |
| `cargo test -p fs3-daemon --lib chunk_plan -- --nocapture` | **0** | 6 passed; printed `oversized 7→10, request_whale 33→50, prod_20_872 1→2, total 41→62` — identical to the ac-0001 receipt |
| `cargo test -p fs3-providers cap_rejection` | **0** | 4 passed — 2 in `openai_stub.rs`, 2 in `azure_openai_stub.rs`, enumerated per binary rather than trusting the total |
| `cargo test -p fs3-daemon --test oversize -- --test-threads=2` | **0** | 12 passed, 0 failed |
| `cargo test -p fs3-daemon --test oversize cap_rejection -- --test-threads=1` **(MUTANT)** | **101** | 0 passed / **3 failed**, all with `retryable: true` |
| same, after `git checkout --` restore | **0** | 12 passed, 0 failed; `git status --short crates/` empty |
| `ddocs build …/reviews/review-010.dd.json` | **0** | ok, 0 adapter warnings, 0 file findings |

Test DB: `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/fs3_review_010`
(created by me for this review). **`:7373` was never touched; no prod database
was read or written by this review.**

## Process notes

- **Packet:** the rendered `packet-reviewer.dd.md` in this worktree was still the
  unfilled template when I opened it (placeholders, no i6/i7, no sha) while the
  `.dd.json` showed `M`. I did not proceed on the template — I read the `.json`
  and got the real packet with all three owed lists. Your 22:41 rebuild message
  confirmed it afterwards. Worth encoding: **the packet contract should be the
  `.dd.json`, or the spawn should block until `ddocs build` has run**, because a
  reviewer who reads only the `.md` starts from an empty brief.
- **Freeze honoured.** No `cargo test` and no `harness checks` between your
  FREEZE and your `REVIEWER CLEARED`; I did read-only hunts instead and ran the
  mutation only after clearance.
- **`pij send` to you failed exactly as `req-0034` documents** (`no registry row
  in the daemon registry`). One attempt only, for the crash disclosure, because
  you were mid-way through characterising prod and needed it; I did **not**
  `pij adopt`. Everything else delivered by file.
- Files written, all inside my fence:
  `.harness/temp/agent/review-010-ack.md`,
  `.harness/temp/agent/review-010-URGENT-db-crash.md`,
  `.harness/temp/agent/review-010-verdict.md` (this file),
  `docs/plans/010-embed-cap-heal/assets/reviews/review-010.dd.json` + `.dd.md`.
  **No code was changed** — the mutation was a temporary working-tree edit,
  restored and verified clean.

**One line for the log:** APPROVE WITH NOTES at 6377a1fe — ac-0001..0004 all
TRUE and independently re-derived, mutation performed and redder than claimed
(3 red, `retryable: true`), one-byte-per-token floor confirmed as a real bound;
3 non-blocking findings, the material one being that `openai_compat` /
`github_copilot` share the 8192 cap and get none of the heal.
