# Cross-platform build strategy — fs3 daemon binaries

**Date**: 2026-08-26 · **Owner**: pij-impressive-ox (resident docker) · **Scope**: POC fence (`assets/poc/docker/`); phase-2 paths still gated · **Verdict**: **strategy ready for phase 2** — every row of the matrix below was built and verified today.

## The matrix

| target | where it builds | how | status |
| --- | --- | --- | --- |
| aarch64-apple-darwin | mac host, natively | pinned rustup toolchain (1.85.0) | ✅ Mach-O arm64 |
| x86_64-apple-darwin | mac host, natively | same toolchain, `--target` | ✅ Mach-O x86_64 |
| aarch64-unknown-linux-gnu | build container, `linux/arm64` | native compile (container arch == target arch) | ✅ ELF; **executed**, serves `/health` 200 |
| aarch64-unknown-linux-musl | build container, `linux/arm64` | musl-tools (host-arch musl-gcc) + forced static | ✅ fully static ELF |
| x86_64-unknown-linux-gnu | build container, `--platform linux/amd64` | native compile inside an amd64 container (OrbStack runs it via Rosetta) | ✅ ELF; **executed** under Rosetta, serves `/health` 200 |
| x86_64-unknown-linux-musl | build container, `--platform linux/amd64` | musl-tools + forced static | ✅ fully static ELF |
| x86_64-pc-windows-gnu | build container, `linux/arm64` | mingw-w64 cross toolchain | ✅ PE32+ exe produced; **file(1)-verified only, never executed** |

## Decisions and why

### macOS targets build NATIVELY on the mac host — never inside Linux containers
Apple's SDK/Xcode license terms do not permit Apple SDK components to be installed or run on non-Apple hardware, so a Linux container can never legally produce a Darwin binary. There is no workaround worth wanting; the mac host *is* the Darwin builder. Both darwin std targets come from one pinned rustup install (see "gotchas" for the shared-host rules this triggered).

### Linux x86_64: same Dockerfile, `--platform linux/amd64`, not cross-linkers
Two ways to produce x86_64 Linux binaries from an arm64 machine:
1. Cross-compilation inside the arm64 container (`gcc-x86-64-linux-gnu` etc.) — works, but every crate with a C dependency needs cross sysroot care, and we'd maintain a second toolchain story.
2. Run the **same** build image as `linux/amd64`; OrbStack executes it via Rosetta (podman would use qemu).

Chose (2). One Dockerfile, one recipe, zero cross-linker configuration, and C-dependency crates just work because everything is native from the container's point of view. Cost: emulated builds are slower (measured below) and need an amd64 image variant (`fs3-poc-build-amd64`). CI will eventually run each platform genuinely natively, where even this cost disappears.

### Windows: x86_64-pc-windows-gnu via mingw-w64 (chosen over msvc/cargo-xwin)

| criterion | gnu/mingw-w64 ✅ | msvc via cargo-xwin |
| --- | --- | --- |
| licensing | pure FOSS (Debian `mingw-w64` package) | downloads Microsoft SDK/CRT bits — EULA surface to manage |
| setup | one apt package, already in `Dockerfile.build` | extra tool (`cargo-xwin`), SDK download at build time, slower cold starts |
| coverage for our deps | fine for std/tokio-class crates (the daemon has no MSVC-specific deps) | only needed for MSVC-only link artifacts or Windows debug tooling |
| determinism/pinning | apt-pinned in the image | SDK version drift managed by xwin |

Pick: **gnu**. Revisit cargo-xwin only if a real dependency demands MSVC linkage or we need PDB-grade Windows debugging.

### musl targets force full static linking
Debian's musl-gcc defaults produce PIE-dynamic binaries (interpreter `/lib/ld-musl-x86_64.so.1`) which defeats musl's purpose. All musl builds therefore set:

```
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static"
```

Verified: both musl outputs report `statically linked`. Note `+crt-static` alone was NOT enough for x86_64 — `-C relocation-model=static` is required too.

### Cache layout
One named target volume (`fs3-poc-cargo-target`) serves every target: cargo isolates artifacts per triple under `target/<triple>/release/`, so caches are per-target by construction. The registry volume is shared across all targets. Output volume gets one subdirectory per triple.

## Timings (measured 2026-08-26, M4 Max / OrbStack)

| measurement | time |
| --- | --- |
| arm64 build image (apt mingw+musl+file, 4× rustup target add), cold | ≈98 s (one-time) |
| amd64 build image variant, cold (incl. Rosetta layers) | ≈110 s (one-time) |
| aarch64-linux-{gnu,musl} warm rebuild (per target) | 0–1 s |
| x86_64-linux-{gnu,musl} warm rebuild under Rosetta (per target) | 2–3 s |
| x86_64-pc-windows-gnu warm rebuild | 1 s |
| mac host darwin builds (either arch, warm) | ≈1 s |

Cold-per-target first compiles were not separately measured because the POC crate is dependency-free; expect real numbers once fs3-daemon brings its dependency tree — re-measure at phase 2.

## Validating on the Windows machine (when it comes into use)

The `.exe` is produce-only until then. When a Windows box is available:
1. Copy `fs3-poc-bin/x86_64-pc-windows-gnu/release/fs3-poc-daemon.exe` over (or `git pull` + run the paved script there).
2. Run it; expect `fs3-poc-daemon listening on 0.0.0.0:8081`.
3. `curl http://127.0.0.1:8081/health` → `200 {"status":"ok"}`.
4. If gnu-runtime issues appear (missing DLLs are unlikely — std-only crate), fall back to the cargo-xwin/MSVC path documented above.
Add the result to this table and flip the open question.

## Open question

Does any future fs3-daemon dependency require MSVC (e.g. windows-rs metadata-driven linking is fine on gnu, but some crates assume `msvc` toolchain)? Unknown until the real daemon surface lands. Watch it during phase 2 integration.

## Phase-2 shape (recommendation)

Promote verbatim into `docker/`:
- `Dockerfile.build` (both platform variants are the same file),
- the target-selection block of `build.sh` (extract into a shared lib sourced by scripts),
- the harness extension gains a `build --target <triple>` verb mapping straight onto `FS3_TARGET`.
