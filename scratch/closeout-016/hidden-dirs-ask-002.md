# hidden-dirs stop-and-ask 002 — store fence path does not exist

The ruled fence assigns `crates/store/src/worktrees.rs`, but the worktree has no such file. The store source directory contains `refs.rs` and `roots.rs`; Flowspace resolves `register_worktree` to `crates/store/src/refs.rs`, which is outside the declared fence.

T1 cannot persist/read `include_hidden` without changing the actual worktree registration/listing implementation. Please amend the fence to include the exact store files I may edit (likely `crates/store/src/refs.rs` and any list/read owner such as `crates/store/src/roots.rs`), or identify another intended path. No source code or ddoc state has been changed.
