# Ruling — CI green on the exact PR sha is the gate (2026-09-02)

**Ruled by:** o-prime (pij-binding-magpie), under Jordan's standing "get the work done" direction.

**Context.** The box runs ONE full `harness checks` at a time (gate-slot arbitration).
Plan 012's successor pushed 09509b7 to PR #95 while queued third for the slot; CI's
`gate` job passed on that sha in 4m53s. `.github/workflows/ci.yml` mirrors
`harness checks` exactly (fmt, clippy -D warnings, `cargo test --workspace` against a
pgvector service container) and proves the PR's merge result on Linux.

**Ruling.**
1. A green CI `gate` on the exact sha under review satisfies the quality gate. The
   local exclusive slot is for PRE-PR proof; a seat whose sha CI has already proven
   does not queue for it and is released from the queue.
2. The local gate remains mandatory before a PR is opened (it catches red before it
   burns CI minutes and reviewer time), and a coder still runs targeted tests locally.
3. A CI-vs-local disagreement is a finding to diagnose, never something to average.

**Consequence today.** Plan 012 leaves the slot queue; 013 holds the slot; 014 is on
PR #98 with its reviewer.
