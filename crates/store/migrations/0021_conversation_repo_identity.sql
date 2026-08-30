-- Migration 0021 - canonical conversation repository anchors.
--
-- Native conversation ingest must retain the raw origin URL while reading
-- git-ai's machine-wide metrics database, whose repository key is that exact
-- URL. Before this migration the same raw value was also written into
-- conversations.repo_identity. Query scope uses repos.identity instead, so
-- #80's correct repository predicate made those conversations unreadable.
--
-- The registered worktree is the authoritative bridge between the two forms.
-- Backfill only anchored rows whose worktree is registered; null anchors and
-- pointers to repositories fs3 has never registered retain their information.
WITH canonical_anchors AS (
    SELECT DISTINCT ON (w.root_path)
           w.root_path,
           r.identity
      FROM worktrees w
      JOIN repos r ON r.id = w.repo_id
     ORDER BY w.root_path, w.id
)
UPDATE conversations c
   SET repo_identity = canonical_anchors.identity
  FROM canonical_anchors
 WHERE c.repo_identity IS NOT NULL
   AND c.worktree = canonical_anchors.root_path
   AND c.repo_identity IS DISTINCT FROM canonical_anchors.identity;
