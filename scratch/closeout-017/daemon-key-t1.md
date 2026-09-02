# daemon-key t1 boundary

T1 complete. RED on unmodified main behavior for both normal and `--json` daemon B.

The writer path is pinned: `daemon.url = http://localhost:<ephemeral>` resolves to both loopback families; daemon A bound `::1`, daemon B fell through the occupied IPv6 address and bound `127.0.0.1`, then `boot::serve` called `StagedAuth::publish` and replaced the shared `daemon.key`. The live probe matrix was `v4 old/new=false/true, v6 old/new=true/false` in both modes.

Command: `cargo test -p fs3-cli --test boot_contract another_loopback_address_family -- --nocapture --test-threads=1` with explicit scratch config and the `:5434` test anchor. Result: 2/2 RED. Each case created and cleaned a per-run `FreshDatabase` on `:5434`.

No production config, port, or database was touched. Next: make binding resolve once and return a type-level publication proof.
