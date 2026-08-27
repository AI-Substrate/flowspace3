# Configuration reference

Every option `flowspace3` reads, with its type, default, effect, and the
environment variable that overrides it (PRD req 58).

This page is the exhaustive list. [`docs/how/configuration.md`](../how/configuration.md)
is the narrative — where config lives, how the layers merge, how secrets stay
out of files, and how to add a new option.

## How to read it

- **Layers**, lowest precedence first: serde defaults → `~/.config/flowspace3/config.toml`
  → `FS3_*` environment variables. `FS3_CONFIG_DIR` moves the directory.
- **Env override** is `FS3_` + section + `__` + key, upper-cased:
  `FS3_DATABASE__URL`, `FS3_SCAN__MAX_FILE_BYTES`. Every key lives in a section,
  so an `FS3_` name with no `__` is not a config override at all — which is what
  keeps the override namespace clear of the secrets namespace.
- A nested name that matches **no** key is a startup failure, not a silent
  no-op. A typo you cannot see is worse than one that stops you.
- Values are typed against the defaults: an integer key parses as an integer, and
  a value of the wrong shape is refused with the line to write.
- Secret **values** never appear here or in `config.toml`. Config names the
  environment variable that holds a key (`api_key_env`); the variable comes from
  the process environment or from `secrets.env` beside the config file.
- `flowspace3 config show` prints the effective configuration, which layer each
  section came from, and whether each named key variable is set.

## `[daemon]`

| Key | Type | Default | Effect | Env override |
|---|---|---|---|---|
| `url` | string | `http://127.0.0.1:7373` | Base URL the daemon serves on and the CLI calls. Must be `http://` or `https://`. The daemon additionally refuses to BIND anything that is not loopback: the surface is unauthenticated and fronts an index of every repo on the machine (PRD req 17). | `FS3_DAEMON__URL` |
| `log_dir` | string | `~/.local/state/flowspace3/logs` | Directory the daemon writes its rolling log file into. A leading `~/` is your home; anything else is taken as given. `~/.local/state` is the XDG state home, and the tilde-relative shape is the same convention `~/.config/flowspace3` already uses — fs3 deliberately has ONE path convention rather than a platform-dirs crate for logs and a hand-rolled path for config. | `FS3_DAEMON__LOG_DIR` |
| `log_level` | string | `fs3_daemon=info,tower_http=info` | `EnvFilter` directives for both destinations (file and stdout). `RUST_LOG` still wins when set, so debugging one run never means editing a config file. | `FS3_DAEMON__LOG_LEVEL` |
| `log_max_bytes` | integer | `8000000` | Roll the active log file once it passes this many bytes. 8 MB keeps an incident's context in one file that still opens in an editor. Must be greater than zero. | `FS3_DAEMON__LOG_MAX_BYTES` |
| `log_max_files` | integer | `5` | How many log files to keep, the active one included — `flowspace3.log`, then `flowspace3.log.1` … `flowspace3.log.4`, oldest highest. The oldest is DELETED on each roll, so `log_max_bytes × log_max_files` (40 MB by default) is a hard ceiling on disk. Must be at least 1; a cap of 1 keeps no history. | `FS3_DAEMON__LOG_MAX_FILES` |

The daemon logs to the file **and** to stdout. The file is never coloured, and
stdout is coloured only when it is a terminal, so a redirected run produces a
clean file rather than one full of escape sequences. Panics — including panics
inside spawned tasks — are routed through the log, which is the whole reason
the file exists: on 2026-08-27 a worker lane died and the only copy of the
evidence was a terminal's scrollback.

A log directory that cannot be written is not a startup failure. The daemon
logs to stdout alone, says so on its first line, and raises a user message;
`flowspace3 doctor` names the active path in its `logs` row.

## `[database]`

| Key | Type | Default | Effect | Env override |
|---|---|---|---|---|
| `url` | string | `postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3` | The central Postgres + pgvector store (PRD req 4). Must be a `postgres://` or `postgresql://` URL. Port 5433 matches the bundled compose stack and stays off 5432 so a machine-local Postgres is never shadowed. | `FS3_DATABASE__URL` |

## `[providers.<name>]` — the registry

Any number of named instances. Declaring one costs nothing: only instances a
port or a repo actually names are ever constructed, so a provider you have no
key for is free to leave configured.

`kind` selects the shape; the remaining keys belong to that shape.

### `kind = "fake"`

No further keys. The deterministic offline stand-in, and a first-class
production value rather than a test hook — it is what makes the whole stack,
search included, work before anyone has an API key. Every fresh machine has one
named `fake`, and it may be redefined.

### `kind = "openai"`

| Key | Type | Default | Effect |
|---|---|---|---|
| `model` | string | *required* | Model name sent to the API. |
| `api_base` | string | *unset* | Override the API base — an Azure-compatible gateway, a LAN server, anything that speaks the OpenAI shape. |
| `api_key_env` | string | `OPENAI_API_KEY` | The **name** of the environment variable holding the key. Never the key. |

### `kind = "azure_openai"`

One instance per DEPLOYMENT, not per resource: Azure names the model by a
deployment in the URL path, and the chat and embeddings deployments are
different names with different `api-version`s in practice.

| Key | Type | Default | Effect |
|---|---|---|---|
| `endpoint` | string | *required* | Resource root, e.g. `https://luna.openai.azure.com`. |
| `deployment` | string | *required* | The deployment name — **not** the model name. Getting this wrong is a 404 that reads like a wrong URL. |
| `api_version` | string | *required* | Azure pins behaviour to this string, and it differs per route. |
| `api_key_env` | string | *unset* | The name of the variable holding the api-key. **Absent means Entra** (managed identity, then `az login`), which is the only way into a resource with key auth disabled. |
| `dimensions` | integer | *unset* | Requested embedding width, verified against the response. Embeddings only; the summarizer ignores it. |

Provider keys are not reachable through `FS3_*` overrides as a matter of shape:
an instance is a tagged enum, and the environment layer coerces against the
defaults, which contain only the `fake` arm. Configure providers in the file.

## `[embedder]` and `[summarizer]` — the ports

| Key | Type | Default | Effect | Env override |
|---|---|---|---|---|
| `active` | string | `fake` | The name of an instance in `[providers.*]` this port uses by default. Two ports naming one instance share a single client. A name that is not in the registry is a startup failure that lists what IS configured. | `FS3_EMBEDDER__ACTIVE`, `FS3_SUMMARIZER__ACTIVE` |

## `[repos."<identity>"]` — per-repo overrides

Keyed by repo identity (`github.com/AI-Substrate/flowspace3`), so a monorepo of
Rust can use a different summarizer from a repo of prose without a second config
file. Both keys are optional; an absent one falls back to the port's `active`.

| Key | Type | Default | Effect |
|---|---|---|---|
| `embedder` | string | *the port's `active`* | Instance name for the embedder port, for this repo only. |
| `summarizer` | string | *the port's `active`* | Instance name for the summarizer port, for this repo only. |

## `[indexing]`

| Key | Type | Default | Effect | Env override |
|---|---|---|---|---|
| `summary_min_lines` | integer | `10` | Size floor, in lines, for per-element LLM summaries (PRD req 32). Must be at least 1. | `FS3_INDEXING__SUMMARY_MIN_LINES` |
| `turn_summary_min_bytes` | integer | `256` | Size floor, in BYTES, for per-turn LLM summaries (workshop 005). Bytes rather than lines because a turn occupies one position in a sequence, so a line floor cannot tell a five-word "ship it" from the same turn carrying a 4KB tool result. Below the floor a turn is embedded raw and never summarised. Must be at least 1. | `FS3_INDEXING__TURN_SUMMARY_MIN_BYTES` |
| `debounce_seconds` | integer | `10` | How long a dirty file must settle before it is processed (PRD req 29). Enforced by the job row's `not_before`, not by a timer — a re-fire pushes the deadline out. | `FS3_INDEXING__DEBOUNCE_SECONDS` |
| `worker_concurrency` | integer | `4` | How many jobs the runner claims at once. This is the QUEUE's width, not provider parallelism: `SKIP LOCKED` hands N workers N different jobs. Must be at least 1. | `FS3_INDEXING__WORKER_CONCURRENCY` |
| `summarize_lane` | integer | `32` | How many `summarize` jobs may be in flight. Its own number because a summarize is one chat call per element. Clamped per instance by the summarizer's own concurrency ceiling. Must be at least 1. | `FS3_INDEXING__SUMMARIZE_LANE` |
| `embed_lane` | integer | `10` | How many merged `embed` BATCHES may be in flight — batches, not items. Clamped per instance by the embedder's ceiling; the local ONNX embedder declares 1, because its session is behind a mutex. Must be at least 1. | `FS3_INDEXING__EMBED_LANE` |

## `[scan]`

| Key | Type | Default | Effect | Env override |
|---|---|---|---|---|
| `max_file_bytes` | integer | `2000000` | Skip files larger than this. Must be at least 1, and greater than `min_file_bytes`. | `FS3_SCAN__MAX_FILE_BYTES` |
| `min_file_bytes` | integer | `1` | Skip files smaller than this. The default skips empty files only. | `FS3_SCAN__MIN_FILE_BYTES` |
| `respect_gitignore` | boolean | `true` | Honour `.gitignore` while walking. Off means indexing build output. | `FS3_SCAN__RESPECT_GITIGNORE` |
| `include_hidden` | boolean | `false` | Walk dot-files and dot-directories. | `FS3_SCAN__INCLUDE_HIDDEN` |
| `follow_symlinks` | boolean | `false` | Follow symlinks. Off by default: a link loop is an infinite scan. | `FS3_SCAN__FOLLOW_SYMLINKS` |
| `standard_ignores` | boolean | `true` | Skip `node_modules`, `target`, `dist` and kin as whole path components, **even when the repo has no `.gitignore`**. Off means a `.gitignore`-less clone indexes its dependencies. | `FS3_SCAN__STANDARD_IGNORES` |

## `[update]`

Auto-update is **on by default** (Jordan, 2026-08-27). The daemon checks GitHub
Releases, and when a newer published build exists it downloads it, verifies it
against the release's `SHA256SUMS`, and atomically replaces the installed
binary. It then tells you through the user messages queue, which rides on every
command's envelope until you restart the daemon. See
[`docs/services/auto-update.md`](../services/auto-update.md).

| Key | Type | Default | Effect | Env override |
|---|---|---|---|---|
| `auto` | boolean | `true` | Check for, download and install newer releases without being asked. Off means the daemon never reaches the network for a release and never swaps a binary — `flowspace3 doctor upgrade` still works, because a human asking for an update is not the same thing as one happening unattended. | `FS3_UPDATE__AUTO` |
| `check_interval_hours` | integer | `24` | How long the daemon waits between release checks. Honoured against a timestamp in Postgres rather than a timer, so a daemon restarted every ten minutes still checks once a day. Must be at least 1 when `auto` is on: GitHub's release endpoints are a shared, rate-limited resource. | `FS3_UPDATE__CHECK_INTERVAL_HOURS` |

## Not configuration

Named here because people look for them in `config.toml` and they are not there:

| Thing | Where it lives | Why |
|---|---|---|
| API keys | `secrets.env` beside `config.toml`, or the process environment | fs3 stores no secrets. Config names the variable; the value never enters a config file, a log line, or `config show`. |
| The config directory | `FS3_CONFIG_DIR` | It steers the loader, so it cannot itself be a config key. It is deliberately not an `FS3_<SECTION>__<KEY>` name. |
| Per-repo or per-folder config files | nowhere | Global file, per-repo *data* (PRD req 28). `[repos."…"]` is how a repo differs. |
| The reconcile cadence | code (`RECONCILE_EVERY_SECONDS`) | Five seconds, and not a knob yet: nothing user-visible waits on it. The update loop's own interval IS configurable, above. |
| The container engine | `FS3_ENGINE` | `flowspace3 doctor` drives `docker` by default; `podman` and `nerdctl` speak the same compose dialect. |

## Keeping this page honest

`cargo test -p fs3-core --test config_reference` fails when a configuration key
exists with no row here. Adding an option therefore costs one deliberate line on
this page — which is the point, and the same encode-don't-document muscle as the
architecture check and the error-code registry.
