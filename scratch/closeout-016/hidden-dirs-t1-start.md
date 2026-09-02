# hidden-dirs t1 start

Status: started after o-prime GO.

Scope: migration 0024; store read/write; `POST /roots` optional request field; resolved `RootReport.include_hidden`; CLI `add --include-hidden/--no-include-hidden`; tests proving true/preserve/explicit-false semantics.

Pre-test database receipt: `/opt/homebrew/opt/libpq/bin/psql postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test -c "select 1"` returned one row with value `1`.

Known fence issue remains recorded in `hidden-dirs-ask-002.md`: the named `crates/store/src/worktrees.rs` does not exist and the actual persistence symbol is in `crates/store/src/refs.rs`. I will complete all reachable in-fence t1 work without modifying unfenced source pending the amendment.
