# STOP-AND-ASK 007 — AC-0006 premise disproven

The global-scope search returns conversation results, but none is demonstrably from failed job 1344012. The job is not a conversation-specific job:

- `enrich.rs:485-496` defines `RECOVERY_IDENTITY = "conv:recovery"` only because `conv:` cannot shadow a real repository identity; `requeue_missing_vectors` batches missing vectors across all content and explicitly cannot know originating repos.
- Job 1344012 payload has six hashes. Five resolve only to document `section` elements (`docs/plans/112-verb-usage/report.md` and `packet-rust-ci.md`). The sixth is the empty hash `e3b0…`; it resolves to empty containers/turns and has no searchable phrase.
- The phrase `pij verb usage ranking` comes from document section hash `c74f…`, not a turn element.
- `flowspace3 search --source conversation --repo all 'Stores overlap, so values are never summed or averaged' --json` returns semantic hits from unrelated conversations; citing one would be a false-positive receipt.

AC-0005 is proven: 4 repaired/requeued rows are done attempts=1; duplicate job 1316706 is terminal with `duplicate-of:1323215`; after status has one named failed embed residue.

Please rule AC-0006 `na` / amend it to a document-search proof, or identify a specific non-empty turn hash/address from job 1344012 that the database evidence above missed. I will not mark an unrelated semantic hit as recovered content.
