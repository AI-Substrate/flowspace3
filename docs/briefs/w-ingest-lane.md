# w-ingest-lane — ingest must not starve behind enrichment

Deferred from plan 005 first light (silkworm, 2026-08-28). NOT yet dispatched.

## The measured property

Conversation-ingest submit is instant (one PG upsert), but the ingest WORK
shares the serial runner pool with provider-bound summarize/enrichment jobs.
First light: a re-poll sat pending behind 500+ summarize jobs created by its
own previous ingest. So THE INDEX LAGS A CONVERSATION BY THE ENRICHMENT
BACKLOG ITS OWN PREVIOUS INGEST CREATED, and the lag grows with conversation
size — a real property for hook-fired ingestion, not a detail.

## Options (Jordan/prime to rule before dispatch)

1. Give INGEST_SESSION its own lane (prime's provisional lean: ingest is
   provider-free and cheap — read/parse/store — while summarize is
   provider-bound; per-conversation serialisation is preserved by the detached
   Postgres advisory lock on the canonical conversation GUID, so alias queue
   keys cannot reorder one conversation).
2. Have ingest enqueue its enrichment at LOWER priority than the next ingest,
   so a busy conversation cannot delay its own next poll.
3. Both (lane split + priority) if the runner machinery makes them one change.

## Constraints for whoever takes it

- Touches shared runner machinery (crates/daemon runner lanes) — small
  collision surface but coordinate with in-flight plans before dispatch.
- Regression proof: a re-poll submitted behind a large enrichment backlog must
  land without waiting for that backlog; enrichment ordering guarantees must
  be stated and tested, not assumed.
