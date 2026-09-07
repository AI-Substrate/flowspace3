# post-drain probe — 2026-09-06T23:52:38Z (behind=14 after 57 min)
## last 6 poll lines
2026-09-06T23:46:49.584566Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=58 skipped=111
2026-09-06T23:48:05.512550Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=51 skipped=111
2026-09-06T23:49:01.099827Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=42 skipped=111
2026-09-06T23:50:00.853065Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=33 skipped=111
2026-09-06T23:51:01.019794Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=24 skipped=111
2026-09-06T23:52:00.768670Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=14 skipped=111
## quiet DEBUG passes? (grep debug reconciled/quiet for convo_poll in last 200 lines)
## jobs by kind now
embed|62481
ingest_session|696
scan_file|3440
summarize|149543
## ingest_session jobs per session (top 12 by count) — the 7 zero-record files should be exactly 1 each
44|ingest:omp/01a073f3-f668-7000-9e5c-31c5a5c853cf@/Users/jordanknight/substrate/chainglass
25|ingest:claude/a5a5588f-0979-439f-a1bf-ddf185a089c7@/Users/jordanknight/substrate/flowspace/flow
21|ingest:claude/e144359d-e66d-48f2-8095-b7f4dbf9705e@/Users/jordanknight/substrate/chainglass
16|ingest:omp/01a06e96-931c-7000-9dba-4983e5fd3586@/Users/jordanknight/substrate/harness-engineeri
13|ingest:claude/cbe08128-4097-4473-9f99-71e9415824f6@/Users/jordanknight/pi-hacking/pij
9|ingest:omp/01a078ca-d20c-7000-9ed4-e6baa3b069af@/Users/jordanknight/pi-hacking/pij
8|ingest:omp/01a075b5-7ae1-7000-85eb-57df4ebbd29d@/Users/jordanknight/substrate/chainglass
5|ingest:claude/6616ca6a-61c0-4a92-b9be-3512605a2826@/Users/jordanknight/substrate/harness-engine
3|ingest:claude/a5a5588f-0979-439f-a1bf-ddf185a089c7@/Users/jordanknight/substrate/flowspace/fs3-
2|ingest:omp/01a0470e-70e1-7000-b73c-d68472ef82e7@/tmp/claude-501/-Users-jordanknight-pi-hacking-
2|ingest:omp/01a0470d-572d-7000-be6b-e12a79471c2f@/tmp/claude-501/-Users-jordanknight-pi-hacking-
2|ingest:omp/01a04ac7-31b7-7000-8e2c-814758099346@/tmp/claude-501/-Users-jordanknight-pi-hacking-
## status.conversations
flowing | catching up: 14 behind, 10 in flight, 0 outcomes unavailable; 10 submitted this pass
  claude tracked 126 behind 4
  omp tracked 410 behind 10
