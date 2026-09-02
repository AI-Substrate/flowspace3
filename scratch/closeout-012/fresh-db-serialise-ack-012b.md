# 012b fresh-db follow-up acknowledgment

Packet received and branch `012b-fresh-db-followups` created from `origin/main` in the existing worktree.

## Numbered plan

1. Clamp decoded legacy seed timestamps to 2020-01-01 through now+1h; prove a current seed round-trips within one second and out-of-window seeds are rejected; remove the clamp for the required red mutation, then restore.
2. Move create-path test-hook construction out of the shipped `create_database` body without weakening the N=8 or cross-runtime tests; enlarge the semaphore for the required red mutation, then restore.
3. Copy the governance DDL probe to `bin/ac-0001-ddl-probe.sh`, changing only its usage path; point ac-0001's receipt at it; run `--check` and attributed `pg_first_light` against :5434/`flowspace3-db-test`.
4. Copy the reviewer JSON/Markdown record at the ruled MD5s into the plan review directory and validate it with `ddocs` from the worktree root.
5. Correct active plan/impl-guide/backpressure text for the landed :5434 test postmaster and delta-APPROVE/closed-findings status, while retaining historical :5433 measurements verbatim in immutable receipts and the review record.
6. Run targeted green tests, both required mutations red/restored, formatting, diagnostics, and deterministic-document validation.
7. Commit with `harness commit`, push the follow-up branch, open one PR, wait for exact-head CI, and write/send the final report.

## Current position

Items 1–5 are implemented. Green baselines: seed clamp 1 passed; serialization 2 passed. Required red mutations observed and restored: clamp removal failed on the pre-2020 bound (`artifact://136`); semaphore capacity 8 failed both serialization tests at 8 concurrent creates (`artifact://140`). Shipped probe execution, final validations, commit, and PR remain.
