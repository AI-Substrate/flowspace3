# Plan 002 Phase 1 POC results — docker daemon base

**Date**: 2026-08-26 · **Engine**: OrbStack (`docker` 29.4.0, OrbStack runtime) · **Host**: Apple M4 Max (aarch64) · **Scope**: `assets/poc/docker/` only (phase 2 gated)

## Verdict: **GO**

Every phase-2 mechanism is proven working end-to-end from a cold start. No blocker found; one real gotcha surfaced and was fixed in-script (see learnings).

## Timings

| measurement | value | notes |
| --- | --- | --- |
| Build image creation (cold, incl. pulling `rust:1.85.0-slim-bookworm`, ~353 MB) | 42 s wall | one-time; cached afterwards |
| Cold `cargo build --release --locked` in container (zero-dep crate) | 0.19 s | crate has no dependencies yet — cold/warm converge until the real fs3-daemon deps land |
| Incremental rebuild after 1-line source change | 0.20 s cargo / ≈1 s script | no dependency recompiles; registry+target served entirely from named volumes |
| Binary publish into shared output volume | <0.5 s | stage + atomic `mv` inside the volume |
| Stack up (cold, incl. `debian:bookworm-slim` pull + pg healthcheck wait) | 14.7 s | |
| Reload loop (build + daemon-only recreate), steady state | ≈1.1 s | two consecutive runs; Postgres never touched |
| `down` → `up` cycle | ≈15 s | data + vector extension survive via named volume |

## Proofs (done_when)

| assertion | result |
| --- | --- |
| dw-0101 ELF aarch64 built entirely in-container from mac host | ✅ `file`: "ELF 64-bit LSB pie executable, ARM aarch64 … interpreter /lib/ld-linux-aarch64.so.1" |
| dw-0102 incremental rebuild skips dependency recompiles | ✅ only `fs3-poc-daemon` recompiled; caches persisted across engine runs |
| dw-0103 `/health` returns `200 {"status":"ok"}` inside container network | ✅ curl from `fs3-poc-db` to `fs3-poc-daemon:8081` |
| dw-0104 stack up green; host reachability; `CREATE EXTENSION vector` | ✅ host `curl 127.0.0.1:8081/health` → 200; extversion 0.8.2 |
| dw-0105 data survives `down`/`up` via named volume | ✅ probe row + extension present after cycle |
| dw-0106 db `StartedAt` unchanged across two reloads; daemon's changes | ✅ db `02:59:52.89217199Z` both times; daemon advanced `03:01:03` → `03:01:14` |
| dw-0107 lint exit 0; compose config validates; FS3_ENGINE=podman dry-run swaps only the binary invoked | ✅ all four scripts echo `podman …` under `DRY_RUN=1` |
| dw-0108 this writeup exists with timings + go/no-go | ✅ |

## Learnings

1. **Text file busy**: recreating the binary directly onto the bin volume fails while the daemon container executes it (`cp: cannot create regular file '/out/fs3-poc-daemon': Text file busy`). Fix baked into `build.sh`: copy to `/out/.staging`, then `mv -f` — rename replaces the directory entry atomically and works under a live mmap/executable inode. Phase 2 must keep the staged-swap shape.
2. **Never mount a volume over `$CARGO_HOME` itself** in official rust images: it would shadow `/usr/local/cargo/bin`. Mount at `$CARGO_HOME/registry`.
3. **Explicit fixed volume names are load-bearing**: `external: true` + fixed names in compose means `engine run -v fs3-poc-cargo-target` and compose resolve to the same volumes regardless of project-name prefixing. This is what makes the build script and the stack share state.
4. **Zero-rebuild dev loop confirmed**: source is bind-mounted read-only; nothing about a source change touches an image layer. Only the daemon service is recreated.
5. **Compose-spec discipline costs nothing**: the whole file validates with plain `compose config`; no Docker-exclusive feature needed anywhere. Podman compatibility remains by-construction (no podman host here) — the open question stands.

## Recommended phase-2 layout

```text
docker/
  Dockerfile.build          # promote POC verbatim (pin bump as needed)
  Dockerfile.daemon         # optional slim runtime image replacing debian:bookworm-slim base
  scripts/
    build.sh                # promote; keep FS3_ENGINE + DRY_RUN contract
    reload.sh               # promote; keep StartedAt printing
    down.sh                 # promote (never -v on caches)
    lint.sh                 # promote; wire into harness checks later
docker-compose.yml          # EXTEND s001's file with the daemon service; never replace
.harness/extensions/docker/ # verbs shell through to scripts/
```

Verb → script mapping:

| harness verb | script |
| --- | --- |
| stack up | `(FS3_ENGINE) compose -f docker-compose.yml up -d` |
| stack down | `scripts/down.sh` |
| status | `compose ps` (+ health line) |
| build | `scripts/build.sh` |
| daemon restart | `reload.sh` minus build (split out a `restart.sh`) or `reload.sh --skip-build` |
| logs | `compose logs -f daemon` |
| exec | `compose exec` passthrough |
| run-in-build-container | `engine run --rm -v repo:/src:ro -v caches… fs3-build-image …` |

## Open items carried to phase 2

- Live podman verification (needs a podman host) — plan open question unchanged.
- Cold/warm timing separation becomes meaningful only once the real fs3-daemon brings dependency weight; re-measure during phase 2 integration.
- x86_64 CI matrix intentionally untouched.
