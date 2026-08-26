# Docker — the paved surface

The repo's docker substrate: an engine-agnostic compose stack (db-only) plus
a pinned build container that compiles and tests fs3 for every target
platform. Every verb shells through to `docker/scripts/*.sh` — read those for
behaviour; this page is the map.

## Stack (db-only)

The compose stack is **postgres+pgvector only** (`127.0.0.1:5433`). The fs3
daemon runs **natively on the host** (ruling 2026-08-26-daemon-native-on-host)
and must never become a compose service — don't add one.

```bash
harness docker up        # or: docker/scripts/stack.sh up
harness docker status
harness docker down      # never deletes volumes
```

## Cross-platform builds

```bash
harness docker build                                   # FS3_TARGET default: aarch64-unknown-linux-gnu
FS3_TARGET=x86_64-unknown-linux-musl harness docker build
FS3_TARGET=x86_64-pc-windows-gnu harness docker build   # PE32+ exe, produce-only
```

- Targets: `aarch64|x86_64-unknown-linux-{gnu,musl}`, `x86_64-pc-windows-gnu`.
- **Darwin targets are refused here** — Apple SDK licensing means macOS
  binaries build NATIVELY on the mac host, never inside Linux containers.
- x86_64 linux targets run the same image as `--platform linux/amd64`
  (OrbStack: Rosetta). One Dockerfile, zero cross-linkers.
- Caches: named volumes `fs3-cargo-{registry,target}` + output volume
  `fs3-bin`; cargo separates artifacts per triple automatically.
- musl outputs are fully static (`+crt-static` + `-C relocation-model=static`
  — Debian's musl-gcc needs both).

Full strategy + timings:
`docs/plans/002-docker-daemon-base/assets/poc/cross-platform-strategy.md`.

## One-shot in-container runs / tests

```bash
harness docker run                    # = cargo test --workspace in-container
harness docker run -- cargo clippy --workspace --all-targets
```

The container joins the compose network and exports
`FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@db:5432/flowspace3`.
That override is mandatory for store's pg tests: their shipped default points
at `127.0.0.1:5433`, which from inside the container is the container itself.

## Engine agnosticism

Everything honours `FS3_ENGINE` (default `docker`; OrbStack live, podman by
construction) and supports `DRY_RUN=1` to echo commands without executing.
Prove it any time:

```bash
harness docker lint                   # exit 0 = spec-valid + no docker-only features
DRY_RUN=1 FS3_ENGINE=podman docker/scripts/build.sh
```

## Gotchas that cost us something

- Binary swap onto a live-mounted volume: stage (`cp` → `.staging`) then
  atomic `mv`, or you get `Text file busy`.
- Mount cache volumes at `$CARGO_HOME/registry`, never `$CARGO_HOME` itself
  (it would shadow `/usr/local/cargo/bin`).
- With `--target <triple>`, artifacts land in `target/<triple>/release/`.

Pointers: `docker/Dockerfile.build`, `docker/scripts/*`,
`.harness/extensions/docker/extension.ts`,
POC lineage under `docs/plans/002-docker-daemon-base/assets/poc/docker/`.
