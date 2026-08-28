# convo — ingesting the conversations agents already had

`harness convo <sub>` — every sub shells through to `flowspace3 conversation …`
(the CLI is the single implementation; this verb is the discoverable surface).

| sub | what it does |
| --- | --- |
| `ingest` | submit one native session for ingest and return immediately |
| `list` | list indexed conversations, newest first |

## What ingest actually does

**It submits; it does not read.** The route validates the address, upserts ONE
queued job, and returns. The daemon's runner does the reading, parsing, shaping
and storing. That shape is deliberate: ingest is fired from HOOKS, which run
often, and a hook that waits on a store read is a hook nobody keeps.

Two consequences worth knowing before you script it:

- **Repeat firings are free while a job is pending.** The queue key is the
  address, and the enqueue upserts among live jobs, so twenty firings during
  one pending job produce one job — the same collapse the file watcher relies
  on.
- **A firing with nothing new is a cheap no-op.** Reading is incremental
  against a durable cursor, so the second ingest of a session costs only the
  turns that appeared since the first. It is not a re-read.

## Addressing a conversation

Two routes, and they land the same conversation because both reduce to
`(harness, session id)`:

```bash
harness convo ingest -- --pij pij-appalling-slug        # by seat
harness convo ingest -- --session <uuid> --harness omp  # by native session id
```

Arguments after `--` are forwarded VERBATIM to `flowspace3 conversation
ingest`. That is deliberate: restating the flags here would mint a second place
where `--session` needs `--harness`, and two declarations of one rule is one
declaration that goes stale.

`--session` REQUIRES `--harness`, and that is not bureaucracy: claude and
copilot session ids are both v4 uuids and they live in different stores, so the
id alone does not say what to open. The seat route needs no `--harness` because
`pij sessions` already knows.

`--folder` defaults to where you are standing, which is almost always the
workspace the conversation was about. The seat route falls back again to the
git directory pij recorded for that seat.

## The four stores

| `--harness` | what it reads |
| --- | --- |
| `claude` | Claude Code session jsonl, plus subagent sidecars as linked child conversations |
| `omp` | omp/pi session jsonl |
| `pij` | the pij seat ledger, `events.ndjson` — the only store that records delivery state |
| `metrics-db` | git-ai's machine-wide sqlite metrics, scoped to one repository |

`metrics-db` is machine-wide and holds every project on the box, so an ingest
from it is scoped to the repository's remote URL and REFUSES rather than falling
back when a folder has no remote. There is no safe unscoped read of that store.

## Reading the result

`ingest` returns the standard envelope with the address it queued and the key it
collapses on. The work shows up in `flowspace3 status` as it drains, and the
turns are searchable with `flowspace3 search "<question>" --source conversation`.

When you look at a completed ingest, **read `deduped`**. "read 412, appended 0,
deduped 412" means a rotation was handled — the session file rolled over, the
reader restarted from the beginning, and the ordinal ledger recognised every
record it had already stored. Without that number a handled rotation and an idle
poll look identical, and so does a silently duplicated conversation.

## What is expected back from you

- If ingest is slow enough to notice from a hook, that is a defect, not a
  tuning problem — the synchronous path is one Postgres statement by
  construction. Capture it with `harness observe` and say what you measured.
- If a conversation appears with turns MISSING rather than wrong, that is the
  failure mode the structural expectations cannot see. Say so loudly; it is the
  one class of reader bug the fixtures were never able to rule out.
