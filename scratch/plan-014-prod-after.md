# Plan 014 prod AFTER receipt — 2026-09-02T04:20:06Z (read-only; daemon b86593c healthy 04:19:44, 130 s after launch)

```
purge log lines (daemon pane):

done|836061
failed|38
running|11
jobs size: 2810 MB
seq_scan/seq_tup_read: 2121|806611448
status --json retention fields:
['retention', 'purged_last_run']
{"queue": [], "retention": {"last_purge_at": null, "purged_last_run": 0, "window_days": 1}}
status --json wall real 0.29
status --json wall real 0.26
status --json wall real 0.27
load: load averages: 29.67 30.68 30.75
search wall [where does the daemon detect new git worktrees appearing and register them]         "snippet": "[tool-call Bash]\n{\"command\":\"echo \\\"health: $(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1:7373/health)\\\"; timeout 20 flowspace3 ping 2>&1 | head -1; echo \\\"=== search timing post-140 (read-only, same query as this morning):\\\"; /usr/bin/time -p flowspace3 search \\\"where does the daemon detect new git worktrees appearing and register them\\\" --limit 3 --json 2>&1 >/dev/null | grep real\",\"description\":\"daemon health after the postgres restart + one search timing\",\"timeout\":120000}\n\n[tool-call Bash]\n{\"command\":\"for T in bponxz70r bt7gx0o0i; do O=/private/tmp/claude-501/-Users-jordanknight-substrate-flowspace-flowspace3/a5a5588f-0979-439f-a1bf-ddf185a089c7/tasks/$T.output; echo \\\"== $T\\\"; grep \\\"^[<>]\\\\|no change\\\" $O; done; for D in fs3-fresh-db-serialise fs3-search-admission fs3-jobs-retention; do for f in $(ls /Users/jordanknight/substrate/flowspace/$D/.harness/temp/agent/ 2>/dev/null | grep -E \\\"ack|report\\\" | grep -v prime); do echo \\\"=== $D/$f\\\"; head -60 /Users/jordanknight/substrate/flowspace/$D/.harness/temp/agent/$f | cut -c1-200; done; done 2>/dev/null | head -150\",\"description\":\"what the watchers saw; read any new acks/reports\"}",
search wall [where does the daemon detect new git worktrees appearing and register them] real 6.69
search wall [how is retry handled for embedding jobs] real 3.79
search wall [what owns the watcher debounce] real 13.90
```

## Second snapshot — 2026-09-02T04:23:52Z
```
done|258421 failed|38 
jobs size: 2811 MB
retention: {"last_purge_at": "2026-09-02T04:20:27.550Z", "purged_last_run": 898802, "window_days": 1}
purge log lines:
d=898802 last_purge_at="2026-09-02T04:20:27.550Z"
ordanknight/substrate/flowspace/fs3-review-014/docs/plans/014-jobs-retention/assets/reviews events=4
-retention/assets/reviews/cross-model-review.dd.json ms=247 left=7
ordanknight/substrate/flowspace/fs3-docs-014-review/docs/plans/014-jobs-retention/assets/reviews even
-retention/assets/reviews/cross-model-review.dd.json ms=247 left=7
status --json wall real 0.34
status --json wall real 0.32
status --json wall real 0.34
load: load averages: 62.93 44.09 36.03
```
