# Phase 1 execution log

## tk-0101 — authoritative conversation addresses

Changed explicit `conv:` reads to ignore cwd-derived scope while retaining explicit `--repo` filtering. Split globally absent GUIDs from explicit-scope misses. Exact `ask --conversation` pins now ignore cwd scope; short selectors and explicit repository flags remain scoped. Ask's internal get rejects model-proposed foreign conversation addresses when unpinned.

Evidence:
- RED before source change: `get_conv_cross_worktree` returned `FS3-E-QUERY-NOT-FOUND` (`artifact://42`).
- GREEN: `get_conv_cross_worktree`, `conv_not_found_messages`, `exact_conversation_pins_ignore_cwd_but_honor_explicit_repo_scope`, and `ask_tools_search_and_get_conversations_under_the_same_scope`.
- Ruling: `.harness/temp/agent/conv-verify-prime-reply-002.md` reverses the old scope assertion and expands the file fence.

Discovery — Noteworthy: `ScopeSource` already records whether scope came from `--repo` or cwd, so the cutover requires no new request flag or compatibility path.

## tk-0102 — conversation verify contract

Added an exact-GUID delivery aggregate in `fs3-store`, the dedicated `FS3-E-QUERY-CONVERSATION-NOT-INDEXED` catalog code, daemon verification for native session and legacy pij identities, an authenticated read-only HTTP route, and the unscopable CLI subcommand. Success requires at least one turn and carries `guid`, `address`, `turns`, `repo`, `worktree`, and `last_turn_at`; a zero-turn header is a distinct not-delivered message with `details.turns = 0`.

Evidence:
- `delivery_probe_is_exact_and_reports_the_last_turn_without_loading_turns` passed.
- `conversation_verify_contract` passed, including the HTTP envelope and absent/zero-turn branches.
- `verify_pij_uses_the_legacy_join_and_names_an_rs_miss` passed against fake join rows.
- `cargo test -p fs3-cli conversation_verify` passed and rejects `--repo`/`--path`.

Discovery — Noteworthy: the store needed a dedicated aggregate; using `outline()` would allocate every turn merely to read the final timestamp.

## tk-0103 — docs and help

Updated the bundled read and conversation guides, `get --help`, the verify TTY view, the plan's stale source pointer, and the generated error-code reference. The docs distinguish authoritative addresses, explicit scope misses, zero-turn delivery, and the legacy-only pij join.

Evidence:
- `cargo test -p fs3-cli --test docs_bundle` — 5 passed.
- Local `get --help` and `conversation verify --help` smokes expose the shipped contracts.
- Solo `harness checks` — green at `2026-09-01T22:55:25.077Z`.

Discovery — Noteworthy: the first corrected gate sampled production Postgres during crash recovery and reported schema `absent`; o-prime verified all production counts intact and cleared the solo rerun. Harness row 124 tracks the guard defect.
