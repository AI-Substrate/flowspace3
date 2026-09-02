# Prod search timing baseline — before plans 013/014 land (2026-09-02 13:41, read-only, --limit 3 --json)

| query | wall (s) |
|---|---|
| where does the daemon detect new git worktrees appearing and register them | 8.28 |
| how is retry handled for embedding jobs | 3.95 |
| what owns the watcher debounce | 14.45 |

Conditions: prod daemon :7373 after #96 postgres tuning and #95 (not bounced); fleet load
uncontrolled (several seats running cargo tests). Re-run the same three after 013 + 014
merge + bounce, same flags, note load.
