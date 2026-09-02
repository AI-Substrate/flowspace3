# STOP-AND-ASK 005 — prod bounce requeue can collide on duplicate failed key

The pre-bounce read found five failed embed rows but only four unique dedupe keys:

- 1314967 — `embed:git:github.com/AI-Substrate/pij:raw:8ecdedc6d073e221d0ebfecb61950417c120114ee98067b78178c410e3c6c560`
- 1315244 — `embed:git:github.com/AI-Substrate/pij:raw:9dce3968beeb4c5ba9d19ac287d25d957e1374819c1f95d6fba39cc227946650`
- 1316706 — `embed:git:github.com/AI-Substrate/pij:raw:043365681f0eb07897f17377016f1d2ac4a4de541de7ea34bb45b5f9eeb7590f`
- 1323215 — same `043365…` key
- 1344012 — `embed:conv:recovery:raw:c5a6be2d9ece36c51e1096c16d4522e62cee52e6e67caeecb85998ac39005da7`

All are `failed|attempts=3|terminal=false`; `flowspace3 status --json` reports embed failed=5.

`requeue_failed` updates every matching failed row to pending in one statement. Its `NOT EXISTS` only excludes a key already live before the update; both duplicate failed rows can qualify together, then collide with the unique pending/running dedupe index and abort the sweep. The existing acceptance assumption “bounce automatically requeues all five” is therefore not safe.

Please rule before bounce:

1. expand my fence to make `requeue_failed` duplicate-safe with a regression test, or
2. o-prime runs a named one-line prod repair selecting one row per dedupe key and terminally retires the superseded duplicate.

Do not bounce on the current automatic mechanism without this ruling. Code PR #92 remains in CI.
