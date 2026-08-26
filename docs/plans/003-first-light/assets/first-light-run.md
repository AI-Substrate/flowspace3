# First light — live run against Azure OpenAI

**Run**: 2026-08-26, worker pij-broad-sawfish · **Plan**: [003](../plan.dd.md) ac-0004 / dw-0107
**Binary**: `target/release/{fs3-daemon,flowspace3}` at `3a7cc42` · **Root**: `crates/store` (17 files)
**Providers**: Azure OpenAI, Entra (`az login` as `jorkni@microsoft.com`), no API key present
**Result**: 154 elements, 94 summaries, 204 vectors, **0 failed jobs**, ~60 s wall. Three questions asked, three answered correctly. A re-scan of the unchanged root cost **zero** provider calls.

---

## Why this root, and an honest repricing

The plan's original cost ruling said the fixture corpus only, "cents not
dollars". That was later reversed to this repository's root on the basis of
"~133 files — modest cost".

**The reversal was priced on the wrong number.** Cost does not scale with files;
it scales with ELEMENTS at or above `summary_min_lines`. Measured first with the
fake provider precisely so the bill would be a measurement rather than a
surprise:

| Root | Files | Elements | summarize jobs | embed jobs |
|---|---|---|---|---|
| repository root | 187 | 2,261 | **1,082** | 1,311 |
| `crates/store` | 17 | 154 | **94** | 110 |

1,082 chat completions is not cents. o-prime ruled **B** — `crates/store` as the
live root — for two reasons worth recording: it is the best genuine demo corpus
in the repository (real domain prose in the doc comments, so the questions have
right answers a wrong index would miss), and content-addressing makes the choice
strictly non-wasteful. If the whole repository is indexed later, **none of these
94 summaries or 204 vectors is paid for again**: they are keyed by `raw_hash`,
and those hashes do not change for having been reached from a different root.

## Configuration

```toml
[providers.azure-chat]
kind = "azure_openai"
endpoint = "https://oaijodoaustralia.openai.azure.com"
deployment = "gpt-5.6-luna"                    # the DEPLOYMENT, not the model
api_version = "2024-12-01-preview"

[providers.azure-embed]
kind = "azure_openai"
endpoint = "https://oaijodoaustralia.openai.azure.com"
deployment = "text-embedding-3-small-no-rate"
api_version = "2024-02-01"
dimensions = 1024

[embedder]
active = "azure-embed"

[summarizer]
active = "azure-chat"
```

No `api_key_env` on either instance, so both authenticate with Entra. The run
used `env -u AZURE_OPENAI_API_KEY -u AZURE_EMBEDDING_API_KEY` deliberately: a
stale exported key silently beats `az login` and produces a 401 that reads like
a broken Entra setup (`docs/services/azure-openai.md`).

## The run

### 0. Doctor built the store — no manual compose step, no second command

```console
$ flowspace3 doctor
{ "ok": true, "command": "doctor", "v": 1,
  "data": { "steps": [
    { "check": "engine",   "outcome": "ok",       "found": "docker present",                    "elapsed_ms": 15  },
    { "check": "stack",    "outcome": "ok",       "found": "postgres is accepting connections",  "elapsed_ms": 7   },
    { "check": "database", "outcome": "repaired", "found": "fs3_live did not exist",
      "action": "created the database fs3_live",                                                 "elapsed_ms": 90  },
    { "check": "schema",   "outcome": "repaired", "found": "missing migration(s) 0001-0005",
      "action": "applied 0001-0005",                                                             "elapsed_ms": 137 } ],
    "healthy": true },
  "next_action": "the store is ready — start the daemon (`flowspace3 daemon &`) and `flowspace3 add <path>`" }
```

249 ms from "no database at all" to a migrated store. Every row says what was
found AND what was done.

### 1. `add`

```console
$ flowspace3 add crates/store
{ "enqueued": 17, "files": 17, "unchanged": 0, "removed": 0,
  "identity": "git:github.com/AI-Substrate/flowspace3", "identity_source": "remote",
  "root_path": "/Users/.../flowspace3/crates/store",
  "skipped": [ { "count": 1, "reason": "config-format" } ] }
```

400 ms. Two things worth reading:

- The identity is the **repository's**, not the subdirectory's, because
  `repo_identity` walks up. Content indexed from this frame is shared with any
  later whole-repo index rather than forked from it — which is exactly what makes
  option B non-wasteful.
- One file refused with a reason (`Cargo.toml`, a config format, PRD req 43).
  A refused file is reported; a git-ignored file is out of scope and appears in
  neither list.

### 2. The queue drained

| t | scan_file | summarize | embed |
|---|---|---|---|
| +20 s | 17 done | 37 done, 53 pending, 4 running | 10 done, 43 pending |
| +40 s | 17 done | 72 done, 18 pending, 4 running | 13 done, 75 pending |
| +60 s | 17 done | **94 done** | **110 done** |

**~60 s wall, 4 workers, 221 jobs, zero failures and zero retries.** The daemon
log contains no `WARN` and no `ERROR` lines for the entire run.

The shape is the design working: summaries are the slow leg and run four at a
time, while embeds — batched sixteen texts to a call — trail them and then
finish in a burst as the summaries that produce them complete.

### 3. What landed

| Table | Rows |
|---|---|
| `elements` | 154 |
| `smart_content` | 94 |
| `embeddings_1024` (`raw`) | 110 |
| `embeddings_1024` (`smart`) | 94 |
| `worktree_files` | 17 |

**Provider call counts** (every call is a queue row, so these are exact):

- **94 chat completions** — one per distinct `raw_hash` at or above the 10-line
  floor.
- **110 embedding calls** — 16 raw batches plus 94 single-summary calls.

Average tags per summary: **4.00**, inside PRD req 36's 1–5 band on every row.

The `model_key`s are the ones the providers reported, not ones assembled from
config:

```
embeddings_1024.model_key = text-embedding-3-small-no-rate@1024
smart_content.model_key   = gpt-5.6-luna@1
```

Both are the Azure **deployment** name — which is the whole point of taking the
key from `provider.key()`: config cannot tell you what served the request.
The embedder's key carries the vector width, so the key that wrote a vector and
the key that reads it are the same string.

### 4. Three questions

**Q1 — "how does the queue stop two workers taking the same job"**

```
0.578 [smart] tests/pg_store_flows.rs:260-307  a_job_is_claimed_once_and_completing_it_frees_its_key
              tags=[job claiming, deduplication, completion, worker filtering]
0.548 [raw]   tests/pg_store_flows.rs:408-443  two_claimers_never_take_the_same_job
0.524 [smart] tests/pg_store_flows.rs:408-443  two_claimers_never_take_the_same_job
              "…two concurrent workers claiming scan_file jobs receive distinct jobs…"
```

Correct. The two tests that exist to defend `SKIP LOCKED` are ranks 1 and 2, and
the raw and smart vectors of the same element compete on their merits —
`match_field` reports which won.

**Q2 — "why is enrichment keyed by a hash instead of a row id"**

```
0.523 [smart] src/smart.rs:108-138  missing_enrichment
              tags=[database query, missing enrichment, model key, deduplication]
0.522 [smart] src/smart.rs:85-94    MissingEnrichment
              "…stores the raw hash used as the dirtiness key and summary storage key…"
0.470 [raw]   src/smart.rs:108-138  missing_enrichment
```

Correct, and notably so: the question is about a DESIGN DECISION, and the top
hits are the reconciler sweep and the type whose summary names the dirtiness key
in its own words. Nothing in the query string appears literally in the code —
this is the summary layer earning its cost.

**Q3 — "what happens when a vector is the wrong width"**

```
0.576 [raw]   tests/pg_store_flows.rs:571-604  a_vector_of_the_wrong_width_is_refused_by_name
0.476 [raw]   src/lib.rs:178-186               a_width_mismatch_names_the_decision_that_explains_it
0.451 [smart] tests/pg_store_flows.rs:571-604  a_vector_of_the_wrong_width_is_refused_by_name
              "…a 32-dimensional vector is rejected when inserting into or querying the
               1024-dimensional embeddings store…"
```

Correct — both tests that pin `StoreError::Dimensions`.

### 5. Idempotence, live (the most valuable number here)

```console
$ flowspace3 scan crates/store
{ "enqueued": 0, "files": 17, "unchanged": 17, "removed": 0, … }
next_action: "nothing changed — 17 files already indexed;
              `flowspace3 search \"<question>\"` answers from the existing index"
```

After the re-scan:

| | before | after |
|---|---|---|
| `smart_content` | 94 | **94** |
| `embeddings_1024` | 204 | **204** |
| `jobs` (total, all time) | 221 | **221** |
| jobs failed | 0 | **0** |

**Zero jobs enqueued, zero Azure calls, zero cost.** Not a cache — the path→blob
map is identical, so there is nothing to do, by construction.

## What this proves, and what it does not

Proven: the whole path runs against a real provider through the registry;
Entra auth works with key auth disabled on the resource; the deployment-derived
`model_key` reaches the rows; summaries honour the tag band; semantic search
answers design questions, not just keyword questions; a re-scan is free.

Not proven here: within-file ranking at scale (17 files is a small corpus);
behaviour under a rate limit (the `-no-rate` deployment does not impose one, so
the retry path was never exercised live — it is proven in CI instead); the
whole-repository run, deliberately not paid for.

## Reproduce

```bash
cargo build --release -p fs3-cli
export FS3_CONFIG_DIR=$PWD/.harness/temp/sawfish/cfg-live   # config above
env -u AZURE_OPENAI_API_KEY -u AZURE_EMBEDDING_API_KEY flowspace3 doctor
env -u AZURE_OPENAI_API_KEY -u AZURE_EMBEDDING_API_KEY flowspace3 daemon &
flowspace3 add crates/store
flowspace3 status                       # until the queue is empty
flowspace3 search "how does the queue stop two workers taking the same job"
```

Prerequisite: a current `az login` whose identity holds *Cognitive Services
OpenAI User* on the resource.
