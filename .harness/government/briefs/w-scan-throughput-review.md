# w-scan-throughput-review — indexing pipeline architecture review vs flowspace2

**From**: pij-instant-lynx (o-prime) · 2026-08-29 · Jordan's verbatim intent:
"the scan queue is crap compared to Flowspace 2. It used to be so much faster.
It used to parallelize and scan massive code bases in minutes. It used to be
able to batch up the embeddings, like not only run embedding batches, but also
inject multiple embeddings per call and things. We need to check all of that
out and do a thorough architecture review."

## The job — a comparative architecture review, READ-ONLY, report-first

Deliverable is a REVIEW DOCUMENT (not code): where fs3's indexing pipeline
loses throughput relative to flowspace2's, with measurements, named
mechanisms, and a ranked remediation list Jordan can turn into packets.

### Sides to compare

- **fs3 (this repo)**: crates/daemon runner lanes (`drain_general` workers,
  `drain_embed` batch-64, ingest lane), crates/store jobs.rs (claim shape,
  LIFO `priority DESC, id DESC`, SKIP LOCKED), scan path (roots.rs enqueue →
  scan.rs), embed path (how many TEXTS per provider CALL, not just jobs per
  claim), summarize path (per-instance Semaphore concurrency), DB write
  patterns (row-at-a-time vs batched inserts), watcher debounce/enqueue.
- **flowspace2**: /Users/jordanknight/substrate/fs2/flow_squared (on disk;
  also indexed as git:github.com/AI-Substrate/flow_squared once its scan
  settles). Its scanner/embedding pipeline: worker/process parallelism, how
  it batched N texts into ONE embedding API call, any pipelining between
  parse and embed, and what made "massive codebase in minutes" true.

### Questions that MUST come back with numbers or line-cites

1. **Per-call embedding batch shape**: fs3 claims 64 JOBS per embed drain —
   how many INPUT STRINGS go in one provider HTTP call, and what does fs2 do?
   (Azure/OpenAI embeddings accept large input arrays; if fs3 sends 1 text
   per call or small arrays, that is the headline finding.)
2. **Scan parallelism**: how many general workers actually run concurrently
   in prod config; where is the wall-clock going for a 1000-file repo
   (parse? summarize wait? DB writes? queue claim overhead?). Measure a real
   bootstrap with timestamps (a scratch repo or the flow_squared receipts in
   the jobs table), break down by stage.
3. **Summarize coupling**: does summarize sit in the scan lane's way? fs2
   comparison: did it enrich inline or fully decoupled?
4. **DB write batching**: rows inserted one-at-a-time per file vs batched
   transactions; queue-claim round-trips per job.
5. **Queue overhead**: cost of the jobs table itself at 40k+ rows (index
   shape, claim contention) — is the queue the bottleneck or the work?
6. Fold in the live churn finding from w-scan-churn (pij-motionless-mawhrin)
   when it lands — cite, don't re-investigate.

### Output

`scratch/scan-throughput-review.md` in the MAIN clone (you are read-only in
product code; scratch is yours): per-question findings with file:line cites
on both codebases, measured numbers, then a RANKED list of remediations
(smallest-fix first, expected throughput effect each). O-prime turns the
accepted ones into packets — you do not code them.

## Rules & fence

- READ-ONLY: no product code changes, no prod daemon mutations. Measuring
  via SQL SELECTs on the jobs/queue tables and reading both source trees is
  in-bounds; adding test roots to the PROD daemon is NOT.
- Work from the main clone (no worktree needed — you change nothing).
- Read CLAUDE.md, TENETS.md. Dogfood `flowspace3 search`/`ask` for
  orientation on both repos (fs2 once indexed) and report every miss.
- Numbered plan-of-attack to pij-instant-lynx first (ack-before-work), then
  the report by path pointer.
