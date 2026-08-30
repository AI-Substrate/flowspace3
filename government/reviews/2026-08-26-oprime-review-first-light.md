# O-prime review — first-light pipeline (e0788da, 31464e8, 3a7cc42)
**Reviewer**: pij-instant-lynx (lead) + 1 independent critic · 2026-08-26 · tests re-run green by critic

## Verdict: APPROVE the happy path (it's excellent); 3 fault-path fixes + 1 test-honesty fix before plan close

Clean bill on the hard stuff: search fully bind-parameterized (repo/path/limit/score/source all `$n`, LIKE metachars escaped), schema guard on all four db-touching routes, doctor's pool access contained to exactly doctor.rs, `earns_raw_vector` principled ("covered by children", pinned structurally), 10 of 11 e2e tests genuinely behavioral against real PG/git/router.

| # | Sev | Finding | Smallest fix |
|---|---|---|---|
| 1 | HIGH | Worker death mid-job wedges the row `running` FOREVER — no lease, no boot sweep, runner dies un-awaited on ctrl-c; and because scan dedupe keys on PATH, the wedged row absorbs every future add/scan of that file (`ON CONFLICT` bumps payload, never state). One SIGKILL during a big index = files permanently unindexable without manual SQL. | At daemon boot, before spawning the runner: `UPDATE jobs SET state='pending' WHERE state='running'` — safe, we are the single writer. |
| 2 | HIGH | The content-addressed SKIP paths swallow downstream enqueues on retry: summarize skip returns Ok without enqueueing the smart vector; scan skip returns early without enqueue_for_tree. And `missing_enrichment` (the D6 reconciler) has ZERO callers and covers only the summary half. Any transient fault in the write→enqueue window = elements/summaries silently invisible to search. | Skips RE-EMIT their downstream work (dedupe makes it free): summarize's Some(summary) path still enqueues the smart embed; scan's Some(tree) path runs enqueue_for_tree over the stored tree. |
| 3 | MED | Embed-batch dedupe keys on the FIRST item's hash and `ON CONFLICT` REPLACES payload — two different batches sharing a first element (branches diverging in the tail; edited file with unchanged head) collide and the displaced batch's items are never embedded, silently. | Key = content_hash over ALL item hashes (sorted, concatenated). |
| 4 | LOW | `two_repos_on_different_instances_write_different_model_keys` proves fallback, not divergence — the per-repo resolution regression would pass the suite. | Wire a second fake instance and assert the keys differ, or rename to what it proves. |

## Also noted (daemon-plan, not blocking)
A PERIODIC reconciler (missing_enrichment + an embeddings-side twin) still has no caller — finding 2's fix removes the acute need; the sweep itself joins the daemon plan's queued decisions.
