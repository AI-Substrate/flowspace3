# w-embed-microbatch — let smart embeds accumulate + batch-level console reporting

**From**: pij-instant-lynx · 2026-08-30 · Remediation #2 of
`scratch/scan-throughput-review.md` (read it first) PLUS Jordan's console
ruling (verbatim intent below).

## Defect A (measured): smart embeds ship 1 text per HTTP call

Each summary stores its result and enqueues a one-item smart embed
(`crates/daemon/src/enrich.rs:486-500`); the general drain calls
`drain_embed` before each claim cycle (`crates/daemon/src/runner.rs:229-238`)
so it claims the single job that just appeared. Measured on the 959-file
run: **11,232 of 11,727 smart calls carried exactly one text** (mean 1.06),
while raw embeds averaged 17.90. fs2 cuts fixed 16-text calls. At batch 16
the same corpus is ~776 calls — 93.4% fewer.

**Fix**: give the embed lane an accumulation trigger instead of
fire-per-summary — wake on threshold (e.g. 16 pending embed items), OR a
short max-wait (bounded staleness, e.g. 1-2s), OR general-lane idle,
whichever first. Preserve the existing 200k-token call splitting and
retry-travels-alone semantics (batch.rs). The planner already merges — the
trigger cadence is the only broken part.

## Defect B (Jordan's ruling, 2026-08-30, verbatim): per-item console noise

"Rather than just show every single item that it embeds or summarizes, have
it just report on the groupings that it uses — when we send up a batch of 32
to the LLM or to the embedding, just have it say that it's sent a batch to
one of those and it had this many in it."

**Fix**: daemon console logs at BATCH/provider-call granularity: one line
per provider call naming kind, item count, and outcome/duration (e.g.
`embed: sent batch of 32 texts (ok, 840ms)`), and equivalent grouped lines
for summarize dispatch. Per-item lines drop to debug/trace level, not
deleted. Keep the 5s progress cadence as the rollup.

## Proof

- Test: N summaries completing within the window produce ≤ ceil(N/threshold)
  provider calls (mutation-checked: remove the window, test fails).
- A run transcript in the PR showing the new console shape.

## Fence

IN: enrich.rs enqueue/wake path, runner.rs embed drain trigger, logging call
sites, tests. OUT: settlement census (w-settle-hotpath), claim index
(w-claim-index), provider adapters, token-split logic. Standard rules: own
worktree fs3-embed-microbatch, plan-ack before code, harness checks/commit,
PR into main, never prod :7373.
