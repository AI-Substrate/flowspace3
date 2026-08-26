# docker — build/run loop + cross-platform binary factory

**Status**: phase-1 POC proven and landed (plan 002); phase-2 integration gated on the s001 fence. **Owner**: pij-impressive-ox (resident docker).

## What it is

The fs3 build substrate: a pinned build container that compiles the daemon for every target platform from a mac host, with cargo caches on named volumes so source changes never rebuild images and never restart Postgres. Phase-2 will re-scope per ruling `2026-08-26-daemon-native-on-host`: compose stays db-only permanently, the daemon runs natively on the host, and this machinery becomes the cross-platform binary factory for it.

POC home: `docs/plans/002-docker-daemon-base/assets/poc/docker/`
Strategy: `docs/plans/002-docker-daemon-base/assets/poc/cross-platform-strategy.md`
Results + timings: `docs/plans/002-docker-daemon-base/assets/poc/docker-results.md`

## Key decisions and why

- **Build container + run pair; never rebuild images on source change.** Source bind-mounted read-only; registry/target caches live in named volumes (`fs3-poc-cargo-registry`, `fs3-poc-cargo-target`, output `fs3-poc-bin`) with explicit fixed names declared `external: true` in compose so script and compose mount the SAME volumes regardless of project-name prefixing.
- **Toolchain pinned by exact tag** (`rust:1.85.0-slim-bookworm`). Host toolchains are NOT pins (see gotchas).
- **Engine-agnostic**: all scripts honour `FS3_ENGINE` (default `docker`; OrbStack live) and support `DRY_RUN=1` which echoes instead of executing — that is how podman compatibility is proven without a podman host. Compose file is spec-only.
- **Cross-platform matrix** (`FS3_TARGET`): darwin targets build natively on the mac host (Apple SDK licensing forbids Darwin builds in Linux containers); linux x86_64 builds run the SAME image as `--platform linux/amd64` (OrbStack: Rosetta) instead of cross-linkers; windows uses `x86_64-pc-windows-gnu` via mingw-w64 (FOSS, no MSVC SDK EULA); musl targets force full static linking.
- **Reload loop is db-safe**: rebuild then `up -d --no-deps --force-recreate daemon` only; proved via unchanged db `StartedAt` across consecutive reloads.

## Gotchas (the expensive lessons)

1. **Text-file-busy on binary swap**: overwriting the volume-mapped executable while the daemon runs fails. Always stage (`cp → /out/.staging`) then atomic `mv`. Keep this shape in phase 2.
2. **Never mount a volume over `$CARGO_HOME` itself** in rust images — it shadows `/usr/local/cargo/bin`. Mount `$CARGO_HOME/registry`.
3. **With `--target <triple>`, artifacts land at `target/<triple>/release/`**, not `target/release/<triple>/`.
4. **musl static needs two flags**: `-C target-feature=+crt-static` alone still yields PIE-dynamic under Debian's musl-gcc; add `-C relocation-model=static`.
5. **Host toolchains are shared infrastructure** (incident 2026-08-26): installing rustup changed the ambient default mid-flight and broke `harness checks` repo-wide. Rule now: pins live in containers only; any host toolchain install must be COMPLETE (rustfmt+clippy included) and must not change defaults. The darwin-x86_64 std target is the one sanctioned reason a pinned rustup exists on the mac.
6. **Rosetta tax**: amd64-container builds are ~2–3× slower than arm64 but need zero extra config; fine for dev, CI will run native.

## How to verify (exact commands)

```bash
cd docs/plans/002-docker-daemon-base/assets/poc/docker

./lint.sh                                  # engine-var coverage + compose spec + docker-only lint; exit 0
FS3_TARGET=aarch64-unknown-linux-gnu ./build.sh   # warm build ≈1s
FS3_TARGET=x86_64-pc-windows-gnu ./build.sh       # PE32+ exe, produce-only

# execute the linux/arm64 binary:
docker run --rm -v fs3-poc-bin:/bins:ro debian:bookworm-slim bash -c \
  '/bins/aarch64-unknown-linux-gnu/release/fs3-poc-daemon & sleep 1; \
   exec 3<>/dev/tcp/127.0.0.1/8081 && printf "GET /health HTTP/1.0\r\n\r\n" >&3 && cat <&3'
# → HTTP/1.1 200 OK … {"status":"ok"}

# db-safe reload proof (db StartedAt frozen across runs):
docker compose --project-name fs3-poc -f compose.yaml up -d
./reload.sh && ./reload.sh                 # compare StartedAt lines

# podman-by-construction dry run:
DRY_RUN=1 FS3_ENGINE=podman ./build.sh
```

## Code pointers

- `assets/poc/docker/Dockerfile.build` — pinned image + matrix toolchains (mingw-w64, musl-tools, pre-added std targets)
- `assets/poc/docker/build.sh` — `FS3_ENGINE` / `FS3_TARGET` / `DRY_RUN`; per-target cache + staged publish
- `assets/poc/docker/reload.sh`, `down.sh` (never deletes volumes), `lint.sh`
- `assets/poc/docker/compose.yaml` — POC stack (db 5434, daemon 8081), external cache volumes
- `assets/poc/docker/daemon/` — throwaway zero-dep `/health` crate
- ddocs state: `assets/tasks/phase-1/tasks.dd.json` (all tk/dw checked)
