# Phase 1 execution log

## tk-0101 — FILL alignment

Status: in progress.

Evidence before change: `fit_to_cap` uses `FILL=(2,3)` via `budget_bytes`; `chunk_plan` used `window_tokens * BYTES_PER_TOKEN` directly. Ruled corpus: `oversized` 136,000 bytes, `request_whale` 710,000 bytes, production case 20,872 bytes. Measured counts: 7→10, 33→50, 1→2; aggregate 41→62.


Status: complete.

Proof: `cargo test -p fs3-daemon chunk_plan -- --nocapture` passed 6 tests and printed `oversized 7→10, request_whale 33→50, prod_20_872 1→2, total 41→62`.

Implementation: `fs3_core::tokens::input_budget_bytes` now owns the FILL translation used by both `fit_to_cap` and `chunk_plan`; overlap remains 200 estimated tokens because it is semantic context, not a cap.

## tk-0102 — provider cap classification

Status: in progress.

Status: complete.

Proof: the focused provider test was red before adapter classification (Azure typed-error assertion failed), then `cargo test -p fs3-providers cap_rejection` passed 4 tests. Both adapters parse `input[N]`; unrelated 400 responses remain `Error::Provider`.

Discovery: a typed port error required the o-prime-approved `fs3_core::Error::InputTooLong` variant and one `answer.rs` provider-failure arm; adapter-local strings would not satisfy the caller-matchable contract.

## tk-0103 — bounded re-split healing

Status: in progress.


Status: complete.

Proof: `cargo test -p fs3-daemon --test oversize cap_rejection_ -- --nocapture` passed the indexed heal, unnamed-error bisection, and terminal exhaustion cases; the full oversize suite passed 12/12. Removing the typed heal match arm made `cap_rejection_heals_dense_input_into_unique_chunks` fail 0/1 with the original provider error; restoring it returned 3/3 green.

Implementation: provider calls are work-queued as owned text/meta pairs. A named index re-splits that input; an absent or invalid index bisects the rejected call until a singleton is isolated. One tightening changes the effective ratio from two to one byte/token, the provable floor. Exhaustion returns a non-retryable failure with source hash, original bytes, and final ratio. Rows remain unwritten until every call succeeds, then receive contiguous per-source chunk numbers in one store transaction.

## tk-0104 — gate and PR

Status: in progress.


Local gate: `harness checks` returned status `ok`; docs, lock metadata, test-database guard, harness contracts, formatting, clippy with `-D warnings`, and workspace tests passed. Changed-path evidence names only the ruled source, test, plan-progress, execution-log, and enrichment service-page files.

Status: complete.

PR #92 opened. GitHub `gate` passed in 5m14s on head `24e67b881eed5804eb9efd4d59d535ebcd0dbf36` after the local harness gate. The PR body carries the alignment measurement, typed-index versus bisection paths, mutation result, recovery mechanism, and assumptions.

## tk-0105 — production drain

Status: blocked pending o-prime's Jordan-approved duplicate repair and bounce.

Pre-bounce evidence: embed failed=5. Jobs 1316706 and 1323215 share the `043365…` dedupe key; `jobs_live_dedupe_idx` means the automatic multi-row requeue can collide unless o-prime repairs the duplicate first. This pre-existing store defect is outside plan 010's fence; recorded as an AC-0005 precondition rather than changed here.


