-- Migration 0020 - one parsed file tree per (blob, parser), including races.
--
-- The content layer shares parses by blob, while element addresses include the
-- path that first produced the parse. Two files with identical bytes therefore
-- used disjoint address keys and concurrent scans could insert two complete
-- trees. Keep the first root deterministically, remove every later tree, wake
-- the scan jobs that failed on the corrupt shape, then make recurrence
-- impossible at the database boundary.

CREATE TEMP TABLE duplicate_file_roots ON COMMIT DROP AS
SELECT id AS root_id,
       blob_sha,
       parser_version,
       row_number() OVER (
           PARTITION BY blob_sha, parser_version
           ORDER BY id
       ) AS survivor_rank
  FROM elements
 WHERE parent_id IS NULL
   AND kind = 'file';

DELETE FROM elements
 WHERE id IN (
       SELECT root_id
         FROM duplicate_file_roots
        WHERE survivor_rank > 1
 );

UPDATE jobs
   SET state = 'pending',
       attempts = 0,
       parks = 0,
       not_before = now(),
       last_error = 'requeued by migration 0020 after duplicate element roots were repaired',
       updated_at = now()
 WHERE kind = 'scan_file'
   AND state = 'failed'
   AND NOT terminal
   AND payload->>'blob' IN (
       SELECT DISTINCT blob_sha
         FROM duplicate_file_roots
        WHERE survivor_rank > 1
   )
   AND last_error LIKE '%file roots, expected exactly one%'
   AND NOT EXISTS (
       SELECT 1
         FROM jobs live
        WHERE live.dedupe_key = jobs.dedupe_key
          AND live.state IN ('pending', 'running')
   );

CREATE UNIQUE INDEX elements_one_file_root_per_blob_parser_idx
    ON elements (blob_sha, parser_version)
    WHERE parent_id IS NULL AND kind = 'file';
