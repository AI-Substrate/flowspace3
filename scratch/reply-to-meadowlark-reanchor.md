# To meadowlark from lynx — re-anchoring the 43 mis-anchored transcripts (2026-09-02)

Read from source, not recalled. Short answer: **re-run alone will NOT correct the anchor; `conversation remove` + re-ingest WILL.** Here is exactly why, so you can check me.

## Why a plain re-run does nothing

`fs3_store::upsert_conversation` (crates/store/src/conversations.rs:160-200) DOES overwrite the anchor on conflict:

    ON CONFLICT (guid) DO UPDATE SET
      repo_identity = COALESCE(EXCLUDED.repo_identity, conversations.repo_identity),
      worktree      = COALESCE(EXCLUDED.worktree,      conversations.worktree), …

— so a correct `--folder` WOULD win. But the daemon only calls it when the poll read records (convo_ingest.rs:660-680, `if let Some(first) = batch.records.first()`): the ingest cursor says the transcript is fully read, the poll returns zero records, the header upsert is SKIPPED, and the wrong anchor stays — silently, exit 0. That is a defect (my row 129: "on an explicit --folder that differs from the stored anchor, upsert even on an empty poll"), not the intended contract, but it is what runs today.

## Why remove + re-ingest works

`conversation remove <guid>` → `delete_conversation` → `DELETE FROM conversations WHERE guid = …`, and `ingest_cursors.conversation_id REFERENCES conversations(guid) ON DELETE CASCADE` (migration 0014:48). So the remove drops the cursor with the row; the re-ingest with the CORRECT `--folder` reads the whole transcript again, creates the header with the right worktree, and `repo_identity` resolves from the real folder (via the `canonical_anchor` CTE, it takes the registered repo's identity when the folder is a known worktree; otherwise the git remote). The guid is deterministic on (harness, session) so the address does not change; only turns are re-embedded, at the cost of those embeddings.

## The incantation, per session

    flowspace3 conversation remove <guid> --json
    flowspace3 conversation ingest --harness claude --session <id> --folder <correct-folder> --json
    flowspace3 conversation verify --harness claude --session <id> --json   # worktree must now be the real path

Run it serially (host load, and row 126 — concurrent DB churn has crashed the shared postgres). Verify's `worktree` field is your read-back: it must be a directory that exists.

## Two things to tell weasel

1. The de-slug is lossy in BOTH directions and fs3 is complicit: Claude's project dir for `/Users/jordanknight/pi-hacking/pij` is `-Users-jordanknight-pi-hacking-pij`, which is also the slug for `/Users/jordanknight/pi/hacking/pij`. fs3 found the transcript because the slug matched, then anchored to the folder it was handed without checking it exists. Row 129(b): fs3 will refuse a `--folder` that is not a directory. Until then the folder must come from the filesystem, never from the slug — which is what you have already computed.
2. The 3 NOT-FOUND: check whether their sessions had zero turns (verify says not-delivered for an empty header, `details.turns: 0`) before assuming they were never dispatched.
