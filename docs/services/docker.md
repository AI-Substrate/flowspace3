# docker — build/run loop + cross-platform binary factory

**Status**: phase-2 LANDED (plan 002): paved root surface `docker/` + `harness docker <sub>`; compose stays db-only per ruling `2026-08-26-daemon-native-on-host`. **Owner**: pij-impressive-ox (resident docker).

## What it is

The fs3 build substrate: a pinned build container that compiles and tests fs3 for every target platform from a mac host, with cargo caches on named volumes so source changes never rebuild images and never restart Postgres. The daemon runs natively on the host; docker's job is the cross-platform binary factory plus the db stack.

Paved home: `docker/` (Dockerfile.build, scripts/) · harness verb: `harness docker <up|down|status|logs|exec|build|run|lint>`
Guide: `docs/how/docker.md` · POC lineage: `docs/plans/002-docker-daemon-base/assets/poc/docker/`
Strategy + timings: `docs/plans/002-docker-daemon-base/assets/poc/cross-platform-strategy.md`

## Key decisions and why

- **Build container + run pair; never rebuild images on source change.** Source bind-mounted read-only; registry/target caches live in named volumes (`fs3-poc-cargo-registry`, `fs3-poc-cargo-target`, output `fs3-poc-bin`) with explicit fixed names declared `external: true` in compose so script and compose mount the SAME volumes regardless of project-name prefixing.
- **Toolchain pinned by exact tag** (`rust:1.95.0-slim-trixie`; trixie/GCC-14 because ort-sys's prebuilt onnxruntime needs GCC-13+ libstdc++). Host toolchains are NOT pins (see gotchas).
- **Engine-agnostic**: all scripts honour `FS3_ENGINE` (default `docker`; OrbStack live) and support `DRY_RUN=1` which echoes instead of executing — that is how podman compatibility is proven without a podman host. Compose file is spec-only.
- **Cross-platform matrix** (`FS3_TARGET`): darwin targets build natively on the mac host (Apple SDK licensing forbids Darwin builds in Linux containers); linux x86_64 builds run the SAME image as `--platform linux/amd64` (OrbStack: Rosetta) instead of cross-linkers; windows uses `x86_64-pc-windows-gnu` via mingw-w64 (FOSS, no MSVC SDK EULA); musl targets force full static linking.
- **Reload loop (phase-1 POC) superseded by the daemon-native ruling**: no compose daemon service exists; the db StartedAt invariance was proven in phase 1 and the paved surface never touches the db service.
- **One-shot in-container tests are the paved proof**: `harness docker run` runs `cargo test --workspace` on the compose network with the db reachable at its SHIPPED default address (socat forward), so both positive and negative boot-contract tests stay honest.

## Gotchas (the expensive lessons)

1. **Text-file-busy on binary swap**: overwriting the volume-mapped executable while the daemon runs fails. Always stage (`cp → /out/.staging`) then atomic `mv`. Keep this shape in phase 2.
2. **Never mount a volume over `$CARGO_HOME` itself** in rust images — it shadows `/usr/local/cargo/bin`. Mount `$CARGO_HOME/registry`.
3. **With `--target <triple>`, artifacts land at `target/<triple>/release/`**; without it, `target/release/`. Mixing both silently splits your cache.
4. **musl static needs two flags**: `-C target-feature=+crt-static` alone still yields PIE-dynamic under Debian's musl-gcc; add `-C relocation-model=static`.
5. **Host toolchains are shared infrastructure** (incident 2026-08-26): installing rustup changed the ambient default mid-flight and broke `harness checks` repo-wide. Rule now: pins live in containers only; any host toolchain install must be COMPLETE (rustfmt+clippy included) and must not change defaults. The darwin-x86_64 std target is the one sanctioned reason a pinned rustup exists on the mac.
6. **Rosetta tax**: amd64-container builds are ~2–3× slower than arm64 but need zero extra config; fine for dev, CI will run native.
7. **The image is trixie (GCC 14), not bookworm**: ort-sys ships prebuilt onnxruntime objects needing GCC-13+ libstdc++ symbols (`_M_replace_cold`, `__cxa_call_terminate`); bookworm's GCC 12 cannot link them, whatever linker flags you add.
8. **Link C++ test/binaries with `-C linker=g++`** for linux-gnu targets — a bare `cc` driver never adds libstdc++.
9. **In-container tests see a different world**: store's pg tests need `FS3_TEST_DATABASE_URL=@db:5432`; daemons spawned by tests honour shipped defaults (127.0.0.1:5433), so run.sh forwards that address to the compose db via socat instead of exporting `FS3_DATABASE__URL` — an env override would mask negative tests (boot_contract's unreachable-store case relies on file-config precedence over nothing).
10. **Tests shell out to real tools**: fs3-git needs the `git` binary; doctor probes a container engine (`docker-cli` + best-effort ro socket mount in run.sh). Missing tools = confusing deep failures far from the cause.
11. **Repo pins `channel = "stable"`** (rust-toolchain.toml): inside containers rustup downloads stable per run unless `/usr/local/rustup` is persisted — run.sh/build.sh mount the `fs3-rustup` volume for exactly that.
12. **With `--target <triple>` artifacts land at `target/<triple>/release/`**; without it they land at `target/release/`. Mixing both silently splits your cache.

```bash
harness docker lint                                   # engine-var coverage + compose spec + no docker-only features
harness docker build                                  # flowspace3 single binary, ELF aarch64 (warm ≈1s)
# windows-gnu / musl targets unavailable: ort-sys ships no prebuilt ONNX
# runtime for those triples (observed 2026-08-26; escalated to Jordan)

harness docker run                                    # cargo test --workspace in-container vs compose db → status ok, 364 passed
DRY_RUN=1 FS3_ENGINE=podman ./docker/scripts/build.sh # podman-by-construction dry run
```

POC-era proofs (reload loop with db StartedAt frozen across runs; in-network
/health) live under `docs/plans/002-docker-daemon-base/assets/poc/docker/`
and remain runnable there.

## Code pointers

- `docker/Dockerfile.build` — pinned image (`rust:1.95.0-slim-trixie`) + matrix toolchains (mingw-w64, musl-tools, g++, cmake, git, docker-cli, socat)
- `docker/scripts/build.sh` — `FS3_ENGINE` / `FS3_TARGET` / `DRY_RUN`; per-target cache + staged publish
- `docker/scripts/run.sh` — one-shot in-container runs; compose-network join; socat db forward; ro socket best-effort
- `docker/scripts/stack.sh`, `lint.sh`
- `.harness/extensions/docker/` — the `harness docker <sub>` verb surface
- POC lineage: `docs/plans/002-docker-daemon-base/assets/poc/docker/` + phase-1/phase-2 tasks.dd.json (tk-0201 superseded by ruling note; rest checked)
