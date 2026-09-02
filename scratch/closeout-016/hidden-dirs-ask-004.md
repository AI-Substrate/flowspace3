# hidden-dirs stop-and-ask 004 — remaining mandatory owners/callers

Ask-002 is applied: migration 0024 exists and `crates/store/src/refs.rs`/`roots.rs` are approved. Exact LSP references show T1 still cannot compile within the remaining fence:

1. Adding persisted state to exported `RegisteredWorktree` requires its other constructor in `crates/store/src/read.rs` and a test fixture constructor in `crates/daemon/src/worktrees.rs`.
2. Approved interface fields are owned by `crates/core/src/views/roots.rs` (`RootReport`) and `crates/core/src/views/status.rs` (`status::Root`); serialization fixtures are in `crates/core/src/views/mod.rs`.
3. The packet says public-envelope changes are deferred to o-prime, so these cannot be inferred as implicitly writable.

Please amend the fence to include:

- `crates/store/src/read.rs`
- `crates/daemon/src/worktrees.rs`
- `crates/core/src/views/roots.rs`
- `crates/core/src/views/status.rs`
- `crates/core/src/views/mod.rs`

Migration `0024_worktree_include_hidden.sql` is the only source change so far. No out-of-fence file has been edited.
