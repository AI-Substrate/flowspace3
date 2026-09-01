# w-conv-verify — address-authoritative `conv:` get + `conversation verify` (backlog row 119)

## What Jordan ruled (2026-09-02)
"have sol code, claude review" — coder on `github-copilot/gpt-5.6-sol-fast-1m`
(omp, effort high), reviewer on Claude. Dispatched AFTER w-embed-cap-heal acks
cleanly through pij. A peer government (harness-engineering) is holding its
`--verify` until this verb exists; its contract is below and is binding.

## Current state, falsifiable in one read
- `flowspace3 get conv:<guid>#t1` succeeds ONLY when the conversation's anchor
  worktree equals the cwd's. Otherwise: `FS3-E-QUERY-NOT-FOUND`, message
  "no conversation <guid> is indexed" — FALSE; `conversation list` shows it.
  `--repo all` lifts it. Measured 2026-09-02 on conv:8c285d65 (same repo as
  cwd, different worktree) and on pij/dd conversations.
- `get`'s own help says: "from the INDEX, so it answers for every registered
  repository, not only the one you are standing in." Shipped behaviour
  contradicts the documented contract.
- The daemon-side message lives around `crates/daemon/src/conversations.rs:274-358`;
  the HTTP surface in `crates/daemon/src/http.rs:304-346`.
- The guid is derived, not minted: `conversation_guid()` at
  `crates/daemon/src/convo_ingest.rs:341` — sha256 of
  `fs3-convo-v1:{harness}/{session_id}` with FORCED version/variant nibbles.

## The job
1. **An explicit `conv:<guid>` is address-authoritative.** A guid the caller
   typed in full is not a search; resolve it index-wide regardless of cwd.
   (`el:` addresses are out of scope — they carry their repo.) If o-prime's
   ruling is overridden by Jordan toward keeping scope, the fallback is: the
   miss says "not in this scope" and prints the widening flag. Build the
   authoritative version; keep the fallback wording ready.
2. **The miss must never claim global absence for a scope decision.** Any
   remaining not-found path on a conv address distinguishes "no such guid in
   the index" from "exists, outside this scope" — two messages, two `details`.
3. **`flowspace3 conversation verify --harness <h> --session <id>`** — the
   consumer's contract, verbatim: exit 0 + `ok:true` ⇒ delivered (return the
   guid, address, turn count, repo, worktree, last turn timestamp); a DISTINCT
   not-indexed error code ⇒ not delivered; **repo-unscoped by construction** —
   no flag can narrow it, so the `--repo all` trap cannot exist in it. Also
   accept `--pij <seat>` via the existing join so the seat route is verifiable
   too (it will be blind for rs seats until pij req-0033 — say so in the error,
   do not fix it here).
4. **Mutation-checked tests**: (a) a conv get from a cwd whose worktree is NOT
   the anchor — fails before, passes after; (b) `verify` for an ingested
   session from a foreign cwd returns `ok:true`; for a never-ingested session
   returns the distinct code; (c) the two not-found messages are asserted as
   two.
5. Docs: `crates/cli/docs/*` for get and conversation; help text.

## Read first
Row 119 in `government/briefs/backlog.md` (both entries) · PR #84's
`--conversation` binding · `convo_ingest.rs:341` · `conversations.rs` ·
row 101 (the ruling this measurement backs).

## Deferred — do not build
Any change to `el:` resolution. The identity union (row 121). Anything in
the embed path.

## Fence
`crates/cli` (get + conversation verbs, docs), `crates/daemon/src/
{conversations,http,convo_ingest}.rs` and tests. Nothing else without a
stop-and-ask.

## Done-bar
`harness checks` green · `feat:` commits via `harness commit` · PR into main
with the mutations stated · live proof after o-prime's bounce: `verify` for
conv:8c285d65's session from the main checkout returns `ok:true`; the old
lying message is gone · buffer LISTED, never cleared.

## Isolation and the wire
Own worktree `../fs3-conv-verify`, branch `conv-verify`, per-seat
`CARGO_TARGET_DIR`, per-run test DBs. NEVER test against prod :7373.
**Ack before code**: numbered plan to o-prime, WAIT.
**pij is unstable** (`government/pij-two-daemons.md`). If `pij send` fails
twice, write to `.harness/temp/agent/conv-verify-ack.md` / `-report.md` and
STOP — o-prime polls those paths. Never `pij adopt`.
