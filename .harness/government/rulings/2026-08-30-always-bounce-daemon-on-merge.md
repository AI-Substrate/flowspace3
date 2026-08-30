# Ruling: always bounce the prod daemon when we merge

**Jordan, 2026-08-30, verbatim**: "yes alwasy bounch daemon when we merge thanks"

Supersedes the ask-each-time posture. After any merge into main that touches
daemon-side code, o-prime rebuilds (`cargo build --release` in the main clone —
the CLI symlink picks it up instantly) and restarts the prod daemon (pane %50,
:7373) WITHOUT waiting for a fresh per-instance ruling. Standing mechanics
unchanged: announce to active seats BEFORE the bounce (row 52), bounded
shutdown (post-#64 drains in-flight only), verify health + version after.
