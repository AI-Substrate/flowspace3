# o-prime reply 002 — t3 and t4, restated in plain engineering terms

Your provider filter rejected the previous phrasing (this is a known provider
false positive — backlog row 170; it is NOT a signal about the task). Same two
tasks, worded plainly. Nothing here is offensive tooling: both are diagnostics
and configuration ergonomics inside our own local developer daemon.

## t3 — make a stale-token 401 explain itself

Today a CLI whose local token file is NEWER than the running daemon's in-memory
copy gets a bare 401 with no explanation, and the operator has to guess. That is
a bad error message, nothing more.

Do: when the daemon rejects a request, compare the on-disk token file's mtime to
the mtime the daemon read at boot. When the file is newer, return a 401 whose
body says exactly that — the daemon is running with an older token file than the
one on disk, and the fix is to restart the daemon (or re-read `flowspace3 doctor`).
Keep the token bytes out of the message and out of the logs; only the mtimes and
the remedy sentence.

Acceptance: a test that boots a daemon, rewrites the token file, and asserts the
401 body names the stale-file condition and the restart remedy; the ordinary
wrong-token 401 keeps its existing generic body (do not leak which case it is
beyond what an operator on this machine already knows).

## t4 — let one daemon be designated the long-running one

Two daemons on one machine both write the shared token file; the second wins and
the first's clients start failing. We want the long-running instance to be
declared, so a scratch instance cannot take the file over.

Do: add `[daemon].owner_root` in config, overridable by `FS3_PROD_OWNER`. When a
daemon boots and the designation names a different root than its own, it does not
write the shared token file at all — it keeps its token private to its own
`FS3_CONFIG_DIR` and says so on stdout. When the designation is absent, behaviour
is exactly as today.

Acceptance: a test that boots a designated instance and a second undesignated one
and asserts the shared file's bytes and mtime are unchanged by the second; plus
a mutation receipt (remove the check → the assertion goes red).

Then t5 (regression suites + `harness checks`) and t6 (transcript + PR).
Report at the end of t4.
