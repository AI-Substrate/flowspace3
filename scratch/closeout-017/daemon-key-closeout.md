# Plan 017 daemon-key-after-bind close-out
1. Reply 002 paraphrased ac-0004 into a different contract: continue with a private token instead of refusing production boot.
2. I caught it by comparing the reply against READY `plan.dd.json`, `impl-guide.dd.json`, task t4, and bp-0004 rather than treating prime prose as a contract mutation.
3. Stop-and-ask `daemon-key-ask-001.md` named the contradiction and recommended the existing fail-closed ddoc contract.
4. Ruling 003 confirmed the ddocs win; reply 002's t4 paragraph is void, while its t3 clarification remains compatible.
5. Local `harness checks` was green on 43fca08 while CI was red because the same SHA received different database URLs.
6. The local harness mints and injects a unique `fs3_test_<epoch>_<entropy>` child URL, which cannot equal `DatabaseConfig::DEFAULT_URL`.
7. CI injected the prod-spelled `127.0.0.1:5433/flowspace3` anchor byte-for-byte, so the new owner guard correctly fired there.
8. The health test now creates its own per-run `FreshDatabase`, writes that child URL into scratch config, stops the daemon, and drops the database.
9. Its non-zero-exit panic now preserves child stdout and stderr; the first CI failure had discarded the useful `FS3-E-PROD-NOT-DESIGNATED` text.
10. My focused crate run found another ordering bug before the full gate: the new prod-owner refusal masked the older unsealed-test refusal.
11. `refuse_a_defaulted_store_under_test` now runs first, retaining its specific `fs3_testkit::sealed` remediation; the existing focused test guards that precedence.
12. The incident writer was dual-stack hostname binding: A took `::1`, B fell through to `127.0.0.1`, and both reached shared-key publication.
13. `localhost` now canonicalizes once, and `StagedAuth::publish` requires a private-field `BoundListener` proof from the successful bind.
14. Next seat: configure prod `[daemon].owner_root` before bounce, never test against default config, and allocate every daemon a per-run database on `:5434`.
