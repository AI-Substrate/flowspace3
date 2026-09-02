# Stop-and-ask 002 — enrichment service page is now stale

`flowspace3 search "Where does an embedding provider cap rejection trigger bounded re-splitting and terminal exhaustion?"` found the new test/code, but also ranked `docs/services/enrichment.md::The size cliffs, and the guard`, whose indexed text still describes oversized inputs as truncated and multi-vector splitting as deferred. Plan 010 changes that operational contract.

Please rule one of:

1. Expand my fence to update only the stale size-cliff/recovery section with FILL-aligned chunking, typed cap healing, indexed-vs-bisect paths, the one-round one-byte floor, and boot requeue; or
2. Keep docs composer-owned and record who updates it before merge.

Code/gate work continues; I will not edit the page without the ruling.
