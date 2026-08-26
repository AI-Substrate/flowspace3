# daemon-shell

A prototype of the host-native half of the fs3 daemon: a `notify` file watcher
and a loopback-only `axum` web service, in one process, with runtime-addable
watch roots.

**This is a learning vehicle, not shipped code.** Read [LEARNINGS.md](LEARNINGS.md)
before copying anything out of it — several of the decisions here exist to be
argued with, and one of them (the inotify recursive-watch race) changes what
the real daemon has to do.

It is deliberately **not** a member of the flowspace3 workspace: its
`Cargo.toml` carries an empty `[workspace]` table, so it has its own lockfile
and its own `target/`, and `fs3-arch-check` never sees it.

## Run it

```bash
cd pocs/daemon-shell
cargo run -- --port 7474 --debounce-ms 10000 --watch /path/to/repo
```

| flag | default | meaning |
| --- | --- | --- |
| `--port` | `7474` | TCP port. `0` asks the OS for a free one and prints it. |
| `--bind` | `127.0.0.1` | Loopback only. A non-loopback address is a startup failure. |
| `--debounce-ms` | `10000` | Quiet period before a path is called dirty. fs3's default. |
| `--watch DIR` | none | Roots to watch from startup. Repeatable, and optional — roots can be added over HTTP. |

`RUST_LOG=daemon_shell=debug` logs every raw event and what the debouncer did
with it.

## The surface

| method | path | answers |
| --- | --- | --- |
| `GET` | `/health` | `{"status":"ok","version":"0.1.0"}` |
| `GET` | `/status` | uptime, debounce, sweep interval, per-root event/pending/dirty counts |
| `POST` | `/watch` | `{"path":"/abs/dir"}` → `201 {"root": "<canonical>"}`, `409` on overlap, `400` on a bad path |
| `DELETE` | `/watch` | `{"path":"/abs/dir"}` → `200`, or `404` if it was not watched |
| `GET` | `/dirty` | the debounced dirty set — **idempotent**, reading does not consume |
| `DELETE` | `/dirty` | the acknowledgement: empties the set, returns how many were taken |

```bash
curl -s localhost:7474/health
curl -s -XPOST localhost:7474/watch -H 'content-type: application/json' -d '{"path":"'$PWD'"}'
curl -s localhost:7474/status | jq
curl -s localhost:7474/dirty | jq
curl -s -XDELETE localhost:7474/dirty
```

## Layout

```
src/core.rs      pure debounce + dirty-set logic, 15 unit tests, no I/O and no clock
src/watcher.rs   the notify shell: OS watchers, the one monotonic clock, the sweep
src/http.rs      the axum shell: routes and wire types
src/lib.rs       wiring; src/main.rs: CLI and the loopback guard
tests/           10 end-to-end tests over real HTTP against a real OS watcher
```

The split is the point. Everything that DECIDES anything is in `core.rs` and is
tested in microseconds; `watcher.rs` and `http.rs` only carry values between the
OS and the core.

## Proving it

```bash
cargo test                                    # 25 tests, ~5s
/opt/homebrew/bin/cargo-clippy --all-targets -- -D warnings
/opt/homebrew/bin/cargo-fmt --all -- --check
```

> On this machine plain `cargo clippy` / `cargo fmt` resolve to rustup shims in
> `~/.cargo/bin` for a toolchain that has neither component installed. Call the
> Homebrew binaries directly. (Recorded as harness observations DL-006/DL-007.)

Cross-platform, as actually run:

```bash
# Linux — real inotify backend, full suite
docker run --rm -v "$PWD:/src:ro" -w /work rust:1-bookworm \
  bash -c 'cp -r /src/. /work/ && rm -rf /work/target && cargo test'

# Windows — compile only, and in its OWN target dir so host artifacts survive
RUSTUP_TOOLCHAIN=1.85.0 CARGO_TARGET_DIR="$PWD/target/cross" \
  ~/.cargo/bin/cargo check --lib --bins --target x86_64-pc-windows-msvc
```
