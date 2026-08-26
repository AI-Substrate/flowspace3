# Daemon worker architecture — locked direction (not yet scheduled)
**Jordan, 2026-08-26** (verbatim intent, recorded by o-prime for the upcoming daemon plan): "the daemon … will be watching a list in the database of files, and then the worker will process through them … we can parallelize LLM work as well as embedding work for ultra-fast things, which FlowSpace does really well … a worker that can do jobs off a backlog, and one of the job types will be scan a file."

## The shape

- **The queue lives in Postgres** (PRD: dirty-file work queue). The watcher (host-native, per ruling 2026-08-26) and bulk scans only ENQUEUE; they never process.
- **A worker loop drains the backlog**: generic job runner over typed jobs — `scan_file` is the first job type; enrichment (`summarize`, `embed`) are natural siblings, letting one file's scan fan out into many small enrichment jobs.
- **Parallelism is the point**: LLM calls and embedding calls batch and run concurrently (fs2 does this well — mine its batching/concurrency shapes). The concurrency-combinator roster row (`Batched`/`Throttled`/`Retry` over the two ports) is the provider-side half of this.
- Composes with everything already landed/locked: content-addressed enrichment means the queue is derivable ("elements with no smart_content row for the current model" IS backlog); the pure scanner is the `scan_file` job's core; blob-SHA keying makes duplicate enqueues harmless (idempotent jobs).

## When we get to it
This becomes the daemon plan (with the PG schema workshop deliberately deferred from plan 001). Inputs ready by then: scanner (mollusk), config (egret), migrations (cicada, landed), watcher learnings (sailfish), providers + Azure (kazimir, landed), cross-platform + docker substrate (ox).
