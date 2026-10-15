-- Migration 0032 - a cache of each search scope's sources, kept exact by
-- change stamps.
--
-- A scoped vector search starts by collecting every source its scope admits:
-- the scope's file blobs and their elements, its conversations' turns, and
-- the summaries of both. On prod (2026-10-15) that took 0.36 s for a 13k
-- source code scope, 0.48 s for a 13k all-kinds scope and 2.5 s cold for a
-- 220k one, on every pass of every search, for a set that changes only when
-- the index does. `scope_sources` keeps it per scope; a search reads it and
-- adds what grew since it was built.
--
-- Growth is told by `changed_xid`: the transaction that wrote the row, set by
-- trigger so that every writer is covered, including ones not written yet. A
-- cached scope reads every row stamped by a transaction at or after the xmin
-- of the snapshot it was built from - every transaction that snapshot could
-- not see - so nothing the build missed is missed. That is MVCC's own
-- visibility rule, with no clock in it. The stamps fire only when a column
-- scope membership depends on changes: the watcher rewrites every
-- `worktree_files` row's `last_seen` on each sync, and that stamps nothing.
--
-- Shrinkage (a deleted file, a re-pointed path, a moved conversation) is
-- counted per repository in `scope_shrinks`, in sources. A cached scope is a
-- superset of the true one, every candidate is re-checked against live
-- provenance at admission, and `fs3_store::scope_cache` rebuilds a key before
-- the sources that left it reach a bound, or after ten minutes.
--
-- Existing rows keep a NULL stamp, which reads as "before any cache". The
-- column adds rewrite nothing; each partial index starts empty, though its
-- build reads the table once. Never wait long for a lock: a boot behind a
-- pg_dump or an open psql fails with this error instead of hanging.

SET LOCAL lock_timeout = '5s';

ALTER TABLE elements ADD COLUMN changed_xid xid8;
ALTER TABLE worktree_files ADD COLUMN changed_xid xid8;
ALTER TABLE smart_content ADD COLUMN changed_xid xid8;
ALTER TABLE conversations ADD COLUMN changed_xid xid8;

CREATE INDEX elements_changed_xid_idx
    ON elements (changed_xid) WHERE changed_xid IS NOT NULL;
CREATE INDEX worktree_files_changed_xid_idx
    ON worktree_files (changed_xid) WHERE changed_xid IS NOT NULL;
CREATE INDEX smart_content_changed_xid_idx
    ON smart_content (changed_xid) WHERE changed_xid IS NOT NULL;

-- `pg_current_xact_id()` assigns the transaction its xid if it has none yet,
-- so even a transaction's first write is stamped with the xid that decides
-- its visibility.
CREATE FUNCTION fs3_stamp_changed() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    NEW.changed_xid := pg_current_xact_id();
    RETURN NEW;
END $$;

-- An element: a new file blob's parse, a reparse, a turn's element, or one
-- whose kind, hash or blob is rewritten in place.
CREATE TRIGGER elements_stamp_insert BEFORE INSERT ON elements
    FOR EACH ROW EXECUTE FUNCTION fs3_stamp_changed();
CREATE TRIGGER elements_stamp_update BEFORE UPDATE OF kind, raw_hash, blob_sha ON elements
    FOR EACH ROW
    WHEN (OLD.kind IS DISTINCT FROM NEW.kind
          OR OLD.raw_hash IS DISTINCT FROM NEW.raw_hash
          OR OLD.blob_sha IS DISTINCT FROM NEW.blob_sha)
    EXECUTE FUNCTION fs3_stamp_changed();

-- A path added, or pointed at another blob (a branch switch onto content the
-- index already holds). Not the sync's `last_seen` rewrite.
CREATE TRIGGER worktree_files_stamp_insert BEFORE INSERT ON worktree_files
    FOR EACH ROW EXECUTE FUNCTION fs3_stamp_changed();
CREATE TRIGGER worktree_files_stamp_blob BEFORE UPDATE OF blob_sha ON worktree_files
    FOR EACH ROW WHEN (OLD.blob_sha IS DISTINCT FROM NEW.blob_sha)
    EXECUTE FUNCTION fs3_stamp_changed();

-- A summary that arrives after its source, or is rewritten.
CREATE TRIGGER smart_content_stamp_insert BEFORE INSERT ON smart_content
    FOR EACH ROW EXECUTE FUNCTION fs3_stamp_changed();
CREATE TRIGGER smart_content_stamp_text BEFORE UPDATE OF text_hash ON smart_content
    FOR EACH ROW WHEN (OLD.text_hash IS DISTINCT FROM NEW.text_hash)
    EXECUTE FUNCTION fs3_stamp_changed();

-- A conversation that is new, or that gains or changes its repository or
-- worktree later (a re-post, boot normalisation): all of its turns join
-- whichever scope it now belongs to.
CREATE TRIGGER conversations_stamp_insert BEFORE INSERT ON conversations
    FOR EACH ROW EXECUTE FUNCTION fs3_stamp_changed();
CREATE TRIGGER conversations_stamp_anchor BEFORE UPDATE OF repo_identity, worktree ON conversations
    FOR EACH ROW
    WHEN (OLD.repo_identity IS DISTINCT FROM NEW.repo_identity
          OR OLD.worktree IS DISTINCT FROM NEW.worktree)
    EXECUTE FUNCTION fs3_stamp_changed();

-- What has left each repository's scopes, in sources (a raw hash or one of
-- its summaries), logged per repository identity ('' for conversations with
-- none, '*' for departures no repository can be named for) by the removing
-- transaction. Counts are upper bounds - a raw hash and every summary it has,
-- one per summarising model - so a cached scope never carries more departed
-- sources than the entries since its cutoff say.
--
-- An append-only log, not a counter per repository: a counter row would be
-- locked from one departure to its transaction's commit, serialising every
-- writer to that repository (a watcher sync is one long transaction) or, for
-- '*', every reparse. `fs3_store::scope_cache` sums the entries since a key's
-- cutoff and prunes those every key has passed.
CREATE UNLOGGED TABLE scope_shrinks (
    identity text NOT NULL,
    removed  bigint NOT NULL,
    xid      xid8 NOT NULL DEFAULT pg_current_xact_id()
);
CREATE INDEX scope_shrinks_identity_xid_idx ON scope_shrinks (identity, xid);

CREATE FUNCTION fs3_count_shrink(departed_identity text, departed bigint) RETURNS void
    LANGUAGE sql AS $$
    INSERT INTO scope_shrinks (identity, removed)
    SELECT departed_identity, departed WHERE departed > 0
$$;

-- The sources one raw hash stands for: itself and each of its summaries.
CREATE FUNCTION fs3_raw_sources(raw text) RETURNS bigint
    LANGUAGE sql STABLE AS $$
    SELECT 1 + count(*) FROM smart_content WHERE raw_hash = raw
$$;

-- The sources a blob stands for: each element's raw hash and its summaries.
CREATE FUNCTION fs3_blob_sources(blob text) RETURNS bigint
    LANGUAGE sql STABLE AS $$
    SELECT COALESCE(sum(fs3_raw_sources(raw_hash)), 0)::bigint
      FROM elements WHERE blob_sha = blob
$$;

-- A turn appended (a deduped one included: its element already existed) or
-- restated: its conversation is stamped, once per statement. A restated or
-- deleted turn takes its old blob (its raw hash and summaries) out of its
-- repository's scopes.
CREATE FUNCTION fs3_turns_changed() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE conversations c SET changed_xid = pg_current_xact_id()
         WHERE c.guid IN (SELECT DISTINCT conversation_id FROM added);
    ELSIF TG_OP = 'UPDATE' THEN
        UPDATE conversations c SET changed_xid = pg_current_xact_id()
         WHERE c.guid IN (
               SELECT n.conversation_id
                 FROM added n
                 JOIN removed o ON o.conversation_id = n.conversation_id
                               AND o.turn_no = n.turn_no
                WHERE o.blob_sha IS DISTINCT FROM n.blob_sha);
        PERFORM fs3_count_shrink(departed.identity, departed.sources)
           FROM (SELECT COALESCE(c.repo_identity, '') AS identity,
                        sum(fs3_raw_sources(o.blob_sha))::bigint AS sources
                   FROM added n
                   JOIN removed o ON o.conversation_id = n.conversation_id
                                 AND o.turn_no = n.turn_no
                   JOIN conversations c ON c.guid = n.conversation_id
                  WHERE o.blob_sha IS DISTINCT FROM n.blob_sha
                  GROUP BY 1) departed;
    ELSE
        PERFORM fs3_count_shrink(departed.identity, departed.sources)
           FROM (SELECT COALESCE(c.repo_identity, '') AS identity,
                        sum(fs3_raw_sources(o.blob_sha))::bigint AS sources
                   FROM removed o
                   LEFT JOIN conversations c ON c.guid = o.conversation_id
                  GROUP BY 1) departed;
    END IF;
    RETURN NULL;
END $$;

CREATE TRIGGER turns_changed_insert AFTER INSERT ON turns
    REFERENCING NEW TABLE AS added
    FOR EACH STATEMENT EXECUTE FUNCTION fs3_turns_changed();
CREATE TRIGGER turns_changed_update AFTER UPDATE ON turns
    REFERENCING OLD TABLE AS removed NEW TABLE AS added
    FOR EACH STATEMENT EXECUTE FUNCTION fs3_turns_changed();
CREATE TRIGGER turns_changed_delete AFTER DELETE ON turns
    REFERENCING OLD TABLE AS removed
    FOR EACH STATEMENT EXECUTE FUNCTION fs3_turns_changed();

-- The update-side counts are ROW triggers guarded by the columns that matter,
-- so they cost nothing when nothing leaves: a statement trigger with
-- transition tables would copy every row of every update - all 175k paths of
-- each watcher sync, every element of a turn-class backfill.

-- A path re-pointed away from its old blob, which takes that blob's
-- sources out of the repository's scopes.
CREATE FUNCTION fs3_path_shrunk() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    PERFORM fs3_count_shrink(
        COALESCE((SELECT r.identity FROM worktrees w JOIN repos r ON r.id = w.repo_id
                   WHERE w.id = OLD.worktree_id), '*'),
        fs3_blob_sources(OLD.blob_sha));
    RETURN NULL;
END $$;

CREATE TRIGGER worktree_files_shrunk_blob AFTER UPDATE OF blob_sha ON worktree_files
    FOR EACH ROW WHEN (OLD.blob_sha IS DISTINCT FROM NEW.blob_sha)
    EXECUTE FUNCTION fs3_path_shrunk();

-- Paths removed: a deleted file, or a worktree's scan that no longer sees it.
CREATE FUNCTION fs3_paths_removed() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    PERFORM fs3_count_shrink(departed.identity, departed.sources)
       FROM (SELECT COALESCE(r.identity, '*') AS identity,
                    sum(fs3_blob_sources(o.blob_sha))::bigint AS sources
               FROM removed o
               LEFT JOIN worktrees w ON w.id = o.worktree_id
               LEFT JOIN repos r ON r.id = w.repo_id
              GROUP BY 1) departed;
    RETURN NULL;
END $$;

CREATE TRIGGER worktree_files_shrunk_delete AFTER DELETE ON worktree_files
    REFERENCING OLD TABLE AS removed
    FOR EACH STATEMENT EXECUTE FUNCTION fs3_paths_removed();

-- A conversation moved off its repository or worktree: all of its turns
-- leave its old repository's scopes. (A deleted conversation's turns are
-- counted as they are deleted.)
CREATE FUNCTION fs3_conversation_moved() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    PERFORM fs3_count_shrink(
        COALESCE(OLD.repo_identity, ''),
        (SELECT COALESCE(sum(fs3_raw_sources(t.blob_sha)), 0)::bigint
           FROM turns t WHERE t.conversation_id = OLD.guid));
    RETURN NULL;
END $$;

CREATE TRIGGER conversations_shrunk_anchor AFTER UPDATE OF repo_identity, worktree ON conversations
    FOR EACH ROW
    WHEN (OLD.repo_identity IS DISTINCT FROM NEW.repo_identity
          OR OLD.worktree IS DISTINCT FROM NEW.worktree)
    EXECUTE FUNCTION fs3_conversation_moved();

-- Departures no repository can be named for: an element rewritten in place
-- (its old kind or hash leaves; two sources), or a summary rewritten (its
-- old text leaves; one).
CREATE FUNCTION fs3_global_shrunk() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'elements' THEN
        PERFORM fs3_count_shrink('*', fs3_raw_sources(OLD.raw_hash));
    ELSE
        PERFORM fs3_count_shrink('*', 1);
    END IF;
    RETURN NULL;
END $$;

CREATE TRIGGER elements_shrunk_update AFTER UPDATE OF kind, raw_hash, blob_sha ON elements
    FOR EACH ROW
    WHEN (OLD.kind IS DISTINCT FROM NEW.kind
          OR OLD.raw_hash IS DISTINCT FROM NEW.raw_hash
          OR OLD.blob_sha IS DISTINCT FROM NEW.blob_sha)
    EXECUTE FUNCTION fs3_global_shrunk();
CREATE TRIGGER smart_content_shrunk_text AFTER UPDATE OF text_hash ON smart_content
    FOR EACH ROW WHEN (OLD.text_hash IS DISTINCT FROM NEW.text_hash)
    EXECUTE FUNCTION fs3_global_shrunk();

-- A reparse: a blob gains rows under a parser version it did not have, and
-- search answers one version per blob (the current one when the blob has
-- it), so the rows of its other versions may leave every scope holding it.
-- A ROW trigger, counted once per blob and version: by the first row of that
-- version to arrive (a tree is written one element per statement, and
-- counting on every row would count each superseded row once per new one).
-- Every other insert costs one probe of elements_blob_parser_idx.
CREATE FUNCTION fs3_reparse_shrunk() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM elements same
                    WHERE same.blob_sha = NEW.blob_sha
                      AND same.parser_version = NEW.parser_version
                      AND same.id <> NEW.id) THEN
        PERFORM fs3_count_shrink('*', COALESCE(sum(fs3_raw_sources(superseded.raw_hash)), 0)::bigint)
           FROM elements superseded
          WHERE superseded.blob_sha = NEW.blob_sha
            AND superseded.parser_version <> NEW.parser_version;
    END IF;
    RETURN NULL;
END $$;

CREATE TRIGGER elements_shrunk_reparse AFTER INSERT ON elements
    FOR EACH ROW EXECUTE FUNCTION fs3_reparse_shrunk();

-- The cache. UNLOGGED: no WAL, and emptied after a crash (with the shrink
-- counts), which only means the next search of each scope rebuilds it.
CREATE UNLOGGED TABLE scope_sources (
    scope_key   text NOT NULL,
    source_kind text NOT NULL,
    source_hash text NOT NULL,
    PRIMARY KEY (scope_key, source_kind, source_hash)
);

CREATE UNLOGGED TABLE scope_cache (
    scope_key text PRIMARY KEY,
    -- The xmin of the snapshot it was built from: rows stamped by this
    -- transaction or a later one are read on top of the cached set.
    cutoff    xid8 NOT NULL,
    built_at  timestamptz NOT NULL,
    sources   bigint NOT NULL,
    -- The repositories whose departures make it stale (`scope_shrinks`), with
    -- '*' always among them; NULL watches every repository.
    watch     text[]
);
