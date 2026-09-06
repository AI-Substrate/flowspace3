# Plan 013+015 prod receipt — 2026-09-02T06:01:39Z
## Before bounce (pg_stat_statements for the OLD search statement)
143|10464
(pg_stat_statements reset)
## Bounce
daemon-restart: ERROR: stop all but one candidate and retry
SUMMARY pane=- old_pid=- new_pid=- binary=/Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3 health=not-checked
healthy after 10s
## Row 147: re-ingest ~/pi-hacking/pij, then wait for TS non-file elements
 "summaries": 0,
      "total": 3550
    },
    "repo_removed": false,
    "root_path": "/Users/jordanknight/pi-hacking/pij",
    "was_registered": true
  },
  "next_action": "removed — 3550 row(s) are now reclaimable; garbage collection runs on its own schedule, or `flowspace3 gc` does it now"
}

: 101,
        "reason": "generated-sibling"
      },
      {
        "count": 41,
        "reason": "unsupported-extension"
      }
    ],
    "unchanged": 1839,
    "worktree_id": 262
  },
  "next_action": "101 scan jobs queued — poll `flowspace3 status` until the queue is empty, then search"
}


## RETAKE after the manual bounce (new pid 79538, 2026-09-02T06:05:09Z)
healthy at +60s
## re-ingest ~/pi-hacking/pij
{"files": 1940, "identity": "git:github.com/AI-Substrate/pij", "jobs_killed": 0, "reclaimable": {"elements": 409, "embeddings": 0, "jobs": 0, "summaries": 0, "total": 409}, "repo_removed": false, "roo
enqueued 1943 unchanged 0 files 1943 worktree 263
TS non-file elements @3: 258 after 260s
function|503
container|138
file|93
## Search receipts (load: load averages: 31.02 28.67 27.25)
search [where does the pij extension register the seat at boot] wall 9.969369000 s → 3 ['5-ts-grammar/assets/backpressure.dd.json:row', ':turn', ':turn']
search [where does the daemon detect new git worktrees appearing and register them] wall 6.925951000 s → 3 [':turn', ':turn', ':turn']
search [how is retry handled for embedding jobs] wall 3.638038000 s → 3 [':turn', ':turn', ':turn']
search [what owns the watcher debounce] wall 15.324145000 s → 3 [':turn', ':turn', ':turn']
## After: pg_stat_statements for the NEW statement
4|8143|WITH candidate_vectors AS MATERIALIZED (
                 SE
## status timings
real 1.00
real 1.20
real 0.73

## CLEAN RECEIPT (bash) — 2026-09-02T06:19:51Z
queue open jobs: 0 at +30s
### TS elements @3 by kind (pij worktree 263)
function|553
container|145
file|100
### TS blobs parsed under @3: 100 of 100
### Search receipts (load: load averages: 13.29 17.16 21.86)
search [where does the pij extension register the seat at boot] wall 9.257092000 s → 5 ['rammar/assets/backpressure.dd.json:row:bp-0006', ':turn:t11100', ':turn:t10748', ':turn:t8674', 'conversation_sources/pij_ledger.rs:function:seat']
search [how does the pij extension send a message to the daemon] wall 5.811147000 s → 5 [':turn:t8749', 'l/assets/inputs/pij-two-daemons.md:section:The shape', ':turn:t10509', ':turn:t7598', 'e/assets/inputs/pij-two-daemons.md:file:pij-two-daemons.md']
search [where does the daemon detect new git worktrees appearing and register them] wall 5.868550000 s → 5 [':turn:t9954', ':turn:t10338', ':turn:t10329', ':turn:t10339', ':turn:t9533']
search [what owns the watcher debounce] wall 11.849461000 s → 5 [':turn:t10329', ':turn:t10858', ':turn:t10339', 'AGENTS.md:section:Dogfood the product ', ':turn:t10844']
### pg_stat_statements (search statement since the 06:02 reset)
8|7940
### status wall
real 0.27
real 0.30
real 0.33

## Probe — why only 100 TS blobs / no .ts search hits — 06:20:55
worktree 263 files: 1960 ; TS paths: 100 ; distinct TS blobs: 100
worktree rows for pij identity: 263|/Users/jordanknight/pi-hacking/pij 190|/Users/jordanknight/pi-hacking/pij-worktrees/s111-review 186|/Users/jordanknight/pi-hacking/pij-worktrees/s110-review 
ALL TS blobs @3 elements by kind: container=154 file=105 function=586 
TS @3 non-file elements WITH an embedding: 740
jobs by state: done=310236 failed=38 
scoped search (path=~/pi-hacking/pij):
0 []
empty_because: None

## Real-usage receipt on VISIBLE pij TypeScript — 06:21:56
candidate exported functions (unique names): 
DB-chosen TS function names: skipVerdict expectSnapshotsEqual resolvedRecipeBody 
search [skipVerdict expectSnapshotsEqual resolvedRecipeBody ] (defined in ) wall 5.946447000 s → 
## Named-function receipt (bash) — 06:23:00, load 29.31 29.34 26.38
search [expectQuarantineRefusal] (defined in harness/scripts/packages-bootstrap.test.ts) wall 9.394876000 s → 5 ['crates/testkit/src/database.rs:container:tests', 'crates/testkit/src/database.rs:function:the_refusal_names_the_', 'crates/testkit/src/database.rs:function:refusal', 'providers/src/openai_compat.rs:function:the_refusal_names_the_', 's/daemon/tests/read_surface.rs:function:refs_bare_dd_address_i']
search [nodePacketRendererDeps] (defined in skills/flow-pair/lib/packet.ts) wall 28.969354000 s → 5 ['erface/assets/inputs/README.md:section:What the POC already p', 'pocs/human-render/LEARNINGS.md:section:LEARNINGS — human-rend', ':turn:t7316', 'pocs/human-render/LEARNINGS.md:section:3.1 The payload DTOs a', 'pocs/human-render/LEARNINGS.md:section:3. What it made AWKWAR']
search [commonPrefixLength] (defined in harness/driver/index.ts) wall 5.056401000 s → 5 [':turn:t62', ':turn:t10055', ':turn:t11119', ':turn:t54', 'ed-cap-heal/impl-guide.dd.json:section:composition']
## Scoping test: same queries from cwd=~/pi-hacking/pij — 06:24:10, load 17.29 25.85 25.29
search [expectQuarantineRefusal] from pij cwd wall 5.518415000 s → 5 ['ates/daemon/src/http/report.rs:container:Refusal', 's/daemon/src/pointer/worker.rs:function:a_refusal_is_terminal_', ':turn:t2648', ':turn:t2769', 'tes/sidecars/tests/sidecars.rs:container:RefuseFirstDelivery'] scope: {"cwd": "/Users/jordanknight/pi-hacking/pij", "repo": "git:github.com/AI-Substrate/pij", "source": "cwd", "worktree": "/Users/jordanknight/pi-hacking/pij"}
search [commonPrefixLength] from pij cwd wall 9.551979000 s → 5 [':turn:t58', ':turn:t908', ':turn:t18', ':turn:t7076', ':turn:t295'] scope: {"cwd": "/Users/jordanknight/pi-hacking/pij", "repo": "git:github.com/AI-Substrate/pij", "source": "cwd", "worktree": "/Users/jordanknight/pi-hacking/pij"}
meta keys of one envelope: ['empty_because', 'scope', 'truncation'] ['composition', 'results']
## Probe 2 — 06:25:37: TS element harness/driver/index.ts::commonPrefixLength
embedding rows for its raw_hash: 1 model=text-embedding-3-small-no-rate@1024 kinds=raw
a Rust function's embedding row for comparison: model=text-embedding-3-small-no-rate@1024 kind=raw
get by address: 
search by the function's own first line [function commonPrefixLength(a: string, b: string): number { ] wall 7.118066000 s → 5 ['harness/driver/index.ts:function:commonPrefixLength', 'harness/driver/index.ts:file:index.ts', 'ss/scripts/local-path-check.ts:function:basename', 'harness/test-utils.ts:function:fingerprint', 'ness/scripts/snapshot-check.ts:function:sha256']
## TS elements @3 by kind (pij worktree)
queue: [{"count": 1, "kind": "summarize", "state": "running", "with_error": 0}]
## Search receipts (load: load averages: 8.30 11.77 16.79)
search [where does the pij extension register the seat at boot] wall 11.413274000 s → 
search [where does the daemon detect new git worktrees appearing and register them] wall 6.884056000 s → 
search [how is retry handled for embedding jobs] wall 4.066839000 s → 
search [what owns the watcher debounce] wall 15.065816000 s → 
## pg_stat_statements (search statement, since the reset)
## status wall
real 0.29
real 0.32
real 0.32
