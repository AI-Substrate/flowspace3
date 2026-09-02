AND EXISTS (
     SELECT 1
       FROM elements admitted
      WHERE (
            (e.source_kind = 'raw' AND admitted.raw_hash = e.source_hash)
            OR (e.source_kind = 'smart' AND EXISTS (
                 SELECT 1
                   FROM smart_content candidate
                  WHERE candidate.text_hash = e.source_hash
                    AND candidate.raw_hash = admitted.raw_hash)))
        AND ($8::text[] IS NULL OR admitted.kind = ANY($8))
        AND ($10::text[] IS NULL
             OR admitted.ddoc->>'id_kind' = ANY($10))
        AND ($11::boolean IS NULL
             OR (CASE
                   WHEN jsonb_typeof(admitted.ddoc->'derived_state') = 'object'
                   THEN (admitted.ddoc->'derived_state'->>'complete')::boolean
                   ELSE (admitted.ddoc->>'gate_terminal')::boolean
                 END IS NOT NULL
                 AND CASE
                   WHEN jsonb_typeof(admitted.ddoc->'derived_state') = 'object'
                   THEN (admitted.ddoc->'derived_state'->>'complete')::boolean
                   ELSE (admitted.ddoc->>'gate_terminal')::boolean
                 END = NOT $11))
        AND ($12::text IS NULL
             OR admitted.ddoc->>'schema' = $12)
        AND ($13::text IS NULL
             OR strpos(admitted.address, 'conv:' || $13 || '#t') = 1)
        AND ($6::text IS NULL AND $7::text IS NULL AND $9::text IS NULL
             OR EXISTS (
                  SELECT 1
                    FROM worktree_files f
                    JOIN worktrees w ON w.id = f.worktree_id
                    JOIN repos r     ON r.id = w.repo_id
                   WHERE f.blob_sha = admitted.blob_sha
                     AND ($6::text IS NULL OR r.identity = $6)
                     AND ($7::text IS NULL OR f.path LIKE $7)
                     AND ($9::text IS NULL OR w.root_path = $9))
             OR EXISTS (
                  SELECT 1
                    FROM turns t
                    JOIN conversations c ON c.guid = t.conversation_id
                   WHERE t.blob_sha = admitted.blob_sha
                     AND ($6::text IS NULL OR c.repo_identity = $6)
                     AND ($7::text IS NULL OR c.worktree LIKE $7)
                     AND ($9::text IS NULL OR c.worktree IS NULL OR c.worktree = $9))))
