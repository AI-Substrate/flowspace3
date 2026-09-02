# Plan 017 prod receipt — 2026-09-02 19:30 local

Prod on main `2d7f45f` (release build 19:29), daemon pid 6252 started 19:29:52,
`~/.config/flowspace3/config.toml` now carries `[daemon].owner_root` (backup
`config.toml.bak-1929`).

## Bounce
- old pid 76514 → Ctrl-C → exited; relaunched by hand in pane %50 (NOT via
  `bin/daemon-restart`, which is backlog 181 and took prod down on the previous bounce).
- Time to serve: ~78 s from launch to listening — the row-177 pre-serve ddoc probe over
  69 registered roots, worse than the 21 s measured at 18:29 because the process also
  re-walked its watch set. Prod pays this on every bounce and no gate can see it.
- `flowspace3 ping` healthy; `status` 69 roots, exactly 1 with `include_hidden=true`
  (the 016 opt-in on `~/pi-hacking/pij`), so plan 016 survived the bounce.

## THE RECEIPT THAT MATTERS — today's outage cannot recur
Ran the merged binary from a FOREIGN cwd (`/private/tmp`) against the PROD config,
which is exactly what happened twice this morning (rows 165, 169):

```
flowspace3: FS3-E-PROD-NOT-DESIGNATED: refusing to boot against the production database
at 127.0.0.1:5433/flowspace3 from cwd /private/tmp. This process is not designated to own
it (`[daemon].owner_root` is /Users/jordanknight/substrate/flowspace/flowspace3). Start
the long-running daemon from inside that root, or set FS3_PROD_OWNER=1 for an intentional
production launch. Tests and scratch daemons must use
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test instead.
```

Key file before: mtime `1788341393`, sha256 `8e6ad2ccc9e9…`
Key file after:  mtime `1788341393`, sha256 `8e6ad2ccc9e9…`  → **UNTOUCHED**
Incumbent daemon still healthy on :7373 immediately afterwards.

So: the foreign daemon refuses, names the owner root, names the override, names the test
URL, and never reaches key staging or the database. The failure mode that logged every
client out twice today is closed, in production, with a receipt.

## Caveat confirmed
The refusal goes to **stderr only** — it fires before `logging::init`, so it is absent
from the daemon log file (017 review, hyena). Read the pane, not the log.
