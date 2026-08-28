# convo-ingest — reading the conversations agents already had

Plan 005, unit u4: the composition root that turns four native session stores
into indexed, searchable conversations. The readers, the durable cursor and the
pure normaliser are units of their own; this page is about the pipeline that
joins them and the surface an operator meets.

Related: `convo-source-claude.md`, `convo-source-omp.md`,
`convo-source-metricsdb.md`, `convo-cursors.md`.

## The surface

```bash
flowspace3 conversation ingest --pij <seat>                       # by fleet seat
flowspace3 conversation ingest --session <id> --harness <store>   # by native id
harness convo ingest -- --pij <seat>                              # same, discoverable
```

`--session` requires `--harness`, and that is not bureaucracy: claude and
copilot session ids are **both v4 uuids** and live in different stores, so the
id alone does not say what to open.

## It submits; it does not read

The route validates the address, upserts ONE queued job, and returns. The
daemon's runner does the reading. Ingest is fired from HOOKS, which run often,
and a hook that waits on a store read is a hook nobody keeps.

Measured on the first-light run: **10–40 ms** for a submit, dominated by process
start. Nothing in that path scales with conversation size or with how much has
already been ingested — one Postgres statement, no filesystem, no subprocess.
The `pij sessions --json` join, measured at 0.5–0.9 s, is on the worker side of
that boundary precisely because it would otherwise have made hook-fired ingest
untenable.

Repeat firings collapse: the queue key is the address plus folder, and the
enqueue upserts among LIVE jobs, so a burst produces one job. Measured: five
rapid submits, one pending row.

## The pipeline, per session file

1. **Look up** the conversation for `(harness, session_id)`. Never a mint that
   guesses — see below.
2. **Load the cursor.** Absent means read from the beginning.
3. **Read**, blocking, off the async thread, exactly as the local ONNX embedder
   is handled.
4. **Upsert the header** — before the ledger is asked anything, because
   `ingest_cursors.conversation_id` is a real foreign key.
5. **Ask the ledger** about exactly the ordinals just read.
6. **Decide purely**: `prepare_batch` dedupes the rescan, numbers the rest from
   the conversation's own high-water mark, and applies the payload policy.
7. **Append**, idempotent on `(conversation_id, turn_no)`.
8. **Record the poll** — ledger rows and cursor in ONE transaction, even when
   nothing was appended, because the reader still moved over bytes.

## Decisions worth knowing

**A conversation id is DERIVED, not minted.** `sha256("fs3-convo-v1:<harness>/<session_id>")`
laid out as a v8 uuid. Two consequences, both load-bearing: the seat route and
the native route land the SAME conversation because both reduce to the same
pair (proven in first light — see below), and forgetting a cursor makes a
re-ingest a re-read rather than a second copy of the conversation. `commit_poll`
refuses to rebind a session that already points elsewhere, which is the backstop
if this derivation is ever changed.

**`started_at` is the first record's timestamp, not the clock.** A conversation
began when its first turn did; an ingest-time stamp would make the same
conversation start at a different moment depending on when someone ran this.
When a poll reads nothing there is deliberately no fallback stamp and the upsert
is skipped — an epoch default is a date nobody chose, absorbed silently, which
is the defect shape this wave found four separate times.

**Ingest is SERIAL.** `ingest_session` is claimed in `SERIAL_KINDS`, not the
batched lane, because turn numbers come from the conversation's own stored turns
and two polls of one conversation must not interleave. The queue enforces the
per-conversation serialisation structurally rather than by anyone remembering to.

**An accounting anomaly is an error, not a statistic.** Every prepared turn is
either accepted or already stored; any other outcome means the ledger and the
turns table disagree about what exists, and the ingest fails loudly rather than
reporting success over a conversation that is missing turns.

**Read `deduped`.** "read 412, appended 0, deduped 412" means a rotation was
handled. Without that number a handled rotation, an idle poll and a silently
duplicated conversation all look identical.

## First light — the measured transcript

Run 2026-08-28 against a scratch database, ingesting **this PM seat's own live
session** by pij id.

| claim | evidence |
| --- | --- |
| submit returns immediately | 0.056 s first call; 0.01–0.04 s over five no-new-data calls |
| burst collapses | five rapid submits → **one** pending `ingest_session` row |
| turns land (ac-0001) | `pij-pale-silkworm` → 739 turns, `started_at` `2026-08-28T01:21:15Z`, matching the session's own first record |
| a turn is readable | `conv:bebaf916…#t150` returns real body text with its real timestamp |
| searchable by meaning | `search "the ordinal is the ledger dedupe key and a changed grouping rule doubles the conversation" --source conversation` → `conv:bebaf916…#t160` at 0.61 |
| re-poll appends ONLY the delta (ac-0003) | 739 → 752 turns on the second poll; the file held 755 eligible records by then, the extra 3 written after the poll |
| both routes are one conversation (ac-0002) | `--session … --harness omp` landed in `bebaf916…`, still **one** conversation, 757 turns |

### Two things first light found that no fixture could

**The `--folder` default is wrong for a worktree-resident seat — FOUND AND
FIXED IN-PLAN.** The first run failed with `holds no session ending
_01a045f4….jsonl`, because it looked under `-substrate-flowspace-flowspace3`
while the session lives under `-substrate-flowspace-fs3-convo-ingest`. pij
records a seat's `gitCommonDir`, which is the MAIN CLONE even when the seat
works in a worktree, and omp slugs by the actual working directory. Every seat
of this fleet is worktree-resident, so the default was wrong more often than
right.

The resolver now asks the STORE rather than the slug. When the folder-derived
directory misses, it scans the store's slug directories for the session id —
globally unique — and reads the working directory the session itself recorded:
omp on its `session` header, claude on its content rows. It deliberately does
NOT un-slug a directory name: a slug joins components with `-`, so
`-substrate-flowspace-fs3-convo-ingest` is indistinguishable from three nested
directories and inverting it would guess. The folder actually used comes back in
the envelope.

Proven live with the exact invocation that failed: the same
`--folder <main clone>` call now completes, lands in the SAME conversation
rather than a second one, and the conversation's `worktree` reads
`/Users/jordanknight/substrate/flowspace/fs3-convo-ingest` — the discovered
directory, not the one that was passed.

**Ingest can starve behind enrichment.** The re-poll job sat `pending` while 500+
`summarize` jobs drained, and landed only once they had. Submitting is instant;
the WORK is queued behind whatever else the runner is doing, and enrichment of a
739-turn conversation is a lot of else. For hook-fired ingest that is a real
latency property, not a detail: the index lags the conversation by the length of
the enrichment backlog the previous ingest created.

## Known limits

- **Dedupe does not cover transcript-imported turns.** They carry no ordinal, so
  tailing a session that was previously imported appends beside them rather than
  recognising them. Import a transcript or tail the session, not both.
- **`metrics-db` refuses a folder with no git remote.** It is machine-wide, so
  an unscoped read is a data leak rather than a convenience, and a directory
  name is not a scope.
- **The remote must be spelled as git-ai stamped it.** Scope is equality on the
  raw remote URL; a transport-spelling mismatch surfaces as "session not found"
  rather than as an empty result.
