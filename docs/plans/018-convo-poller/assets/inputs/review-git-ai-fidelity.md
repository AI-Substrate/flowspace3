# Can git-ai be flowspace3's one conversation source for every harness?

**Asked:** "I thought we were going to get all conversation history from git-ai
for any harness because it does the heavy lifting of standardising for us. Look
at the data it collects and report on whether it is high enough fidelity.
Compare our own session between what flowspace3 collects today and what git-ai
has. If git-ai has enough, we have one impl for all harnesses, today and future."

**Verdict:** git-ai is **high fidelity when it captures and unreliable about
whether it captures** — and it does **not** standardise. The payload is each
harness's raw native record; git-ai unifies the envelope (repo, tool, session,
parent, timestamp) and the transport (one sqlite), not the dialect. So it
collapses *transport* to one implementation and leaves one *parser per
harness* — which is the part that actually costs. Measured on this machine,
2026-09-06 00:15–00:48Z, git-ai 1.6.21, fs3 at `3ebfd58`.

---

## 1. Fidelity when it works — my own session, git-ai vs the native file

Session `a5a5588f` (this o-prime seat). git-ai's copy vs
`~/.claude/projects/…/a5a5588f….jsonl`, over the window git-ai covers
(≤ 2026-09-04 22:14:53):

| measure | native | git-ai |
|---|---|---|
| distinct assistant `message.id` | 6,002 | 6,001 |
| distinct record `uuid` | 27,638 | 27,636 |
| `tool_use` / `tool_result` blocks | 5,015 / 5,015 | 5,015 / 5,015 |
| `text` blocks / `thinking` blocks | 2,221 / 3,687 | 2,221 / 3,687 |
| largest tool_result (one 55 KB `Read`) | 55,881 chars | 55,871 chars |
| subagent sidecars, parent-linked | 15 | 15 (all match by id) |
| record types (mode, attachment, file-history-snapshot, …) | all 16 | all 16, same counts |

It is a byte-faithful copy of the native jsonl records, wrapped: `v.0` IS the
Claude line. The two missing uuids are the `thinking` + `tool_use` written at
22:14:51–53 — the records after the last `PostToolUse` that fired. That is
structural (§3), not noise.

**fs3 today**, by contrast, reads the native file directly and holds this
session at whatever turn count the last manual ingest reached. Both are copies
of the same bytes; the difference is who moves the cursor.

## 2. What git-ai stores per harness — raw, not standardised

`event_kind=5` rows, `v.0` payload, sampled newest-first:

| tool | rows in 11k sample | `v.0` is | content depth |
|---|---|---|---|
| claude | 6,512 | the native jsonl line | full: text, thinking, tool_use, tool_result, subagents |
| github-copilot-cli | 4,876 | copilot's event (`data`, `id`, `parentId`, `type`) | full: `user.message`, `assistant.message`, tool exec, `model.messages_snapshot` with system/user/assistant |
| pi (omp) | 12 | omp's record (`id`, `message`, `parentId`) | full — **but stops 2026-09-02** (§4) |
| codex | 17 | codex `payload` | tool calls; not checked for user text |
| cursor | 15 | `{"status":"success","type":"turn_ended"}` | **none** — markers only, 563 bytes/row |

Our `metrics_db.rs` already carries two dialect parsers (claude shape at
`:592-741`, copilot shape at `:765-908`). Adding omp/codex/cursor to it would
be exactly the work of adding native readers — the dialect is the same either
way; only the file vs sqlite framing differs, and `tail.rs` already owns the
file framing once.

## 3. Reliability — where git-ai silently stops

This is the disqualifier for "sole source", not fidelity.

**a. My session stalled with no error.** `tracked_streams` row for `a5a5588f`:
`watermark = 84,661,106 = last_known_size`, `last_processed 2026-09-04
22:14:53`, `processing_errors 0`, `last_error NULL`. File now 85,971,723 bytes.
9.92 days after the first record. Nothing in three daemon logs names the
session. Firing the hook by hand (inline and `--hook-input stdin`, then with
`GIT_AI_TRANSCRIPT_STREAMING_LOOKBACK_DAYS=60`) produced kind-4 file
attribution rows and moved the transcript watermark by zero bytes. Not a size
cap: streams of 194 MB, 160 MB and 139 MB are current. Leading suspect is the
daemon-side `transcript_streaming_lookback_days` (a `git-ai config set` +
`git-ai bg restart` change — not made; that is your box's daemon).

**b. A whole session never captured.** `974459cf` (novels, 2026-09-05
23:37–23:39, 13 `tool_use` blocks = 13 hook firings): **0 rows** in git-ai.
Its sibling `f74fbceb` in the same directory is current to within minutes.

**c. The sweep never visits `~/.claude/projects`.** 202 sweep items in the two
newest logs hit `~/.claude-alt`, 19 hit `~/.copilot`, none hit `~/.claude`. So
every session you and the fleet run under the default config dir depends
entirely on the `PostToolUse` hook path — the one that failed in (a) and (b).

**d. The last turn of every session is structurally absent.** Forwarding
happens on `PostToolUse`; the final assistant message after the last tool
call has no hook after it. A session that ends in prose loses its conclusion;
a session with no tool calls at all is never captured.

**e. omp is dead in git-ai.** Last `pi` row 2026-09-02; zero rows for
grim-gurgeh's live 12 MB omp session; zero omp sessions in four days. The
native omp store has all of it.

None of a–e is visible from git-ai's side without opening `transcripts-db`
and comparing watermark to file size by hand.

## 4. fs3's own metrics-db route is broken independently (row 193)

`convo_ingest.rs:538` opens `~/.git-ai/metrics.sqlite3`; the real file is
`~/.git-ai/internal/metrics-db`. Every metrics-db ingest is accepted then
fails. Copilot, whose only store this is, has never been ingested here.

## 5. Recommendation

**Do not make git-ai the sole source. Use it as a second source and a
cross-check.**

1. **Primary = native stores, polled by fs3.** One poller over
   `~/.claude/projects`, `~/.omp/agent/sessions`, and the copilot store, keyed
   on files, with cursor-vs-size visible in `doctor`. This is the thing fs3
   controls; it is the only path that cannot stall because someone else's
   daemon config changed.
2. **git-ai metrics-db = the copilot store (fix row 193) and a reconciliation
   input.** Where both hold a session, `verify` can report the delta
   (git-ai turns vs fs3 turns) — that is the check that would have caught 3a
   in a day instead of after a question.
3. **The "one impl" you want is the normaliser, and it already exists.**
   `conversation_normalize.rs` + the `ConversationSource` seam is the single
   shape; the readers are dialect adapters. A new harness costs one adapter
   whether its bytes arrive from a file or from git-ai's sqlite. git-ai does
   not remove that adapter; it moves it.

If git-ai grows a stable normalised turn schema and a liveness guarantee
(watermark == size, or an explicit stall error), revisit. Today it has
neither, and this review's own evidence — a 10-day-old prime session silently
dropped, a two-minute session never picked up — is the case.

## Not verified

- Root cause of 3a. Suspect `transcript_streaming_lookback_days`; needs a
  daemon config change to prove.
- Codex rows for user-prompt text (only tool payloads sampled).
- Whether `.claude-alt` vs `.claude` selection is git-ai config or an artefact
  of which session was open at daemon start.
