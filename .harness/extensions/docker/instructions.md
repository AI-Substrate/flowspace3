# docker — the paved surface

`harness docker <sub>` — every sub shells through to `docker/scripts/*.sh`
(the scripts are the single implementation; this verb is the discoverable
surface). Engine-agnostic: everything honours `FS3_ENGINE` (default `docker`;
OrbStack live, podman by construction) and `DRY_RUN=1`.

| sub | what it does |
| --- | --- |
| `up` / `down` / `status` | compose stack lifecycle (db-only: postgres+pgvector on 127.0.0.1:5433; `down` never deletes volumes) |
| `logs [-- args…]` | compose logs passthrough |
| `exec -- <service> <cmd…>` | exec into a compose service |
| `build` | build fs3-daemon for `FS3_TARGET` inside the pinned build container (linux gnu/musl arm64+x86_64, windows-gnu; darwin targets are refused — those build natively on the mac, Apple SDK licensing) |
| `run [-- cmd…]` | one-shot command in-container, joined to the compose network with `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@db:5432/flowspace3`; default `cargo test --workspace` |
| `lint` | FS3_ENGINE coverage + compose-spec validity + no Docker-exclusive features |

Compose stays DB-ONLY by ruling 2026-08-26-daemon-native-on-host: the fs3
daemon runs natively on the host and never becomes a compose service.

Full guide (gotchas, cache layout, cross-platform strategy):
`docs/how/docker.md`.
