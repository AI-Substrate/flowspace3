# Plan 013 prod receipt — 2026-09-02T06:50:48Z
## Before (015 build, pid 33521): old search statement stats since the 06:02 reset
23|8629|WITH candidate_vectors AS MATERIALIZED (
## Bounce (old pid 33521) via bin/daemon-restart
daemon-restart: found pane=%50 target=flowspace3:2.0 pid=33521
daemon-restart: sending Ctrl-C to pane=%50
BOUNCE NOT VERIFIED: rc=138 old=33521 new= — stopping

## After manual relaunch on the 013 binary — 2026-09-02T06:53:21Z, pid 1548
(pg_stat_statements reset at 06:53:22)
## Search receipts on the 013 build (load: 29.24 20.22 17.10)
search [where does the daemon detect new git worktrees appearing and] wall .564971000 s → 5 [':turn', ':turn', ':turn'] scan_incomplete= False passes= 1
search [how is retry handled for embedding jobs] wall .384954000 s → 5 [':turn', ':turn', ':turn'] scan_incomplete= False passes= 1
search [what owns the watcher debounce] wall .695606000 s → 5 [':turn', ':turn', ':turn'] scan_incomplete= False passes= 1
search [function commonPrefixLength(a: string, b: string): number {] wall .539329000 s → 5 [':turn', 's/parsers/fixtures/sample.ts:function', 'crates/daemon/src/watch.rs:function'] scan_incomplete= False passes= 1
## New search statement stats (since reset)
4|209|234|WITH scope_blobs AS MATERIALIZED (
    S
## status wall
real 0.28
real 0.28
real 0.29
