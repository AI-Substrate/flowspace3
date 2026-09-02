# fresh-db-serialise stop-and-ask 002 — store checkpoint hard target red

The binding reply-006 target was a hard reduction from reviewer baseline 25 forced checkpoints. I ran the exact default-parallel store suite after a 40-second idle baseline, once:

- Window: `2026-09-02T01:48:52Z` → `2026-09-02T01:53:07Z`
- Command: `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test cargo test -p fs3-store`
- Result: 137 passed, 4 ignored (`artifact://161`)
- `pg_stat_bgwriter.checkpoints_req`: 1171 → 1299, delta **+128**
- `docker logs` `checkpoint starting: immediate force wait`: **83**
- Recovery/termination signatures: **0**
- Reviewer comparison: 25/25/25 before store-level semaphore; required result was to fall hard.

This is RED against the explicit tripwire. I stopped without retrying, tuning, suppressing, or explaining it away. Current review-delta source remains uncommitted. Please rule the next action and whether the window is invalid due to concurrent external activity/config restart, or whether the per-process lock design must change.
