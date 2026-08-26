-- Migration 0003 - the ref layer: repos, worktrees, and path -> blob.
--
-- Workshop 002 ("PG schema: content layer, ref layer, job backlog"). The ref
-- layer is deliberately cheap: pointers only. Nothing expensive hangs off it,
-- which is what makes removing a worktree a safe, local delete (decision D8 -
-- a worktree going away must never cascade into re-payable LLM spend).
--
-- The content layer (0004) is keyed by blob, not by worktree, so forty
-- branches holding the same file share one parse and one enrichment. This
-- table is the only place that knows which live path currently holds it.

CREATE TABLE repos (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- PRD req 35: the git remote URL when there is one. A folder with no
    -- remote falls back to a path-derived id, which is why this is TEXT and
    -- not a parsed URL.
    identity    TEXT        NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE worktrees (
    id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    repo_id   BIGINT NOT NULL REFERENCES repos(id),
    -- Absolute host path. Not repo-relative: this is where the machine can
    -- actually find the checkout.
    root_path TEXT   NOT NULL,
    -- Branch or ref when known; NULL for a detached or non-git folder.
    ref_name  TEXT,
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (repo_id, root_path)
);

CREATE TABLE worktree_files (
    worktree_id BIGINT      NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
    -- Relative to the worktree's root_path.
    path        TEXT        NOT NULL,
    -- Git blob id. Untracked files are hashed identically (PRD req 23), so an
    -- unstaged file is addressable on the same terms as a committed one.
    blob_sha    TEXT        NOT NULL,
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (worktree_id, path)
);

-- Search resolves a content hit back to the live paths that hold it, so the
-- reverse lookup (blob -> where is it right now) is the hot direction.
CREATE INDEX worktree_files_blob_sha_idx ON worktree_files (blob_sha);
