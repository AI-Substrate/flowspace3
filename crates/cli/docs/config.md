# Configuration

All configuration is files in `~/.config/flowspace3/`. Nothing is stored in the
database, and nothing is written into the repositories you index. Override the
directory with `FS3_CONFIG_DIR`.

```bash
flowspace3 config show      # the effective values, and which layer each came from
```

An absent config file is a working system: every value has a default, and the
defaults run the whole stack offline with no keys.

## The layers, lowest first

1. **Defaults** compiled into the binary.
2. **`config.toml`** in the config directory.
3. **`FS3_*` environment variables**, `__` for nesting:
   `FS3_DATABASE__URL=…`, `FS3_EMBEDDER__ACTIVE=fake`.

An `FS3_` variable WITHOUT `__` is not a config override — that is what keeps
the override namespace off the secrets namespace. A name that does nest but
matches no real key is a startup failure, because an override that silently
does nothing is worse than a refusal.

`flowspace3 config show` prints which layer won for each section, which is the
fastest way to answer "why is it using that value".

## The shape

```toml
[daemon]
url = "http://127.0.0.1:7373"      # loopback only — enforced, not advised

[database]
url = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"

[providers.fake]
kind = "fake"

[embedder]
active = "fake"

[summarizer]
active = "fake"

[indexing]
summary_min_lines = 10             # size floor for per-element summaries
debounce_seconds = 10              # how long a dirty file must settle
worker_concurrency = 4             # jobs claimed at once

[scan]
max_file_bytes = 2000000
min_file_bytes = 1
respect_gitignore = true
include_hidden = false
follow_symlinks = false
```

## Secrets are a separate chain

Config files name the ENVIRONMENT VARIABLE that holds a key
(`api_key_env = "OPENAI_API_KEY"`), never the key. Values come from the process
environment or from `secrets.env` in the same directory, which is loaded into
the environment at startup. A variable already set is never overwritten, and a
secret is never logged or printed — `config show` masks the database password
and reports a key variable as set or not set.

## Per-repo overrides

```toml
[repos."git:github.com/AI-Substrate/flowspace3"]
summarizer = "fake"
```

Keyed by repo identity, so a monorepo of Rust and a repository of prose can use
different models without a second config file. A repo that names nothing gets
the active selection.

See `flowspace3 docs get providers` for what can go in the registry.
