# daemon-key t2 boundary

T2 complete.

- `StagedAuth::publish` requires a private-field `BoundListener` proof constructed from an actual Tokio listener.
- Normal and sandbox boot publish only while borrowing that proof, then consume it into HTTP serving.
- `localhost` canonicalizes once to `127.0.0.1`; Tokio no longer falls through to a second loopback family.
- A/B normal + `--json`: 2/2 green; key bytes/mtime unchanged, no temp residue, daemon A still authorized.
- fs3-daemon lib: 172/172 green.
- Mutation receipt: removing canonicalization restores 2/2 RED dual-family clobbers; guard restored and green reconfirmed.

No production surface touched.
