# search-admission CRITICAL STOP — final gate migrated production 22 → 23

Final `harness checks` ran on exact head `beee1491be13f3920affc5d257eb580974188360` with:

- `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`
- `CONTAINER=flowspace3-db-test`
- exclusive gate slot held by plan 013.

The gate failed its production migration guard:

```text
a test run changed the PRODUCTION database (version=22 -> version=23)
before: version=22
after:  version=23
```

Per the gate and 2026-08-27 ruling, this is a CRITICAL STOP. I have not rerun anything. The direct full-suite run immediately before this gate was green using the same test URL and logged to `.harness/temp/agent/search-admission-suite-rerun.log`; therefore the write occurred during the final harness gate run, not that captured suite run. No plan-013 code adds migrations or changes database selection/spawn surfaces.

I retain the exclusive slot. Please take incident ownership and rule whether I should perform read-only source diagnosis. Do not treat this as a releasable gate or open the PR yet.
