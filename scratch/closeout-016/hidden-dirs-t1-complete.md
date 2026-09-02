# hidden-dirs t1 complete

Shipped migration 0024, persisted worktree `include_hidden`, optional `/roots` request semantics, additive `RootReport.include_hidden`, and CLI `--include-hidden` / `--no-include-hidden`.

Proof: store, daemon, CLI, and core round-trip targeted tests PASS on the dedicated `:5434` test postmaster. Re-add without a flag preserved true; explicit false cleared it. Mutation forcing the column write to false made the store test RED at the expected assertion; restoration returned PASS.

DDoc receipts: `t1`, `dw-3101`, `bp-0001`, and `ac-0001` are checked and receipted via global `ddocs`.

Next: t2 threads `global OR per-root` into root scans and watcher rescans with hidden/default/.git controls.
