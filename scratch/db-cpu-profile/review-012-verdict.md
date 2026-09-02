# review-012 VERDICT — cross-model reviewer (rs seat)

**Plan** 012-fresh-db-serialise · **PR** #95 · **SHA reviewed** `5c7f7bdb` (head later advanced to `05d7d87c`, docs-only — review stands)
**Review record** `docs/plans/012-fresh-db-serialise/assets/reviews/review-012.dd.json` (+ built `.dd.md`, `ddocs validate` → ok)

## VERDICT: CHANGES REQUESTED — 1 HIGH, 3 MEDIUM, all confirmed by evidence I ran

The serialisation primitive is real. Everything inside `FreshDatabase` takes the permit. What is not true is the fleet-level promise, and the change quietly grants a destructive capability that did not exist before.

| # | Sev | One line | Fix size |
| --- | --- | --- | --- |
| f-1a01 | HIGH | `cargo test -p fs3-store` still issues unserialised concurrent CREATE/DROP — 107 sites via a duplicate helper the lock never reaches | move the semaphore into `fs3_store::{create,drop}_database` |
| f-1a03 | MEDIUM | every `harness checks` now force-drops any aged `fs3_<label>_…` — including a live `flowspace3 sandbox` >6 h old | one predicate: `numbackends = 0` |
| f-1a02 | MEDIUM | a listening server that refuses permanently (bad password) is told "wait and retry" | one condition ahead of `:148` |
| f-1a04 | MEDIUM | the concurrency test asserts the primitive, not the create path — it stays green if a create path forgets the permit | one test at N=8 on `create` |

**Fix before merge**: f-1a03 (only item granting new destructive reach) and f-1a02 (one condition; it *is* the "truthful advice" promise).
**Named follow-ups**: f-1a01, f-1a04. Row 126 should read **reduced**, not closed.

## The three owed lists, answered

**(1a) Is the lock process-wide?** **YES.** `static OnceLock<Semaphore>` + `tokio::sync::Semaphore`, awaited — not a std Mutex across `.await`, not a per-runtime Lazy. Proven, not read: `crates/testkit/src/fresh_database.rs:430-467` spawns OS threads each building its own `new_current_thread` runtime, barrier-synced. Green as shipped; **I deleted the permit (`:45-48`) and it went red** — `observed more than 1 concurrent database mutations` at `fresh_database.rs:459` — then restored the file and confirmed `git status` clean.

**(1b) Can any path reach `create_database`/`drop_database` without the permit?** **Inside `FreshDatabase`, no** — `create_unmigrated_from:239`, `sweep_orphans_at:314`, `cleanup:356`; `create_from`→`create_unmigrated_from`, `destroy`→`destroy_force`→`cleanup`. No nesting, so no self-deadlock at N=1. **Outside it, yes, three ways** → f-1a01.

**(1c) Advice classifier.** Both owed negatives hold: a refused port can never read "recovering" (the `Some(false)` branch returns first, `:138`), and the recovery wording never contains `COMPOSE_UP` (`:149-151`). The failure is the third case → f-1a02.

**(1d) Sweep parser — the negative asserted.** `flowspace3`, `flowspace3_test` fail the `fs3_` prefix. Any name lacking `<epoch>_<32hex>` fails. Any label containing `_` fails (the second `rsplit_once` leaves the underscore inside `label`, failing the alnum check) — so `fs3_migrations_<32hex>` and `fs3_blastradius` are both ineligible. And the catalog query is a **literal** prefix, `left(datname, length($1)) = $1` (`crates/store/src/admin.rs:176`), not `LIKE` — `_` is never a wildcard. **The parser is sound.** Its blindness is to *liveness*, not to shape → f-1a03.

**(2) Receipts disbelieved, re-derived.** 27/27 testkit lib tests green in my own run against my own DB (`fs3_review012` on **:5433**, never :7373). Mutation performed. Oversize re-run myself. Bad-password advice probe executed.

**(3) Known-open — zero findings spent** on row 110, 124b, 140, 122.

## Row-141 forced-checkpoint evidence (the number you asked for)

Shared `flowspace3-db`, pg16, `log_checkpoints=on`, `shared_buffers=128MB`. Each run preceded by a 40 s idle baseline.

| Run | Window (UTC) | Result | Forced checkpoints BEFORE → DURING |
| --- | --- | --- | --- |
| `cargo test -p fs3-daemon --test oversize`, **default parallelism** | `01:15:18Z → 01:15:38Z` | 12 passed / 15.12 s / exit 0 | **0 → 2** |
| `cargo test -p fs3-store`, **default parallelism** | `01:17:00Z → 01:17:38Z` | exit 0 | **0 → 25** |

Terminations / recovery / starting-up lines: **0** in both windows.

The interesting number is not the count, it is the shape: the forced checkpoint starting `01:15:24.987` ran **`total=12.887 s`** (11633 buffers = 71 % of shared_buffers, `sync=11.515 s`, 4503 files), with the remaining drops queued onto a second checkpoint at `01:15:37.884`. **~12.9 s of a 15.1 s suite was one forced checkpoint.** So serialisation *coalesces* forced checkpoints; it cannot remove them, because `DROP DATABASE` forces one by design. The 25 from the store suite are the ones nothing is coalescing.

## AC ledger

| AC | Verdict | Basis |
| --- | --- | --- |
| ac-0001 | **PARTIAL** | serialisation real + mutation-proven by me; but counter is on the primitive not the create path, N=2 not 8, and "no caller can bypass" is false at fleet level |
| ac-0002 | **TRUE as written** | both named cases green in my run; goal 2's broader claim fails on the third case |
| ac-0003 | **TRUE** | both shapes swept, fresh kept, green in my run; negative asserted four ways |
| ac-0004 | **TRUE** | independently reproduced — table above |
| ac-0005 | **NOT PROVEN** | tk-0106 unchecked, **and it cannot be run as specified**: `list_orphans_from` has zero callers anywhere; the only production caller of the sweep is the destructive `fs3-test-suite:17`. o-prime needs a throwaway binary first — name that seam rather than discover it |

`tk-0105` (gate/CI) also unchecked at `5c7f7bdb`. `cargo fmt --all --check` and `cargo clippy -p fs3-testkit --all-targets` are clean here. **I deliberately did not run `harness checks`** — `fs3-test-suite` would have executed the widened destructive sweep against the container every other seat shares. That is f-1a03 biting the reviewer.

## Scope truth for the merge record

The semaphore is per **process**. CI runs `cargo test --workspace` (`.github/workflows/ci.yml:121`), the gate runs `cargo test --all`, no nextest — so one process per binary, binaries sequential, and within a cargo invocation the guarantee is effectively machine-wide. It is **not** machine-wide across seats: N worktrees each running their own gate against the one :5433 postmaster still produce N concurrent `CREATE DATABASE`. That multi-seat concurrency, plus f-1a01, is the actual crash mechanism this plan cites. The real fix is the declared non-goal (row 124b).

## Hygiene

Read-only on code throughout, except the instructed mutation, which was applied and reverted in a single command with `git status` verified clean. Wrote only `assets/reviews/` and `.harness/temp/agent/`. Per-run DB `fs3_review012` on **:5433**; dropped on exit.
