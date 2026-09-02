# review-017 VERDICT — plan 017 daemon-key-after-bind

**Reviewer:** `pij-unexpected-hyena` (cross-model, github-copilot/claude-opus-5)
**SHA under review:** `6d6637d8eac60ec61a200338fb4b061b98040caf` (PR #108, base `689ac27`)
**Record:** `docs/plans/017-daemon-key-after-bind/assets/reviews/review-017.dd.json`
(+ built sibling `.dd.md`) — `ddocs build` **ok**, `ddocs validate` **ok**, both
run from the worktree root.

## VERDICT: APPROVE WITH FINDINGS — all five ACs TRUE

| AC | judgement | how I proved it (my evidence, not the author's) |
| --- | --- | --- |
| ac-0001 | **TRUE** | Own RED witness: removing the canonicalisation reproduced the incident with `v4 old/new=false/true, v6 old/new=true/false` — A held `::1`, B took `127.0.0.1` and republished. GREEN: 5/5 boot_contract on my run. |
| ac-0002 | **TRUE** | Own mutation: publish-before-bind **fails to compile** (`E0308`). 175/175 daemon lib on my run. Full workspace enumeration: exactly 3 `BoundListener::new`, 3 `publish`, 1 of each in prod code. |
| ac-0003 | **TRUE** | Live A/B on a scratch daemon: unchanged mtime → honest generic hint (no false positive); newer mtime → names the overwrite and `:60843`. Own `>`→`>=` mutation turns the false-positive guard RED. |
| ac-0004 | **TRUE** | Six-case decision table end-to-end through the real binary, all correct — including two the author did not test (unset `owner_root`; `FS3_PROD_OWNER=0`). CI verified by me: run 33610456864 `success`, headSha `6d6637d8…`, job `gate` success. |
| ac-0005 | **TRUE** | Full transcript reproduced on my own scratch setup with my own sha256 + `mtime_ns`; B lost the bind, key byte- and mtime-identical, no residue, `ping` still 0, DB dropped. |

Done-bar: **d1** every AC judged with cited evidence ✓ · **d2** both boot paths
examined for the proof ✓ (normal by test *and* compile failure; sandbox by
construction — see f-17b1) · **d3** record built **and** validated ✓.

## Findings — 2 MEDIUM, 1 MINOR. None falsifies a plan promise.

**f-17a1 (MEDIUM) — the fail-closed guard is untested in exactly the state prod is in.**
`refuse_undesignated_production_store` refuses an unset `owner_root` via
`Option::is_some_and` (boot.rs:420-424), but no test covers
`owner_root: None` + prod URL + foreign cwd. I mutated it to fail **open**
(`map_or(true, …)`) and **175/175 daemon lib, 5/5 boot_contract, 6/6 boot:: all
stayed green.** Since prod has no `[daemon]` section at all, the one behaviour
your post-merge runbook depends on is the one behaviour nothing ratchets.
*Smallest fix — three lines in the test that already exists:*
`Config::default()` + foreign cwd + `false` → `expect_err`.
**This is the only finding I'd close before or with the merge.** The behaviour is
correct today (I verified by hand); what's missing is the ratchet.

**f-17a2 (MEDIUM) — `ping --json` cannot show the field this PR added.**
`DaemonClient::health()` flattens a failed envelope to
`anyhow!(failure.render())` (client.rs:186-188), so `flowspace3 ping --json` on a
401 emits prose on stderr with no `key_newer_than_daemon`. The same daemon's raw
HTTP 401 carries `"key_newer_than_daemon": true` correctly. The human `fix` text
*does* survive, so ac-0003 holds. Pre-existing plumbing (client.rs isn't in the
diff) — but `ping` is the verb an operator meets this outage through, and every
*other* verb already carries the envelope intact. Follow-up.

**f-17b1 (MINOR) — `--json` is not a second boot path.** It's a clap
`global = true` flag affecting only `output_mode()`; `Command::Daemon` branches
on `sandbox` alone. The `--json` test runs byte-identical boot code to its twin.
The genuinely distinct path is `--sandbox`, which is safe by *construction*
(stages into its own tempdir; binds before `serve`) — d2 is met for that half by
reading, not by test. Recorded so nobody later reads "--json is tested" as
sandbox coverage.

## Your three amplifications, answered

**(3) Can the type make publish-without-bind impossible outside the module?**
**Yes — and outside the *crate* it's stronger than that.** `stage`, `StagedAuth`,
`publish` and `BoundListener` are all `pub(crate)`; `lib.rs` re-exports only
`auth::Auth`. No downstream crate can name the staging type, let alone publish.
In-crate, `BoundListener` wraps a *private* `tokio::net::TcpListener` — a type
with no unbound constructor — so a successful bind is a compile-time
precondition of publishing. **The honest limit:** the type proves *a* bind, not
*the configured* bind; that residual is held by discipline at the single call
site (boot.rs:635-643, three lines apart). That's an invariant, not a test.

**(2) Is the prod refusal reachable and legible, or a silent exit?**
**Reachable and fully legible: exit 1, complete text on stderr** — I measured it
through the real binary (safely: non-loopback `daemon.url` as a hard backstop, so
even total guard failure couldn't reach a database). Two caveats worth your
runbook: it fires *before* `logging::init`, so it **never reaches the daemon log
file**; and via `bin/daemon-restart` your own terminal will show only the generic
`no replacement 'flowspace3 daemon' appeared in pane %50` — a refusing daemon
exits inside the first 0.25s poll, so the script won't observe it, and
`FS3-E-PROD-NOT-DESIGNATED` will be sitting in pane %50's scrollback. Expect that
shape on the first bounce. (Script read only, never executed, per your steer.)

**(mtime granularity / clock skew on ac-0003)** — the implementation is *better*
than the plan's wording. The plan promised mtime-vs-**boot time**, two clocks.
The code compares the key file's mtime against **its own earlier mtime** — same
inode, same filesystem clock — so cross-source skew is structurally impossible.
Coarse mtime granularity or a backward clock jump both yield **false negatives**
(degrades to the honest generic hint), never a false positive. I could not
construct a realistic false positive: a matching key short-circuits at
auth.rs:174, so a mere `touch` never reaches the check.

## Fence — stated as what I did NOT touch

- **Never went near:** `:7373`, `:5433`, the default config dir, and
  `~/.config/flowspace3/daemon.key`. Every daemon I started had an explicit
  scratch `FS3_CONFIG_DIR`.
- **⚠️ Packet fence assertion is stale, benignly.** The packet says that key's
  mtime is `16:57:45` and must still be. It is **`2026-09-02 18:29:55`**. Not me:
  prod daemon pid **76514 started at 18:29:55** (release binary from the main
  clone) and the key mtime matches its start **to the second**, with exactly one
  daemon process alive. That is the healthy shape — the owner republishing its
  own key on a legitimate bounce ~47 min ago — not a third clobber.
- **Shared `flowspace3_test` at `:5434`: 19 worktree roots before, 19 after.**
- **Scratch created AND destroyed:** database `fs3_review017_1788340158` on
  `:5434`, dropped `WITH (FORCE)` — `datname like 'fs3_review017%'` now returns
  **0**. All scratch config dirs and cwds removed.
- **Code:** four mutations applied transiently per owed-2, each reverted
  immediately; `git status --porcelain -- crates/ bin/` is **empty**. Nothing
  committed, no PR touched.
- **Gates I did not hold:** no `harness checks` — the exclusive slot is not the
  reviewer's. Targeted `cargo test` only.

## Friction captured (not drained — buffer is yours)

`harness observe` **DL-001** (difficulty, degrading): a review worktree has **no
usable semantic-search surface**. The flowspace MCP server answers *"MCP server
not connected"*, and the only other index is the prod daemon on `:7373`, which
this fence forbids — so AGENTS.md's dogfooding mandate is unsatisfiable from a
reviewer seat and I fell back to grep/lsp. Suggested encoding: a scratch-daemon
recipe in the reviewer packet (scratch config + `:5434` + ephemeral port, index
the worktree, search, drop), or an auto-reconnecting MCP pointed at a per-worktree
index. **I did not clear the buffer** — worker seats list and report only.

## Recommendation

**Merge.** Land f-17a1's three-line test with it or immediately after; f-17a2 and
f-17b1 are follow-ups. The fix is sound, the root cause is genuinely pinned (I
reproduced it), and the central invariant is enforced by the compiler rather than
by a comment.
