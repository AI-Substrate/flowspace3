# review-012 ACK — cross-model reviewer (rs seat)

- **Plan**: 012-fresh-db-serialise · **PR**: #95 · **SHA under review**: `5c7f7bdb069cdb79de3bcf2203d57f934a75c22c`
- **Worktree**: /Users/jordanknight/substrate/flowspace/fs3-review-012 (detached HEAD, confirmed `git rev-parse HEAD` == 5c7f7bdb)
- **Packet**: docs/plans/012-fresh-db-serialise/packet-reviewer.dd.json — read, and the THREE OWED LISTS are present (i6). Not refusing; brief is complete.

## The three owed lists, as received

1. **Least confident — hunt first**
   - (a) lock truly process-wide across per-test tokio runtimes (static Semaphore ok; std Mutex across `.await` is the trap; per-runtime Lazy silently per-test)
   - (b) any path reaching `fs3_store::create_database`/`drop_database` WITHOUT the permit (`create_from`, `create_unmigrated_from`, `cleanup`, `destroy_force`, sweep)
   - (c) advice classifier: refused port must never read "recovering"; "recovering" must never print COMPOSE_UP
   - (d) sweep parser: must never match `flowspace3`, `flowspace3_test`, or any name lacking the `<epoch>_<32hex>` tail — assert the negative
2. **Disbelieve the receipts** — re-run testkit tests myself; PERFORM the lock mutation (remove permit, observe >1 in flight); run the oversize suite at DEFAULT parallelism with a stated log window; grep container log for `terminating any other active server processes` AND `checkpoint starting: immediate force wait`, reporting before/after forced-checkpoint counts (row-141 evidence).
3. **Known-open — zero findings spent**: 56 leaked `fs3_*` DBs on prod (row 110), separate test postmaster (124b), postgres config (140), search CTE (122).

## Fence acknowledged

Read-only on code. Writes limited to `docs/plans/012-fresh-db-serialise/assets/reviews/` and `.harness/temp/agent/`. No edits, no commits, no merges, no government files.
Per-run DBs on **:5433** with my own label; **never :7373**.

## Channel

Delivering by FILE (`review-012-ack.md`, `review-012-verdict.md`) plus a one-line `pij send pij-binding-magpie` pointer per file. Never `pij send pij-instant-lynx`; never `pij adopt`.

Starting review now.
