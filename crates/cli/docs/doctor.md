# doctor

Diagnosis and repair, in dependency order. Doctor fixes what it can and reports
what it cannot, one row per step.

```bash
flowspace3 doctor
```

## What it walks

| step | checks | repairs |
|---|---|---|
| `engine` | a container engine is on PATH (`FS3_ENGINE`, default `docker`) | no — installing a runtime is not a CLI's job |
| `stack` | Postgres is accepting connections | yes — runs `compose up -d` and WAITS for a real connection |
| `database` | the configured database exists | yes — `CREATE DATABASE` |
| `schema` | the database has every migration this binary carries | yes — applies the missing ones |
| `keyword-index` | with `[search] lexical_backend = "bm25"`: this Postgres has pg_search and the BM25 index is built and valid | no — names the image swap, or `doctor keyword-index` |
| `exact-index` | with `fs3.exact_rank = 'compact'`: every vector has its binary copy (`embeddings_1024_bq`), recorded complete; `warn` when vectors lack one | no — names `doctor exact-index` |
| `daemon` | `GET /health` answers on `daemon.url`; `warn` when it answers but its embedder has been failing (since when, and the last error) | no — see below |
| `auth` | `daemon.key` is mode `0600` and the daemon accepts it | no — restart republishes a missing or stale key |

Every row reports `found` (what it saw) and, when it acted, `action` (what it
did). A row that only says "ok" would be a row you cannot verify.

## The outcome words

| outcome | meaning | degrades the verdict? |
|---|---|---|
| `ok` | already fine | no |
| `repaired` | was broken; doctor fixed it | no |
| `info` | reported for awareness; nothing is wrong | no |
| `warn` | working, but not as it should be — decide something | yes |
| `down` | not running — start something | yes |

The vocabulary is closed, and each word is a promise about what you should do.
`info` exists so a row can be reported without claiming the stack is unhealthy.

## Two fields, two questions

- `data.healthy` — is the STORE usable?
- `data.verdict` — `ok`, or `degraded` when something doctor checked is not up.

They are separate because conflating them is misleading: a machine with a
perfect store and no daemon running is not a working system, and reporting a
plain "ok" there sends you looking in the wrong place.

The envelope stays `ok: true` either way. The COMMAND succeeded; it is the
subject it reports on that may be degraded.

## Why the daemon is reported and never started

Doctor starts the container stack because a container is a background service
you already asked for by configuring it. A daemon is a FOREGROUND process, and
a diagnostic command that spawns one leaves something running that you did not
ask for and cannot see. So the row names the command instead:

```
daemon  down  nothing is listening on http://127.0.0.1:7373
              not started — run `flowspace3 daemon &`
```

## When a step cannot be repaired

Doctor stops there and returns an error envelope, because every later step
depends on it — probing a schema on a server that is not running produces a
second, less useful copy of the same failure. The steps that DID pass ride
along in `meta`, so a failed run still tells you how far it got.

## `install-skill`

`flowspace3 doctor install-skill` installs or updates the bundled agent skill
into `~/.agents/skills` and `~/.claude/skills`, explicitly: the walk never
installs, and nothing writes those files silently or by force.

The walk's last row (`skills`) reports where installed copies stand — current,
stale, or missing — naming the install command when they do not. It is an
`info` row: it asks without degrading the verdict, and doctor never installs.

## `keyword-index`

`flowspace3 doctor keyword-index` builds the BM25 keyword index that
`[search] lexical_backend = "bm25"` searches with: `CREATE EXTENSION pg_search`,
then the index, `CONCURRENTLY`, beside the trigram index, so the daemon keeps
indexing and searching while it builds. Run again, it changes nothing; after an
interrupted build it drops the invalid index and builds it again. On a Postgres
without pg_search it answers `unavailable` and names the switch to the ParadeDB
image (`docs/how/database.md`). Restart the daemon afterwards: it looks for the
index at boot.

## `exact-index`

`flowspace3 doctor exact-index` fills the binary copy of every vector
(`binary_quantize(vector)`, 128 bytes, table `embeddings_1024_bq`) that a scope
ranked exactly can be ranked on: the copies are compared first, and the nearest
are re-ranked by the full vectors (`fs3.exact_rank = 'compact'`, see
`flowspace3 docs get search`). A trigger writes the copy of every vector stored
after migration 0033, so the command only fills the ones from before it, in
batches that each commit on their own; it is safe beside a running daemon and an
interrupted run resumes. It then refreshes the table's visibility map (so the
copies are read from the index alone) and records the copy complete, which is
what lets search use it. Run again, it only re-checks. It reads every vector
once: a heavy job on a big index, so pick a quiet moment.

The report gives `state` (`built`, `ready` or `incomplete`), `filled`, `bytes`,
the vectors per embedding model, and the database's `exact_rank`. To rank on
it: `ALTER DATABASE … SET fs3.exact_rank = 'compact'`, then restart the daemon
(pooled connections keep the setting they opened with).
