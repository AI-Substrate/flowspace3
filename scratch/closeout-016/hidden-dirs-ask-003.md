# hidden-dirs stop-and-ask 003 — required contract files are outside fence

T1 source mapping found two additional mandatory paths outside the ruled fence:

- `crates/store/src/refs.rs`: owns `RegisteredWorktree`, `register_worktree`, `list_worktrees`, and `find_worktree`; the fenced `crates/store/src/worktrees.rs` does not exist.
- `crates/core/src/views/roots.rs` and `crates/core/src/views/status.rs`: own public `RootReport` and status `Root`; both must gain `include_hidden` to meet the approved interface. `crates/core/src/views/mod.rs` contains their serialization round-trip fixtures.

`crates/store/src/lib.rs` may remain untouched if existing re-exports cover modified symbols. Store tests already fall within the fence.

Please amend the fence to include `crates/store/src/refs.rs` and `crates/core/src/views/{roots,status,mod}.rs`. The packet explicitly forbids unilateral out-of-fence/public-envelope changes, so I have not edited source. T1 is started and the dedicated `:5434` database probe returned `select 1 = 1`.
