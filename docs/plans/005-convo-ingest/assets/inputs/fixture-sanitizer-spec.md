# Golden fixture harvest + sanitizer spec (tk-c103)

Authority: PM (pij-linguistic-narwhal), under prime ruling B/C of 2026-08-28.
Binding for every fixture committed under `crates/testkit/fixtures/conversations/`.

The fixtures ARE the frozen contract (tenet 2): every reader unit and the shared
contract suite prove themselves against these bytes and nothing else. Shape
fidelity therefore outranks tidiness — when in doubt, keep the record exactly as
the store wrote it.

## 1. Layout

```
crates/testkit/fixtures/conversations/
  claude/
    <session-uuid>.jsonl                     # main conversation
    <session-uuid>/subagents/agent-*.jsonl   # child conversations
    <session-uuid>/subagents/agent-*.meta.json
    <session-uuid>/tool-results/*            # spilled large outputs (>=1)
    PROVENANCE.md
  omp/
    <startTs>_<session-uuid>.jsonl
    PROVENANCE.md
  pij/
    events.ndjson
    PROVENANCE.md
  metrics_db/
    metrics.sqlite3                          # table `metrics`, event_kind=5
    PROVENANCE.md
```

One store per directory, no shared files: this is the same collision-surface
rule the units follow (tenet 3).

## 2. Coverage each fixture MUST contain

Chosen so the recipe's ten gotchas are provable from committed bytes alone.

| store | required content |
|---|---|
| claude | an assistant message split across MULTIPLE LINES sharing one `message.id` (gotcha 1) · at least one `tool_use` + matching `tool_result` · at least two bookkeeping record types that ingestion must skip by allowlist (`mode`, `file-history-snapshot`, `attachment`, `queue-operation`, …) · a compaction/summary user turn ("This session is being continued from a previous conversation…", gotcha 5) · >=1 subagent sidecar with its `.meta.json` (gotcha 6) · >=1 record referencing a spilled `tool-results/` file, with that file present (gotcha 9) |
| omp | a `session` header record · `message` records for roles user / assistant / toolResult with `toolCallId` + `toolName` pairing · at least one `write` toolCall whose `arguments.path` starts `xd://` (gotcha 2) · a `model_change` or `thinking_level_change` record · a compaction / injected-instruction turn (gotcha 5) · assistant usage fields present on >=1 record |
| pij | `message`, `tool_call`, `tool_result` AND `receipt` events (receipts exist nowhere else) · strictly monotonic `seq` · >=1 receipt in a non-delivered state if one exists in the source |
| metrics_db | `event_kind=5` rows in BOTH dialects: the claude mirror AND a copilot event-stream row (event name under `type`, model at `data.modelCall.model`, gotcha 3) · >=2 distinct `external_session_id` values · >=1 row whose `event_json` names a DIFFERENT repo, so repo-scoping (gotcha 8) is provable by a negative |

Size budget: <= 200 records per store, <= 512 KB per directory. Fixtures are
read on every test run; a fixture nobody can eyeball is a fixture nobody
maintains.

## 3. Sanitisation rules (prime ruling B, 2026-08-28)

Applied in this order, to every harvested byte:

1. **Shape is preserved verbatim.** Never drop, reorder, rename or re-indent a
   field. Never re-serialise a record through a pretty-printer: one JSON object
   per line, exactly as the store wrote it. Byte offsets are the product's
   cursor — reformatting invalidates the fixture as evidence.
2. **Home path rewrite.** `/Users/jordanknight` -> `/Users/agent`, and the
   derived cwd slug `-Users-jordanknight-` -> `-Users-agent-`, everywhere
   including inside embedded JSON strings and file names.
3. **Body cap.** Any single string value longer than 2048 bytes is cut to 2048
   bytes ON A CHARACTER BOUNDARY and suffixed `…[fixture-truncated]`. (Cutting
   mid-character is exactly the bug `docs/services/conversations.md` records;
   a fixture must not reproduce it accidentally.)
4. **Credential scrub (mandatory).** Grep every fixture before committing for
   `key=`, `api_key`, `apikey`, `token`, `bearer`, `sk-`, `ghp_`, `gho_`,
   `github_pat_`, `password`, `secret`, `Authorization`, `AWS_`, `-----BEGIN`.
   Every hit is replaced with `REDACTED` (keep the surrounding key/field so the
   record shape survives). Record the hit count in PROVENANCE.md — including
   zero, so "not checked" and "checked, clean" are distinguishable.
5. **Project scope.** Content from projects other than flowspace3 is excluded,
   with ONE deliberate exception: the metrics_db fixture keeps a foreign-repo
   row (see coverage table) whose payload is reduced to the minimum that proves
   scoping — no prose from that project.
6. **Identifiers stay real.** Session uuids, record uuids/parentUuid chains,
   ledger `seq`, sqlite `rowid`, timestamps and `message.id` are NOT secrets and
   are NOT rewritten: dedupe, ordering and cursor behaviour are what we are
   proving. Human names of fleet seats stay (they are public in this repo).

## 4. PROVENANCE.md, one per store directory

Required fields, no prose beyond them:

```
source        <absolute path the sample came from, with the home rewrite applied>
harvested     <ISO date> by <who>
records       <n> lines / rows  (<n> kept of <n> in source)
covers        <gotcha numbers and coverage-table items this fixture proves>
sanitised     home-path rewrite: <n> · body caps: <n> · credential redactions: <n>
notes         <anything a reader must know to trust these bytes>
```

## 5. What harvesting must NOT do

- No Rust. Fixture harvest produces DATA only; the trait, the types and the
  contract suite are the PM's (ruling C).
- No edits outside the fixture directory being harvested — not `lib.rs`, not
  `Cargo.toml`, not another store's directory.
- No formatter, no linter, no `harness checks`, no `cargo` anything.
- No synthesised records. Every byte descends from a real store on this machine;
  if the coverage table demands a record the real sample lacks, harvest a
  DIFFERENT real session that has it and say so in PROVENANCE.md. A hand-written
  record is a fixture that proves our imagination, not the store.
