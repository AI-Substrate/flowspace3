# review-017 DELTA VERDICT — the ratchet bites

**Reviewer:** `pij-unexpected-hyena` (cross-model, github-copilot/claude-opus-5)
**Delta reviewed:** `6d6637d8eac60ec61a200338fb4b061b98040caf..3d7484a4d1ba09be90d344af9b89e0cb1a7b9e4d`
(PR #108 head). Scope held to this range only — nothing re-litigated from round 1.

## VERDICT: f-17a1 CLOSED. No new findings. Ship on CI green.

## Scope of the delta

`git diff --stat 6d6637d..3d7484a` = 8 files, +12/-6. **Exactly two of those lines
are code**, both inside `#[cfg(test)] mod tests`; the other six files are the
plan/tasks/backpressure ddocs and their built siblings. No production code path
changed, so round 1's adjudication of ac-0001..0005 stands unmodified and needed
no re-derivation.

```rust
// crates/daemon/src/boot.rs, in production_store_requires_an_owner_root_or_explicit_designation
+ refuse_undesignated_production_store(&fs3_core::Config::default(), foreign.path(), false)
+     .expect_err("an unset owner_root must fail closed outside an owner tree");
```

This is the exact shape asked for: **`Config::default()`** — so `owner_root` is
genuinely `None` — a foreign cwd, and `explicitly_designated: false`. Not a
`Some(<foreign path>)` stand-in, which would have passed while leaving the real
gap open. It is placed as the FIRST assertion in the test, before the pre-existing
`Some(..)` cases, so a failure here is unambiguously attributable to the unset
state rather than to downstream fallout.

## The check that matters — RED at that assertion specifically

I re-applied the fail-open mutation myself at the delta sha rather than trusting
the report:

```rust
// boot.rs:420-424 — an unset owner_root now DESIGNATES every cwd
- .is_some_and(|root| path_is_within(current_dir, root));
+ .map_or(true,   |root| path_is_within(current_dir, root));
```

**RED, at the new assertion and nowhere else:**

```
thread 'boot::tests::production_store_requires_an_owner_root_or_explicit_designation'
panicked at crates/daemon/src/boot.rs:914:14:
an unset owner_root must fail closed outside an owner tree: ()
```

`boot.rs:914` is the added `expect_err`. This is the specific-assertion proof, not
"something in the suite went red".

## Blast radius — before vs after, the whole point of the fix

| under the SAME fail-open mutation | at `6d6637d` (round 1) | at `3d7484a` (now) |
| --- | --- | --- |
| `cargo test -p fs3-daemon --lib` | **175 passed, 0 failed** | **174 passed, 1 failed** |
| `cargo test -p fs3-cli --test boot_contract` | 5 passed, 0 failed | 5 passed, 0 failed |
| `cargo test -p fs3-daemon --lib boot::` | 6 passed, 0 failed | 1 failed (the same test) |

That row is the finding and its closure in one line: the identical mutation that
the entire suite could not see at `6d6637d` is now caught, by one test, at one
assertion. `boot_contract` staying 5/5 under mutation is correct and expected —
it never covered this guard, which is precisely why the hole existed.

## Restored predicate — green, nothing else moved

Reverted with `git checkout -- crates/daemon/src/boot.rs`, then:

- `cargo test -p fs3-daemon --lib` → **ok. 175 passed; 0 failed**
- `cargo test -p fs3-daemon --lib boot::` → **ok. 6 passed; 0 failed**
- `cargo test -p fs3-cli --test boot_contract -- --test-threads=1` → **ok. 5 passed; 0 failed**
- `cargo test -p fs3-core --lib owner_root` → **ok. 1 passed; 0 failed**
- `cargo test -p fs3-daemon --test health` → **ok. 3 passed; 0 failed**

Counts are unchanged from round 1 (175 / 6 / 5 / 1 / 3) because the assertion was
added to an existing test rather than as a new one — no test-count drift, and no
other test's behaviour moved in either direction.

**CI on `3d7484a`:** run `33613725420` (`ci`) was `in_progress` at the time of
writing. Merge gate is o-prime's, on green.

## Fence — re-asserted after the delta

- Four mutations across both rounds, every one reverted;
  `git status --porcelain -- crates/ bin/` **empty**.
- Shared `flowspace3_test` at `:5434`: **19 worktree roots** — unchanged across
  both rounds.
- Scratch databases matching `fs3_review017%`: **0**. No new scratch daemon or
  database was needed for the delta; it is a pure unit-test check.
- `~/.config/flowspace3/daemon.key` mtime still **`2026-09-02 18:29:55`**,
  matching prod pid 76514's start second-for-second — untouched by me, as in
  round 1.
- `:7373`, `:5433`, the default config dir: never approached. No `harness checks`.
- Observation buffer left **undrained** (o-prime-owned).

## Recommendation

**Merge on CI green.** f-17a1 is closed by the smallest correct change: the
behaviour was already right, and it is now ratcheted by a test that provably
fails when the guard opens. f-17a2 and f-17b1 remain rowed as 182/183 and are
not blockers.
