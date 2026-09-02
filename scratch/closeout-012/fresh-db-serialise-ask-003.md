# fresh-db-serialise stop-and-ask 003 — architecture drift rejects required Tokio promotion

## STOP receipt

Command, run alone in the plan-012 worktree:

```text
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test \
  cargo run -p fs3-testkit --bin fs3-arch-check
```

Result: exit 1 (`artifact://34`). Exact violation:

```text
fs3-store -> tokio: declared in [dependencies] but the allow-list permits it only in [dev-dependencies]. A dev-only edge that gets promoted ships in the binary; if the promotion is deliberate, change `tokio@dev` to `tokio` in testkit/arch-allowlist.toml and say why in the review.
```

## Assessment

The promotion is deliberate and required: the process-wide mutation boundary now lives in shipped `fs3_store::{create_database, drop_database, drop_database_if_idle}` and uses `tokio::sync::Semaphore`; keeping Tokio dev-only would make the production library fail to compile. The test-only counter/delay has now been moved behind `#[cfg(test)] mod create_test_hook`; it is not the reason Tokio ships.

## Requested ruling

Expand the fence to `crates/testkit/arch-allowlist.toml` and approve replacing the `fs3-store` edge `tokio@dev` with `tokio`, retaining the existing architectural relationship while recording that store-level DDL serialization is runtime behavior. No new crate dependency is introduced; this changes the allowed kind of an existing workspace dependency.

Stopped before the allowlist edit, AC edits, cross-runtime test, focused tests, or probe.
