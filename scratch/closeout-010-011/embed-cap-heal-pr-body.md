## Summary

- align `chunk_plan` with the same two-thirds FILL byte budget as `fit_to_cap`
- classify only hosted embedding-cap 400s as typed `Error::InputTooLong`
- re-split the rejected input at the one-byte/token floor; bisect calls when no valid `input[N]` is present
- fail terminally with source hash, original bytes, and final ratio if the provider still rejects
- keep all vectors in memory until one atomic store write, with contiguous per-source chunk numbers
- update the enrichment service contract and recovery instructions

## Alignment measurement

Ruled corpus, before → after FILL alignment:

- `oversized` (136,000 bytes): **7 → 10**
- `request_whale` (710,000 bytes): **33 → 50**
- production-sized item (20,872 bytes): **1 → 2**
- aggregate: **41 → 62**

The 20,872-byte production item splits by alignment alone.

## Adapter paths

Both OpenAI and Azure use the shared hosted-embedding classifier. An exact 400 containing `maximum input length is 8192 tokens` becomes `Error::InputTooLong`. If the body contains `input[N]`, that index is carried to the daemon and only that member is re-split. If the index is absent or invalid, the daemon bisects the rejected call until the offending singleton is isolated. Other 400s remain `Error::Provider`.

## Mutation check

With the `Error::InputTooLong` match arm removed from `embed_items`, `cap_rejection_heals_dense_input_into_unique_chunks` failed **0/1** with the original provider cap error. Restoring the arm returned the focused heal set to **3/3 green**.

## Verification

- `cargo test -p fs3-providers cap_rejection` — 4 passed
- `cargo test -p fs3-daemon chunk_plan -- --nocapture` — 6 passed; printed the measurements above
- `cargo test -p fs3-daemon --test oversize` — 12 passed
- `ddocs validate docs/plans/010-embed-cap-heal/plan.dd.json` — 0 errors, 0 warnings
- `ddocs validate docs/plans/010-embed-cap-heal/assets/tasks/phase-1/tasks.dd.json` — 0 errors, 0 warnings
- `harness checks` — status `ok`; docs, lock, test DB, contract, fmt, clippy, and test gates passed

## Production drain mechanism

The deploy bounce invokes `boot::recover_enrichment_jobs` (`crates/daemon/src/boot.rs:176-194`), which calls `fs3_store::requeue_failed` for non-terminal summarize/embed jobs. The query (`crates/store/src/jobs.rs:506-534`) resets state to pending, attempts and parks to zero, schedules immediately, and skips live duplicate keys. No repair SQL is required.

## Assumptions

- Hosted OpenAI/Azure embedding deployments retain the declared 8192-token cap and the measured rejection phrase.
- A token cannot consume fewer than one input byte; one byte/token is therefore the tightest useful ratio.
- Provider calls return vectors in the adapter-normalized input order.
- `put_embeddings` remains the single atomic write for the complete source chunk set.
- O-prime owns the production bounce and joins the read-only before/after status, five-key state, and recovered-conversation search receipts.
