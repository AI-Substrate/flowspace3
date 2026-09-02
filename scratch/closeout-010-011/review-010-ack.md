# review-010 ACK — canary + read receipt

## Identity

- `pij whoami` (quoted verbatim):

```
pij session: pij-fiscal-tick
folder:      /Users/jordanknight/substrate/flowspace/fs3-review-010
state:       idle
role:        —
data dir:    — (rs seats have no data dir; the legacy store's concept)
```

- Role: cross-model REVIEWER, plan 010-embed-cap-heal, PR #92.
- SHA under review: `6377a1fe4b14bc27b7894bd3a997724a87763b7f`, detached in
  `/Users/jordanknight/substrate/flowspace/fs3-review-010`. Confirmed by
  `git log --oneline -3` → `6377a1f fix: heal embedding cap rejections`.
- Delivery: BY FILE (rs seat; no `pij send` to o-prime attempted, no `pij adopt`).
- Inbox message from `pij-instant-lynx` received and read.

## Read receipt

Read before starting work:

1. `docs/plans/010-embed-cap-heal/packet-reviewer.dd.json` — the AUTHORITATIVE
   copy. Note for o-prime: at the time I first opened it, the rendered
   `packet-reviewer.dd.md` in this worktree was still the **unfilled template**
   (placeholders `<plan slug>`, `<ord>`, no i6/i7, no sha) and the `.dd.json`
   showed as `M` (uncommitted). I did NOT proceed on the template; I read the
   `.json` and got the real packet. Your 22:41 message confirms the rebuild.
2. `docs/plans/010-embed-cap-heal/plan.dd.md` — goals, non-goals, ac-0001..0006.
3. `crates/core/src/tokens.rs` — `BYTES_PER_TOKEN=3`, `FILL=(2,3)`,
   `input_budget_bytes`.
4. Full diff `133445a..6377a1f` (15 files, +785/-272), read in full for
   `crates/core/src/error.rs`, `crates/core/src/tokens.rs`,
   `crates/providers/src/openai.rs`, `crates/providers/src/azure_openai.rs`,
   `crates/daemon/src/answer.rs`, `crates/daemon/src/enrich.rs`.

### The three owed lists — confirmed present (packet i6)

1. **Least confident** (hunt first): (a) single-round re-split to the claimed
   one-byte/token floor — is the bound real, does exhaustion terminate, does the
   floor over-split; (b) the BISECT path when the body carries no valid
   `input[N]` — termination, batch order, mis-attribution; (c) no duplicate or
   partial chunk rows when a heal round fails midway (PK
   `(source_hash, source_kind, chunk_no, model_key)`); (d) exact-phrase
   classification `maximum input length is 8192 tokens`.
2. **Disbelieve the receipts**: re-run the three test commands myself and read
   exit codes; PERFORM the mutation myself (remove the `Error::InputTooLong` arm,
   confirm red, restore); re-derive 7→10, 33→50, 1→2, 41→62 from test output.
3. **Known-open, zero findings spent**: search latency / host load (row 122); the
   compose `flowspace3-db` container-name collision (row 110 family); the builder
   skill's stale `node_modules/.bin/dd`; the five prod jobs NOT yet drained
   (ac-0005/0006 — post-merge+bounce, out of this review).

Plus i7: judge `docs/services/enrichment.md` size-cliff section for factual
agreement with the code.

## Numbered plan

1. Read plan, impl-guide, backpressure survey, full diff. **[done]**
2. Re-derive the alignment numbers by running
   `cargo test -p fs3-daemon chunk_plan -- --nocapture` myself; compare printed
   output against ac-0001's receipt, not the prose.
3. Re-run `cargo test -p fs3-providers cap_rejection` and
   `cargo test -p fs3-daemon --test oversize`; read exit status.
4. Perform the mutation myself (delete the `Error::InputTooLong` arm in
   `embed_items`), confirm the heal test goes RED, restore with `git checkout --`,
   and verify the tree is clean afterwards.
5. Hunt (1a): derive the ACTUAL byte/token ratio one heal round reaches from
   `FILL`/`BYTES_PER_TOKEN`, and test whether the reported "final ratio" string
   matches it. Test whether an input SMALLER than the halved window is re-split
   at all.
6. Hunt (1b): trace bisect for termination, order preservation, mis-attribution.
7. Hunt (1c): trace `completed`/`vectors` alignment and the end-of-run
   `chunk_no` renumbering; check `put_embeddings` for stale-row handling across
   a re-embed that changes the chunk count.
8. Hunt (1d): classification exactness on both adapters, incl. the hardcoded
   `8192` literal in the phrase match.
9. Judge `docs/services/enrichment.md` against the code (i7).
10. Judge every AC true/false with cited evidence; examine composition seams.
11. Write the review ddoc under
    `docs/plans/010-embed-cap-heal/assets/reviews/`, build it, and write
    `.harness/temp/agent/review-010-verdict.md`.

## Fence acknowledged

Read-only on code. I write only
`docs/plans/010-embed-cap-heal/assets/reviews/` and `.harness/temp/agent/`.
No commits, no merges, no government files. The mutation check in step 4 is a
temporary working-tree edit, restored immediately and verified clean; it is not
a code change and will not be committed.

Test DBs: per-run `FS3_TEST_DATABASE_URL`; never `:7373`. One `cargo test`
invocation at a time (host load is high).
