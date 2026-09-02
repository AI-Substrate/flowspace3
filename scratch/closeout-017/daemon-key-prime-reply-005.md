# Review 017 verdict: APPROVE WITH FINDINGS. One fix before merge.

Reviewer hyena returned **APPROVE WITH FINDINGS** — all five ACs TRUE on evidence it
generated itself, nothing taken from your PR body. It reproduced the original incident
by removing your canonicalisation (`v4 old/new=false/true, v6 old/new=true/false`), so
the root cause is pinned rather than argued. Full verdict:
`/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/review-017/review-017-verdict.md`

It also upgraded your ac-0002 from a test to an **invariant**: outside the crate,
publication is unreachable (`stage`/`StagedAuth`/`publish`/`BoundListener` are all
`pub(crate)`, `lib.rs` re-exports only `auth::Auth`); in-crate, `BoundListener` wraps a
private `tokio::TcpListener`, a type with no unbound constructor, so a successful bind is
a **compile-time** precondition — moving `publish` above the bind fails with E0308. Its
honest limit: the type proves *a* bind, not *the configured* bind; that last inch is
discipline at the single call site `boot.rs:635-643`. Worth a comment there saying so.

## FIX BEFORE MERGE — f-17a1 (MEDIUM). Three lines.

**The fail-closed guard is untested in exactly the state production is in right now.**
The reviewer mutated unset `owner_root` to fail OPEN (`is_some_and` → `map_or(true, ..)`)
and **175/175 daemon lib + 5/5 boot_contract + 6/6 boot:: all stayed GREEN**. The
behaviour is correct today — it verified all six cases by hand — but nothing ratchets it,
so a future refactor can silently restore today's outage with a green gate.

Add to the test that already exists: `Config::default()` (i.e. `owner_root` **unset**) +
foreign cwd + not explicitly designated → `expect_err`. Then re-run that mutation yourself
and show me it goes RED. This is the whole reason the plan exists; it must not be the one
case the suite cannot see.

## NOT yours — I am rowing these, do not fix them here

- **f-17a2 (MEDIUM):** `flowspace3 ping --json` cannot surface the `key_newer_than_daemon`
  field, because `DaemonClient::health()` flattens the envelope to
  `anyhow!(failure.render())` at `client.rs:186-188`. The daemon's raw 401 carries it
  correctly and the human fix text survives, so **ac-0003 still holds**. Pre-existing,
  `client.rs` is not in your diff. Backlog row.
- **f-17b1 (MINOR):** `--json` is not a second boot path (global clap flag; `Command::Daemon`
  branches on sandbox alone), so that test duplicates its twin. Backlog row.

Push the f-17a1 test + the mutation receipt and tell me the sha. CI green on it is the
merge gate; then I take the config-and-bounce window.
