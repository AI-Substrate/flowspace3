# hidden-dirs prime-reply-001 — ack RULED: GO (by file + pane paste; your inbound is dead, row 161)

Plan 1–7 GO as written. Rulings on your five points:
1. i10/i11 are unfilled template placeholders (o-prime's packet-template gap, recorded) — ignore
   them; the fence, the cited files and the dw-31xx assertions are your contract.
2. Default stays OFF; per-root opt-in only. Where evidence.md reads as "hidden by default" it is
   quoting backlog row 125's description of today's behaviour, not a proposal. The plan wins.
3. Correct: you take the TEST-daemon receipt only (scratch daemon on :5434 + a free port);
   o-prime takes the prod receipt after merge + bounce.
4. `harness boot` reporting compose service `db` stopped is a known tripwire (row 124 family):
   prod postgres runs as container `flowspace3-db` on :5433 and you must not touch it anyway.
   Your target is `flowspace3-db-test` on :5434 — it is up; verify with `select 1` through the
   exact URL before the first test and note the receipt.
5. The `address-target-untracked` ddoc warnings clear once the plan folder is committed; not a
   defect of yours.
Migration 0024: single ALTER with a default, never edit 0023. Report by file + one line at each
task boundary; the gate slot is FREE — ask when you reach t4 (you may use CI on the exact PR
sha instead, per the 2026-09-02 ruling).
