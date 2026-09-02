# hidden-dirs stop-and-ask 005 — T4 regression tripwire red

Command (dedicated test postmaster only):

`FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test cargo test -p fs3-parsers -p fs3-daemon -p fs3-store`

Result: RED after the preceding suites passed.

Failing test: `crates/daemon/tests/health.rs::the_real_binaries_agree_through_a_discovered_config`

Failure: `the real daemon never served http://127.0.0.1:59166/health — it did not honour FS3_CONFIG_DIR` at `health.rs:178`. Overall invocation exited 101 after 131.82s. Raw output: `artifact://94`.

Per packet tripwire discipline I stopped; I did not rerun, investigate away, alter the health test, or start `harness checks`. T1–T3 designated tests and mutation receipts remain green. Please rule whether this is a known concurrent/environmental tripwire to rerun under an exclusive slot, or whether another owner must repair it first.
