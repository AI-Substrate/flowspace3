# Install and first run

One binary: `flowspace3`. The daemon is a subcommand of it, not a second
artifact — one file to install, one version, and no way for a CLI and a daemon
of different vintages to meet.

## Build from source

```bash
cargo build --release -p fs3-cli    # produces target/release/flowspace3
```

Needs a Rust toolchain (1.95+, edition 2024) and a container engine (Docker,
OrbStack, or Podman via `FS3_ENGINE=podman`).

## First run

```bash
flowspace3 doctor        # starts the stack, creates the database, migrates it
flowspace3 daemon &      # the indexer
flowspace3 add .         # index the current directory
flowspace3 status        # poll until the queue is empty
flowspace3 search "how does the queue work"
```

`doctor` walks engine -> stack -> database -> schema and REPAIRS what it can as
it goes. You do not run `docker compose` yourself, and there is no second
command to apply migrations.

No configuration is required. The default provider is `fake`: a deterministic,
offline, keyless embedder and summarizer that produces real vectors, so the
whole stack works — including search — before you have any API keys.

## What gets written where

- Configuration: `~/.config/flowspace3/config.toml` (override the directory
  with `FS3_CONFIG_DIR`).
- Secrets: `~/.config/flowspace3/secrets.env`, loaded into the environment at
  startup. Config files name variables; they never hold key values.
- Data: Postgres, on `127.0.0.1:5433` by default.
- The repositories you index: **nothing**. fs3 writes no files into them.

## Verifying an install

```bash
flowspace3 doctor        # every row should be ok or repaired, verdict ok
flowspace3 ping          # the daemon answers with its version
```

`doctor`'s verdict is `degraded` when the store is fine but the daemon is not
running — that is the normal state before you start one.
