- id: DL-001
  kind: difficulty
  description: "No command creates a migrated scratch database. To build a hand-made fixture for a review probe I had to apply crates/store/migrations/*.sql by hand; plain 'psql -f' fails at 0020_one_file_root_per_blob.sql because its CREATE TEMP TABLE ... ON COMMIT DROP needs the whole file in ONE transaction (sqlx does this, psql autocommit does not). Discovering the '-1' flag was guesswork."
  severity: degrading
  workaround: "docker exec ... psql -1 -f - < each migration file, in sort order"
  suggested_encoding: "harness db scratch <name> — create + migrate a throwaway database on FS3_TEST_DATABASE_URL and print its URL; harness db drop <name> to reap it"
  fp: a9b6e96fd71a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T05:26:47.004Z"
- id: CONF-001
  kind: confusion
  description: "ddocs validate's builder/review vocabulary is only discoverable by failing. 'resolution: open' was rejected with 'value \"open\" is not in confirmed, refuted, fixed, deferred' — correct, but there is no way to read that enum up front, and none of the four words means 'not yet measurable', which is the honest state of an acceptance criterion whose environment has not been bounced yet. I used 'deferred'."
  severity: annoying
  workaround: "ran ddocs validate --json and read the enum out of the error message"
  suggested_encoding: "ddocs schema builder/review should print field enums; consider a 'not-yet-measurable' resolution distinct from 'deferred'"
  fp: 23a7d63ed55a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T05:26:47.156Z"
- id: DL-002
  kind: difficulty
  description: "Third agent today mis-reported the read tool's '[Some lines truncated to 768 chars]' footer as the FILE being truncated, and spent review budget on it before retracting (o-prime, 2026-09-02: 014 reviewer, then me on 013, then a third). Backlog row 152 already records the phenomenon, but recording it has not stopped recurrence: each agent independently reads a long ddoc cell, sees it cut mid-sentence, and reasonably concludes the document or 'ddocs build' is lossy. The cost is real — I nearly refused the packet under i1b because the owed lists looked absent, and I raised it as an intake defect in my ack."
  severity: degrading
  workaround: "Read the .dd.json source instead of the rendered .dd.md, and confirm with awk '{print length}' <file> | sort -n | tail -1"
  suggested_encoding: "Make the footer name itself as a VIEWER limit, not a file property — e.g. 'display truncated to 768 chars/line by this tool; the file is intact, re-read with :raw or check with awk'. Recurrence three times in one day means the wording, not the docs, is the defect."
  fp: 039a04941d4a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T05:30:19.289Z"
