# conv-atlas data format (schema_version 1)

**Authorities:**
- Tables: `export/src/conv_atlas/contract.py` (`TABLES`).
- View JSON: `contract/bundle.schema.json`.
- TypeScript mirror: `app/src/contract/types.ts`.

The column lists below are a map, not the authority. A change to any of the three must change all three in the same commit, and the tests validate the synthetic fixture against both layers.

There are two consumers, so the format has two layers. Both come from the same in-memory frames in one export run:

1. **Analysis layer** (`tables/`, `vectors/`): for agents and data scientists. Columnar, full fidelity, keyed by stable ids that round-trip to fs3 (`guid`, `conv:<guid>`, `conv:<guid>#t<n>`). Self-describing via `datapackage.json` and a generated `README.md`.
2. **View layer** (`view/`): for the SPA. Compact JSON with array-index references, shaped for rendering and playback (`bundle.schema.json`).

A third, cross-snapshot layer, `insights/`, is where agents write findings back. The app renders them as annotations on the map, the timeline and playback.

## On disk

```
$CONV_ATLAS_HOME (default ~/.local/share/flowspace3/conv-atlas/)   ← never inside the repo
  bundles/
    manifest.json                       snapshots[] + latest; the app polls this
    <YYYYMMDDTHHMMSSZ>/                  one immutable snapshot
      datapackage.json                  Frictionless Data Package: every table, field, type, unit,
                                        description, primary/foreign key + provenance block
      README.md                         generated data dictionary, caveats, example DuckDB queries
      tables/                           Parquet (zstd)
        conversations.parquet           PK guid · address · kind · label · blurb · repo_key · worktree ·
                                        harness · seat · models[] · parent_guid · started_at · last_turn_at ·
                                        turns · n_work · n_tool_call · n_tool_result · n_retrieval ·
                                        n_human · n_peer · n_agent · tags[] · community_id · x · y · embedded
        turns.parquet                   PK address · guid FK · turn_no · turn_at · role · source · class ·
                                        summary · head · tags[] · raw_hash · bytes · summarized · stopset ·
                                        session_x · session_y
                                        (no raw bodies: `flowspace3 get <address>` fetches them)
        communities.parquet             community_id · label · blurb · tags[] · size · cx · cy
        knn_edges.parquet               a_guid · b_guid · cos_centred · cos_raw · cross_repo · t_first
        subagent_edges.parquet          parent_guid · child_guid · started_at
        seats.parquet                   seat · harness · first_seen · last_seen · n_conversations ·
                                        n_sent · n_received · identity_source
        messages.parquet                one row per peer message: address · sent_at · from_seat · to_seat ·
                                        to_guid · from_guid
        activity_daily.parquet          guid · day · turns · work · tool · human · peer
        tags.parquet / tag_cooc.parquet tag · n · conversations / tag_a · tag_b · n
        session_shifts.parquet          guid · turn_no · turn_at · score · label · tags[]
        session_novelty.parquet         guid · turn_no · novelty
      vectors/
        conversation_centroids.npy      float32 [N×1024], mean of work-turn vectors, NOT centred
        conversation_centroids.index.parquet   row → guid
        embedding_mean.npy              float32 [1024], the corpus mean subtracted for "centred" values
        turn_vectors.f16.npy            OPTIONAL (--turn-vectors): float16 [M×1024] work turns
        turn_vectors.index.parquet      row → address
      view/                             SPA layer, JSON, bundle.schema.json
        meta.json · conversations.json · edges.json · tags.json · sessions/<guid>.json
  insights/                             cross-snapshot, append-only, agent-written
    index.json
    <id>.json                           {id, title, body_md, author, created_ms, snapshot_id, kind?,
                                         refs: {guids[], addresses[], seats[], repos[], tags[], time_range}}
  cache/                                incremental export state (not a deliverable)
```

## Rules

- **The analysis layer has stable keys.** No array indices. Times are ISO-8601 UTC timestamps. Every turn row carries its fs3 address.
- **The view layer is derived from the analysis layer, never the reverse.** A test checks both against each other: same guids, same counts.
- **Provenance travels with each snapshot** in `datapackage.json#/provenance`:
  - database URL host and name only, never credentials
  - snapshot time, git SHA of `tools/conv-atlas`
  - embedder and summariser model keys, coverage stats
  - the boilerplate stop-set and its hit counts
  - centring, UMAP and clustering parameters, and the random seeds
  - the known-defect list (for example DL-029: tool_call text is polluted by the elision bug)
- **Agent entry points** (JSON envelope when piped, like fs3):
  - `conv-atlas describe [table]`: the data dictionary
  - `conv-atlas sql "<duckdb sql>" [--snapshot id]`: tables are pre-registered as views
  - `conv-atlas insight add <file.json>`: validated, then appended to `insights/`; the open app picks it up
- **Analysis tables are versioned like an API.** A schema change bumps `schema_version`, and the app and the tests refuse versions they don't know.
