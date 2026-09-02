- id: DL-001
  kind: difficulty
  description: "Shared flowspace3-db (port 5433) crashed into recovery mode during 'cargo test -p fs3-daemon --test oversize': 12 tokio tests each CREATE DATABASE in parallel on a loaded host; postgres logged 'server process (PID 3147563) exited with exit code 2' -> 'terminating any other active server processes' -> automatic recovery. Same shape recorded 2026-08-27/28 against 'CREATE DATABASE fs3_migrations_*' (signal 6 Aborted). All 12 tests failed with 'expected to read 5 bytes, got 0 bytes at EOF', which reads as a test failure, not as 'the fleet database just died'."
  severity: degrading
  workaround: "Waited ~20s for automatic recovery, then re-ran with --test-threads limited."
  suggested_encoding: "FreshDatabase should serialise CREATE/DROP DATABASE behind a process-wide lock (or the daemon test binaries should default to a low --test-threads), and its connect-failure panic should distinguish 'server in recovery / closed the connection' from 'no postgres configured' so the message names the real cause instead of telling the agent to run docker compose up."
  fp: b96318ca2845
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:49:31.317Z"
