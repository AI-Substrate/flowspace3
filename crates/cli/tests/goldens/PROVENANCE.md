# PROVENANCE — the ac-0002 byte-goldens

These files are the mechanical guarantee behind plan 007's central promise: a
human-readable output layer is added, and **the bytes an agent sees do not
move**. They are asserted by
`crates/cli/tests/envelope_goldens.rs::the_piped_envelope_is_byte_identical_to_pre_plan_main`.

## Who minted them, and why that is the whole point

| Fact | Value |
|---|---|
| Captured from | `main` at `1ce572bf3af14e7db2ddbfd642c61557bc67458d` — the commit **before** plan 007 existed |
| Built in | `/Users/jordanknight/substrate/flowspace/fs3-hi-goldenbase` (a detached worktree of that sha, deleted after capture) |
| Binary | `target/debug/flowspace3`, `fs3-cli v0.4.0` |
| Toolchain | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`, `rustc 1.95.0 (59807616e 2026-04-14)` (Homebrew), host `aarch64-apple-darwin` |
| Captured on | 2026-08-28 by `pij-near-carp` (PM, plan 007), tk-a103 |
| Capture command | `FS3_GOLDEN_BIN=…/fs3-hi-goldenbase/target/debug/flowspace3 FS3_GOLDEN_UPDATE=1 cargo test -p fs3-cli --test envelope_goldens` |

The branch tree at capture time was byte-identical to `1ce572b` under `crates/`,
`Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` (`git diff --stat
1ce572b..HEAD -- crates/ …` was empty; only `docs/plans/007-*` differed), so the
harness and the witness disagree about nothing except which binary ran.

**A capture is not a fix.** If the test goes red, the agent-facing envelope
moved. That is a product decision for o-prime — never a golden to re-take. The
harness refuses to let a `FS3_GOLDEN_UPDATE` run report success for exactly this
reason.

## Why the daemon's answer is a file rather than a database

`ac-0002` says "identical store state". A live store cannot deliver that
reproducibly: `worktree_id` is a serial (`crates/daemon/src/roots.rs:48-60`),
queue counts move while the runner drains (`crates/daemon/src/status.rs:83-96`),
`last_error` is `ORDER BY updated_at DESC` (`crates/store/src/jobs.rs:479-486`),
and root paths are whatever the host registered. Seeding a database would make
the goldens depend on Postgres, on migrations, and on embedding tie-ordering —
none of which is the thing plan 007 changes.

So the daemon's ANSWER is frozen instead, and the harness exercises the surface
the plan actually touches: HTTP body → `DaemonClient::envelope` → `Envelope` →
`emit` → stdout. Determinism beats fidelity for a byte-witness (o-prime ruling
C, 2026-08-28).

## The frozen answers (`responses/`)

Real captures came from the live daemon at `http://127.0.0.1:7373` on
2026-08-28 via `curl`, unedited. Synthetic bodies are hand-built from the
daemon's own types and its own `next_action` generators — cited per row — for
the verbs that MUTATE, which must never be fired at a live index to make a
fixture.

| file | origin | modelled on |
|---|---|---|
| `status.json` | real capture — `GET /status` | — |
| `search.json` | real capture — `GET /search?q=render the envelope for a human&limit=3` | — |
| `get.json` | real capture — `GET /get?address=el:…/crates/cli/src/main.rs::emit` | — |
| `tree.json` | real capture — `GET /tree?limit=3` | — |
| `conversation-list.json` | real capture — `GET /conversations` (empty index) | — |
| `error-not-found.json` | real capture — `GET /get?address=el:git:nope/nope::nope` | `FS3-E-QUERY-NOT-FOUND` |
| `error-query-empty.json` | real capture — `GET /search?q=` | `FS3-E-QUERY-INVALID` |
| `add.json` | synthetic | `RootReport` (`crates/daemon/src/roots.rs:48-60`) + `next_after_scan` (`crates/daemon/src/http.rs:381-394`) |
| `scan.json` | synthetic | same, the `enqueued == 0` branch |
| `remove.json` | synthetic | `RemoveReport` (`crates/daemon/src/remove.rs:69-105`) + `next_after_remove` (`:132-146`) |
| `gc.json` | synthetic | `GcCounts` (`crates/daemon/src/remove.rs`) + `next_after_gc` (`:164-173`) |
| `messages.json` | synthetic | a `status` answer carrying a PRD req 59 `UserMessage` (`crates/core/src/messages.rs:86-104`, severity spelling `:42-52`) |

## Checksums

`responses/` (sha256, bytes):

| file | sha256 | bytes |
|---|---|---|
| `add.json` | 050946759e4313ec2733c34d1407315a2459fa38ee0cf73cb97666addf4ec637 | 509 |
| `conversation-list.json` | 00a271195cc5340f221ad3af6edb27c0c2dc92b4d5ccc546f1fba6446fc84c9e | 178 |
| `error-not-found.json` | fc53a317ed71d94de96822fdbf145af87930443d9a08b140f3e36663f8eaf4e4 | 444 |
| `error-query-empty.json` | 76c73619e6c2a3d5452745c315fa26e164426b38ffb07d4d4a6e05fa8dd6fee0 | 273 |
| `gc.json` | 9647083ebdc816a583fdb9b4632bfa1f9effee1e9843ccb4ba04d9503c3894ba | 231 |
| `get.json` | d0ab84388c66b219dc1259773475badd48d25130131071cb571fa4c9e4ed903b | 1663 |
| `messages.json` | 04e9e575a2ee9e8eccf36c1c1c163bc540259ce15f8068267a69a0aa576ea5e6 | 589 |
| `remove.json` | 663165af6ce8a45fff755d0019b994d3cb9c13fa60e47d02e781feb12d20e70d | 390 |
| `scan.json` | e32b0617403b0ef604579fc9df6f6c894fd5a7547e9e6fb168af02065a8d3b6f | 370 |
| `search.json` | ddbb73753fe6378b4badd4818450cdb962db20796a412652842708aae303920d | 2719 |
| `status.json` | 6d0d35abdc7a44a63d0a84cabb3e0a7ea0b4ef630c4ea00fad03fc19ca9ba4a6 | 1362 |
| `tree.json` | 0b76ffa21ec30d9eeb91a75b12ec5d5070327db3567c9f4bc696c84db0a1ac3b | 681 |

`stdout/` — what the pre-plan binary printed (sha256, bytes):

| file | sha256 | bytes |
|---|---|---|
| `add.stdout` | 515c8ff8dc8c630295bbc879e5ed2216c07c8d9afa72965e0fdae8673a5d73d2 | 715 |
| `agents-start-here.stdout` | d9470b33a78e9d2d0d2a19ef02364cc4fdfe2cc0f363b192361aaeea009e2229 | 8869 |
| `conversation-list.stdout` | 9cbcb689420fc78a7582cd9c6151f5872b5d6a4696c1fcfa2303dc533ba7b6d7 | 209 |
| `docs-list.stdout` | b6228fa474489331ccecaf6f2e7a28e74dca5400d7a8af4a018bc9fcd2d9f0a7 | 1453 |
| `error-not-found.stdout` | d9e3cfeb5db8aa52305d41c3cdc24c357ae3104d73645454ed1241371fdf9c8c | 550 |
| `error-query-empty.stdout` | d68d4fc9a63ba0e11c94c38a0b5c07731bba4d9b0388f356096f3b7922284d79 | 352 |
| `gc.stdout` | 001540d282423df75a03184c463b49c1144cbf7d27ef669a0e7e13d06b845789 | 285 |
| `get.stdout` | 5e82aa9be1fd74338d80343c0ff4a2fb8d991b9a92053de82898fd70915af034 | 1947 |
| `remove.stdout` | 2e8cddd12da21cd60e26e60993468127cbf4f59a0beea4aec19d96ce52959519 | 501 |
| `scan.stdout` | 87b0a90b3932eed3670acefb052fc8582f2b2ccf762679fe6f3e46723bedaf48 | 454 |
| `search.stdout` | 28d39b292a0cbb61b361a0e50f196480a3f6b278d7c54104cd4804464163ac2e | 3348 |
| `status-with-messages.stdout` | eb930d456f238760a3a51dc295195321bf54591a6f6158322f39cc414059634e | 810 |
| `status.stdout` | c16ac802fe51f138629a3d6bef5a2879fd97397e5598b8aa396b4b3fac6d6c62 | 2073 |
| `tree.stdout` | fe11591da0c30ad99af25eb19d2eb9633323508e0579e5d0c12e5d4e1d3c2278 | 943 |
| `usage-error-prints-no-envelope.stdout` | e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 | 0 |

The last one is empty ON PURPOSE, and it is the golden most likely to catch a
renderer mistake: a usage problem is answered by clap before an envelope exists,
so stdout carries nothing and the exit code is 2. A human layer that starts
printing prose there breaks every `flowspace3 … | jq` that currently sees clean
end-of-stream.

## Not covered, and why

| verb | why not | what covers it instead |
|---|---|---|
| `doctor` | `Step.elapsed_ms` comes from a live `Instant` (`crates/cli/src/doctor.rs:60-94`), and the walk's findings depend on the host's daemon, database and providers — there is no byte-stable form to freeze | ac-0001's four-case output matrix; a doctor golden would need a contract change (a fixed-clock seam), which is a bigger decision than plan 007 |
| `ping` | prints prose, not an envelope (`crates/cli/src/main.rs:627-647`) | `crates/cli/tests/ping.rs` |
| `config show` | prints a rendered config table, no envelope (`crates/cli/src/main.rs:608-625`) | `crates/cli/src/show.rs` unit tests |
| daemon-unreachable failure | the message embeds an ephemeral port and an OS-specific transport string, so it is not byte-stable across machines | `crates/cli/tests/ping.rs` asserts the shape |
| `conversation import`, `conversation remove`, `docs get`, `doctor install-skill`, `doctor upgrade` | same output path as the covered verbs, no additional surface | the covered cases |
