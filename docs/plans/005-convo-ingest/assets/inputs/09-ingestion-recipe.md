# Conversation ingestion recipe — where turns live, how to address, join, and tail them

For the fs3 conversations import surface (req-0024..0027; harness extension v1).
From pij-squealing-xoxarle, 2026-08-28. Everything below verified on this
machine; payload recommendation (what to store per turn) is the companion doc
`scratch/conversations-telemetry-sample.md`.

## 0. Input contract → resolution

Ingestion input is either `(pij_id, folder)` or `(native_session_id, harness,
folder)`. Resolution rule: a pij id resolves to `(native_session_id, harness)`
via the join in §2; from there the HARNESS decides the store and the FOLDER
picks the workspace-sluged directory. Slug = absolute path with `/` → `-`
(both claude and omp use this convention, e.g.
`-Users-jordanknight-substrate-flowspace-flowspace3`).

## 1. The stores, one conversation each

### a) Claude Code native (PREFERRED for claude — the source git-ai mirrors)
```
~/.claude/projects/<cwd-slug>/<session-uuid>.jsonl        # the conversation
~/.claude/projects/<cwd-slug>/<session-uuid>/             # sidecar dir
    subagents/agent-*.jsonl + agent-*.meta.json           # child conversations
    tool-results/                                          # large tool outputs spilled to files
    custom-title.json
```
One json object per line. Record `type`: `user` / `assistant` (message with
content blocks: text, thinking, tool_use / tool_result) plus bookkeeping rows
(`mode`, `permission-mode`, `file-history-snapshot`, `attachment`,
`queue-operation`, …) that ingestion should skip by allowlist, not blocklist
churn. Every content row carries `sessionId`, `uuid`, `parentUuid`, ISO
`timestamp`. GOTCHA: an assistant message appears as ONE LINE PER CONTENT
BLOCK sharing `message.id` — dedupe/merge by `message.id` (usage identical
across duplicates; verified 0 divergent).

### b) omp / pi native
```
~/.omp/agent/sessions/<cwd-slug>/<startTs>_<session-uuid>.jsonl   # conversation
~/.omp/agent/sessions/<cwd-slug>/<startTs>_<session-uuid>/NN.bash.log  # raw bash outputs
```
Record types: `session` (header: id, cwd, title) · `model_change` ·
`thinking_level_change` · `message` (roles: user / assistant / toolResult —
toolResult carries `toolCallId` + `toolName` so pairing is free) · `custom` /
`custom_message` · `title`/`title_change`. Assistant usage is per message
(input/output/cacheRead/cacheWrite/totalTokens + duration/ttft/stopReason).
GOTCHA: in-process pij tools are encoded as `write` toolCalls with
`arguments.path = "xd://pij_send"` — remap name to the xd suffix or every
survey miscounts writes and loses sends.

### c) pij seat ledger (the agent-to-agent view, keyed by SEAT not uuid)
```
~/.pij/<pij-id>/events.ndjson
```
Events: `{seq, timestamp, type: message|tool_call|tool_result|receipt, data}` —
a session mirror PLUS delivery `receipt`s (`to`, state, messageId) that exist
nowhere else. Use when the conversation wanted is "what did this seat say to
whom", or as the fallback mirror for a harness we cannot read natively.

### d) git-ai metrics-db (fallback/cross-check; machine-wide sqlite)
```
~/.git-ai/internal/metrics-db  · table metrics · event_kind=5
```
`external_session_id` = native uuid; `event_json` = `{"t":epoch,"e":5,
"v":{"0":<native record>},"a":{attrs}}` — for claude it is a mirror of (a);
copilot sessions exist ONLY here (event-stream dialect; event name under
`type` NOT `name`; model nested at `data.modelCall.model`). Scope by repo
(`event_json like '%<folder-name>%'`) or you ingest other projects.

## 2. The join: pij id → native session id → files

1. `pij sessions` — columns pij-id · harness · harness-session-uuid. The uuid
   SHAPE routes: v4 → claude/copilot; v7 `01a0…` → omp.
2. Files: claude → `<projects>/<slug>/<uuid>.jsonl`; omp → glob
   `<sessions>/<slug>/*_<uuid>.jsonl` (filename carries a start-ts prefix).
3. Fallbacks when the registry lacks the seat: worker-roster.md records native
   ids at canary time; last resort is time-window + cwd correlation.
4. Reverse (uuid → pij id): grep `pij sessions`; ledger receipts confirm.

## 3. Incremental reading — cursors that survive re-polling

All three file stores are APPEND-ONLY jsonl → **byte offset is the cursor**.

- Recipe: remember `(file_path, inode/device, byte_offset)`. On poll: stat; if
  size > offset, read from offset, parse whole lines only (keep a partial-line
  tail buffer — a writer can be mid-line at read time); advance offset past
  complete lines. If inode changed or size < offset, treat as rotation: re-scan
  from 0 with dedupe on (uuid | id | seq).
- Per-store natural ordinals to STORE with each ingested turn (for dedupe and
  citation): claude `uuid` (+ `parentUuid` chain); omp record `id` +
  `timestamp`; pij ledger `seq` (monotonic int — the cleanest cursor of all);
  metrics-db `rowid` (query `where rowid > :cursor and external_session_id=…
  order by rowid` — event_ts is seconds-grain and non-unique, rowid is safe).
- Timestamps are NOT safe cursors anywhere (second-grain collisions in
  metrics-db; equal ISO stamps within a burst in omp).
- New sidecar files (claude subagents/) appear mid-session: incremental
  ingestion must re-glob the sidecar dir each poll, each new file starting at
  offset 0 and linked to the parent via its `.meta.json` / dir placement.

## 4. What reconvo.py already solves (reuse, don't rewrite)

`scratch/reconstruct/scripts/reconvo.py`:
- Store READERS for metrics-db (both dialects), omp jsonl, pij ledger —
  including the xd:// remap, claude per-block dedupe, tool_use↔tool_result
  pairing, and copilot's type/name + modelCall quirks. These reader functions
  are the ingestion parser, minus the cursor plumbing (it reads whole files).
- Turn NORMALIZATION to one shape: `{ts, actor, kind: human|pij_in|pij_out|
  report_card|assistant|tool_call, text, ref}` — the kinds map cleanly onto
  the payload spec in conversations-telemetry-sample.md.
- Multi-source merge on timestamp + YAML selection (kind/regex/window/limit).
Also reusable: `token-ledger.py` usage extraction per dialect (if turns carry
cost) and `convo-sample.py` classification heuristics (evidence vs
re-derivable). What none of them have: incremental cursors, inode handling,
sidecar re-globbing — that is the new work.

## 5. Gotchas (each one bit us; details in 01-telemetry-experience-log.md)

1. Claude one-line-per-content-block (merge by message.id) — else dup turns
   and ~2× token counts.
2. omp xd:// virtual writes (pij_send etc.) — name-based queries miss them.
3. Copilot: event name under `type`; per-call model at data.modelCall.model;
   seat-level model labels lie (the "gemini" PA called gpt-5.4-nano).
4. pij send bodies can be head-TRUNCATED on delivery (omp→busy-claude defect);
   the ledger + the SENDER's transcript hold the full body — prefer sender side.
5. Compaction: claude compaction lands IN-SESSION as a summary user turn
   ("This session is being continued from a previous conversation…") — same
   file, same uuid; treat as a `system` turn kind, not human. omp compaction
   (pij compact-self) similarly appears as injected instruction turns. Do NOT
   drop them: they are the only marker that context was rebuilt (a turn-count
   discontinuity follows).
6. Multi-file: one claude SESSION = main jsonl + N subagent jsonl; ingest as
   separate conversations linked by parent (metrics-db
   external_parent_session_id, or the sidecar dir), or the subagents' work is
   invisible.
7. Live files: lynx's session grew 808k→904k output tokens between two of our
   surveys the same day — always stamp ingestion time; never assume a session
   is finished (closed-revivable seats can resume and APPEND to the same file).
8. Machine-wide stores (metrics-db): repo-scope every query.
9. Large tool outputs in claude may spill to the sidecar `tool-results/` dir —
   the inline record then references rather than contains; resolve the file if
   the payload rule wants heads of everything.
10. Retention: native claude/omp files persist; git-ai bash db is 30 days —
    ingestion should not depend on it.
