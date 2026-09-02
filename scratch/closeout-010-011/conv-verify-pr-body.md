## Summary

- make canonical `conv:<guid>` reads and exact `ask --conversation` pins ignore cwd-derived scope while preserving explicit `--repo` filters and unpinned ask corpus boundaries
- add `conversation verify --harness <h> --session <id>` / `--pij <seat>` with an exact store aggregate and `FS3-E-QUERY-CONVERSATION-NOT-INDEXED`
- distinguish globally absent conversations from explicit-scope misses; document and render the shipped contracts

## Mutation evidence

`get_conv_cross_worktree` was run before the lookup change and failed with `FS3-E-QUERY-NOT-FOUND` at `read.rs::conversation_window`. After the change it passes for a different worktree in the same repository and a different repository. Reinstating the former unconditional `conversation_in_scope` filter reproduces the red result. This is the backlog-row-101 assertion explicitly reversed by o-prime reply 002.

## Verification

- `cargo test -p fs3-daemon --test conversation_query` — 16 passed
- `cargo test -p fs3-daemon --lib verify_pij_uses_the_legacy_join_and_names_an_rs_miss` — passed
- `cargo test -p fs3-store --test pg_conversations delivery_probe_is_exact_and_reports_the_last_turn_without_loading_turns -- --exact` — passed
- `cargo test -p fs3-cli conversation_verify` — 2 passed
- `cargo test -p fs3-cli --test docs_bundle` — 5 passed
- `cargo run -p fs3-cli -- get --help` — authoritative address and explicit filter contract present
- `cargo run -p fs3-cli -- conversation verify --help` — only identity and daemon flags present
- `harness checks` — green (`2026-09-01T22:55:25.077Z`)

## Contract notes

- exact full GUID / `conv:` selector: ignores `ScopeSource::Cwd`, honors `ScopeSource::Flag`
- short conversation selector: remains scoped for disambiguation
- zero-turn conversation header: not delivered; dedicated code plus `details.turns = 0`
- missing rs seat under `--pij`: legacy-only join error names `pij req-0033`
- verify success: `{guid,address,turns,repo,worktree,last_turn_at}` with no scope inputs

## Assumptions

- consumers treat exit 0 plus `ok:true` as delivered and the dedicated nonzero code as not delivered
- o-prime owns the production bounce and ac-0006/ac-0007 read-backs
- rs seats remain unresolvable through the legacy join until pij req-0033
