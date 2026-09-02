# review-012 DELTA VERDICT — cross-model reviewer (rs seat)

**Plan** 012-fresh-db-serialise · **PR** #95
**Round 1** `5c7f7bdb` measured · **Round 2** `f3aec311` structural + measured · **Round 3** `09509b75` **verified**
**Record** `docs/plans/012-fresh-db-serialise/assets/reviews/review-012.dd.json` (+ built `.dd.md`, `ddocs validate` → ok, 27 findings)

---

# VERDICT: APPROVE

Every finding this review raised is closed or explicitly accepted. The headline target is met, measured independently by me. The two new guards were mutation-proved, not taken on trust. One NIT and one partial remain, neither blocking.

## The number, measured three times across three shas

$$16 \;\longrightarrow\; 2 \;\longrightarrow\; 1$$

| Sha | Suite | max concurrent DDL | samples >1 | foreign |
| --- | --- | --- | --- | --- |
| `5c7f7bdb` | full `fs3-store` | **16** (n=2) | 52, 43 | 1 |
| `f3aec311` | full `fs3-store` | **2** (n=2) | 196, 137 | 16, 15 |
| **`09509b75`** | full `fs3-store` | **1** | **0** of 774 active | **0** |
| `09509b75` | `pg_first_light` (was site 1) | **1** | **0** of 143 active | 0 |
| `09509b75` | daemon `first_light` (was site 2) | **1** | **0** of 87 active | 0 |

All on `:5434` / `flowspace3-db-test`, default parallelism, attributed by `application_name`.

## New guards — mutation-proved, not accepted

Green baselines first: `fs3-testkit` 29 passed, `fs3-store` 18 passed, zero failures.

| Mutation | Result | Closes |
| --- | --- | --- |
| Force the sweep drop via the shared const | **RED** `fresh_database.rs:571` | f-1a18 |
| Remove the permit from the store create path | **RED** `admin.rs:705` — the **restored** cross-runtime test | f-1a13 |

## Fold-ins confirmed

- **f-1a0d** — both raw-DDL sites swapped; measured $2 \rightarrow 1$ on each binary independently
- **f-1a0f** — SQLSTATE class rule adopted: `server_rejected_permanently` = a database error whose code does not start with `57`, exactly as proposed
- **f-1a14** — `ac-0001` carries the `pg_stat_activity` clause as an **additional** receipt with explicit thresholds
- **f-1a18** — `ac-0003` names the unforced `DROP` as defence-in-depth covered by the new guard
- **f-1a15** — `ac-0004` now names `:5434` and `docker logs flowspace3-db-test`; the stale "no separate test postmaster" non-goal is gone
- **f-1a0e** — parser widened to reach `fs3_migrations_*` and `fs3_storelock_*` (see the NIT below)

## Outstanding, non-blocking

**f-1a1a (NIT, new)** — the widened parser now *derives* age from entropy. `unique_seed_created_at` is `(seed as u64) / 1_000_000_000` with **no range check**. Correct today, because `unique_seed` puts nanos in the low 64 bits — but the sweep's age decision for two name shapes is now coupled to that bit layout and nothing tests the coupling. If the layout ever changes, uniformly random low-64 bits decode to roughly **10 % below the current epoch**, i.e. immediately sweepable. The dangerous direction is the cheap one to close: clamp the decode to a plausible window, and add one test asserting `unique_seed_created_at(format!("{:032x}", unique_seed()))` is within seconds of now.

**f-1a10 (partial)** — `cfg(test)` scaffolding is tidied into a named `create_test_hook` module, but two `cfg(test)` lines still sit in the shipped `create_database` body. Acceptable; noted rather than pressed.

## Scope truth for the merge record

The semaphore is per **process**. Row 126 reads **reduced**, not closed — as the plan's own summary now states. Row 124b landed mid-plan as the `:5434` postmaster, and the non-goals were corrected to match.

## Method notes worth keeping

- **The two-empty-sets trap.** An empty `list_orphans` agreeing with an empty catalog proves nothing — a listing that always prints nothing agrees just as well. `ac-0005` was only proved once four databases were minted to give it something to be wrong about: aged-idle-conforming (listed), aged-but-live (excluded), too-young (excluded), malformed (excluded).
- **Mutations need a clean server.** The race test leaked on failure and its epoch-1 names poisoned the next run's candidate window. My own first M2 "red" was that poisoning; caught only because two different mutations failed at an identical line.
- **Attribution before conclusions.** My original `16` was measured without attribution on a shared server with five live worktrees. It re-derived identically once attributed — accidentally correct, which is not the same as demonstrable.

## Hygiene

Read-only on code throughout; every mutation applied singly and reverted byte-identical, tree verified clean after each. Wrote only `assets/reviews/` and `.harness/temp/agent/`. All scratch databases dropped to zero. Six observations (DL-001…DL-005, CONF-001) captured and **not** drained — the buffer is shared.
