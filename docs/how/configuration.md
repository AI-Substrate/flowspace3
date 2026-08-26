# Configuration

fs3 reads all of its configuration from **one global directory**:
`~/.config/flowspace3/`. Nothing lives in the database (PRD reqs 28, 39), and
there are no per-repo or per-folder config files — one machine, one
configuration.

```
~/.config/flowspace3/
├── config.toml     what fs3 does        (checked, typed, printable)
└── secrets.env     KEY=value            (never printed, never merged)
```

Neither file has to exist. With both absent fs3 runs on its defaults, which are
a complete offline stack: both ports select the built-in `fake` provider
instance, so no API keys are needed.

## Where a value comes from

Three layers, lowest precedence first:

| Layer | Source | Use it for |
|---|---|---|
| 1 | serde defaults in `crates/core/src/config.rs` | the offline stack |
| 2 | `~/.config/flowspace3/config.toml` | your machine's real settings |
| 3 | `FS3_*` environment variables | containers, CI, one-off runs |

`FS3_CONFIG_DIR` moves the whole directory somewhere else — that is how tests
and throwaway environments get an isolated config without touching yours.

To see the result of all three, ask:

```console
$ flowspace3 config show
# effective fs3 configuration
# config file: /Users/you/.config/flowspace3/config.toml (present)
# secrets:     /Users/you/.config/flowspace3/secrets.env (present)
# layers: defaults < config.toml < FS3_* environment
#
# [daemon]     from config.toml
# [database]   from defaults
# [providers]  from config.toml
# [embedder]   from config.toml
# [summarizer] from defaults
# [repos]      from config.toml
# [indexing]   from defaults
# [scan]       from FS3_* environment
#
# resolved providers (secrets are never printed):
#   embedder -> small (OPENAI_API_KEY: set)
#   summarizer -> fake
#   summarizer for github.com/AI-Substrate/flowspace3 -> big (AZURE_KEY: NOT SET — the daemon will refuse to start)

[daemon]
url = "http://127.0.0.1:7373"

[database]
url = "postgres://flowspace3:<redacted>@127.0.0.1:5433/flowspace3"

[providers.small]
kind = "openai"
model = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"
...
```

That output is the answer to "why is fs3 not doing what my file says": it shows
the merged values *and* which layer won for each section. `--config-dir DIR`
reads somewhere else without exporting anything.

## Changing something

Edit `~/.config/flowspace3/config.toml` and restart the daemon. Every section
and every key is documented — with an example block — on the types in
[`fs3_core::config`](../../crates/core/src/config.rs); `cargo doc -p fs3-core
--open` renders them.

A full file:

```toml
[daemon]
url = "http://127.0.0.1:7373"      # loopback only; anything else is refused at boot

[database]
url = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"

# --- the provider registry: any number of named instances ---------------------
[providers.small]
kind = "openai"                    # the shape; the rest of the table follows it
model = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"     # the NAME of a variable, never a key

[providers.big]
kind = "openai"
model = "gpt-4o"

# `fake` always exists — offline, deterministic, no keys — and may be redefined.

# --- the ports name one instance each -----------------------------------------
[embedder]
active = "small"

[summarizer]
active = "fake"

# --- per-repo overrides, keyed by repo identity -------------------------------
[repos."github.com/AI-Substrate/flowspace3"]
summarizer = "big"                 # this repo only; `embedder` stays the default

[indexing]
summary_min_lines = 10             # PRD req 32: the size floor for summaries
debounce_seconds = 10              # PRD req 29: how long a dirty file settles

[scan]
max_file_bytes = 2000000           # skip generated bundles and vendored blobs
min_file_bytes = 1                 # skip empty files
respect_gitignore = true
include_hidden = false
follow_symlinks = false            # a link loop is an infinite scan
```

### Providers are a registry, not a shape per port

Declaring an instance and *selecting* it are separate steps, which buys three
things:

- **Two ports can share one instance** by naming it twice — one client, one
  connection pool, one place to change the model.
- **A repo can differ from the default** without a second config file:
  `[repos."<identity>"]` names an instance for either port. This is still the one
  global file; per-repo settings are keyed *data* in it.
- **Declaring an instance costs nothing.** Only instances something references
  are constructed, so a `[providers.…]` table you have no key for does not stop
  the daemon starting.

Selecting a name that is not in the registry is a startup error that lists the
names that are:

```console
$ flowspace3 daemon
Caused by: invalid config: 1 problem found:
  - embedder.active: names provider "smal", which is not configured; configured
    providers are: big, fake, small
    try: [providers.smal]
         kind = "fake"
```

Mistakes are reported **all at once**, each naming the file, the key, and a line
you can paste:

```console
$ flowspace3 daemon
Error: loading configuration from /Users/you/.config/flowspace3
Caused by: invalid config: 2 problems found:
  - daemon.url: must not be empty
    try: url = "http://127.0.0.1:7373"
  - scan.max_file_bytes: (0) must be at least 1 — zero would skip every file
    try: max_file_bytes = 2000000
```

A missing file is not a mistake: the daemon logs, at INFO, the path to create.

## Environment overrides

An override is `FS3_` + section + `__` + key, upper-cased:

```bash
FS3_DATABASE__URL=postgres://user:pw@db:5432/fs3 flowspace3 daemon
FS3_SCAN__MAX_FILE_BYTES=4096 flowspace3 config show
FS3_EMBEDDER__ACTIVE=fake flowspace3 daemon               # select another instance
FS3_PROVIDERS__SMALL__MODEL=text-embedding-3-large flowspace3 daemon  # reshape one
```

Two rules make this predictable:

- **Values are typed by the key they target**, not guessed. `max_file_bytes` is
  an integer, so `FS3_SCAN__MAX_FILE_BYTES=big` is a startup error naming the
  variable — not a silently ignored setting.
- **A nested `FS3_*` name that matches no key is refused.** `FS3_DATABSE__URL`
  fails at boot and lists the real section names. An override that quietly does
  nothing is the worst possible outcome.

`FS3_`-prefixed names *without* `__` are not configuration: `FS3_CONFIG_DIR`
steers the loader, and a key variable of your own called `FS3_ACME_API_KEY` is
left alone. Do not name a secret `FS3_SOMETHING__ELSE`.

## Secrets

Secret **values** never appear in `config.toml`. Config names the variable that
holds a key:

```toml
[providers.small]
kind = "openai"
api_key_env = "OPENAI_API_KEY"
```

...and the value comes from one of two places, in this order:

1. the process environment — `OPENAI_API_KEY=… flowspace3 daemon` wins over everything;
2. `~/.config/flowspace3/secrets.env`, which both binaries load *into* the
   environment as their first act at startup.

```bash
# ~/.config/flowspace3/secrets.env   (chmod 600 it)
OPENAI_API_KEY=sk-…
export AZURE_OPENAI_API_KEY="…"     # a leading `export` is tolerated
```

There is no `${VAR}` templating anywhere in `config.toml`, on purpose: one
mechanism, and it is the one the rest of the world already uses.

Nothing prints a secret. `config show` reports a key variable as `set` or `NOT
SET` by name only, the database password is masked as `<redacted>`, the
daemon's startup log lists the variable *names* a secrets file supplied, and
`AppState`'s `Debug` masks the database URL. If you add a printer, print
`Config::redacted()`.

## Adding a new injected service

This is the repo's DI story, and it is deliberately boring: **services receive
everything they need and go looking for nothing.** There is no container, no
service locator, and no `Config` global. The composition root is the only code
that chooses (workshop 001 rule 4 — see
[fs3-architecture.md](../rules-idioms-architecture/fs3-architecture.md)).

(The provider *registry* is not a counter-example: it is configuration data, and
`[providers.…]` names are resolved to objects once, at startup, in the
composition root. Nothing looks a service up by name at call time —
`AppState::embedder_for(repo)` is a map lookup over objects that already exist.)

Say you are adding a `Scanner`.

**1. Give it a config section** — a struct in `crates/core/src/config.rs` with
`Default`, `#[serde(default, deny_unknown_fields)]`, and a rustdoc block showing
its `config.toml` snippet:

```rust
/// ```toml
/// [scan]
/// max_file_bytes = 2000000
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanConfig { /* … */ }
```

Add the field to `Config`, add its name to `SECTIONS` (so `config show` reports
its provenance), and give it a `collect(&self, problems: &mut Vec<Problem>)`
that pushes a [`Problem`] per unusable value — **collect, never
return-on-first**. Call it from `Config::problems`.

**2. Construct the service from that section, and only that section.**

```rust
// crates/daemon/src/wiring.rs
fn build_scanner(scan: &ScanConfig) -> Result<Scanner> {
    Scanner::new(scan.max_file_bytes, scan.respect_gitignore)
}
```

Take `&ScanConfig`, never `&Config`. A service that receives the whole config
can reach the database URL, and then its dependencies are no longer its
signature. This is the rule the architecture doc means by "one composition
root": read the function, know the blast radius.

**3. Wire it in the one place that chooses.**

```rust
impl AppState {
    pub fn from_config(config: Config) -> Result<Self> {
        let embedder = build_embedder(&config.embedder)?;
        let scanner = build_scanner(&config.scan)?;      // <- the new line
        // …
    }
}
```

`AppState` keeps the whole `Config` because it is the composition root's record
of what it wired (`/health` and `config show` report from it) — not so services
can reach into it. Nothing constructs itself from `AppState`.

**4. Prove it.** A core unit test for the section's validation, and a daemon
test that a fixture directory selects the instance you expect
(`crates/daemon/tests/config_loading.rs` is the pattern). If the service is a
new *port* — a trait with a second real implementation — stop: rule 3 says a
third port is a stop-and-ask.

The drift check enforces the crate direction underneath all of this: `cargo run
-p fs3-testkit --bin fs3-arch-check` fails on any dependency edge that is not in
`crates/testkit/arch-allowlist.toml`, and it runs inside `harness checks`.

## Where the code is

| Concern | Lives in | Why there |
|---|---|---|
| Types, defaults, validation, the merge, secrets parsing, redaction | `crates/core/src/config.rs` | pure — parses strings, touches no file |
| Reading the files, the environment, and mutating it | `crates/daemon/src/config.rs`, `crates/cli/src/settings.rs` | effects live at the edges |
| Choosing implementations | `crates/daemon/src/wiring.rs` | the one composition root |
| Printing it back | `crates/cli/src/show.rs` | presentation, and the only redaction consumer |

The daemon and the CLI each own their ~30 lines of file IO and call the same
`fs3_core::resolve`. The CLI is a thin HTTP client that may not depend on the
daemon crate, so the *shim* is duplicated; the *meaning* is not, and a
divergence in behaviour would be a bug in one shim rather than a second config
system.

### Adding a new provider kind

A new kind of embedder or summarizer is a new arm of `ProviderInstance` (`kind =
"…"`), not a new port and not a new section:

1. Add the arm in `crates/core/src/config.rs`, with its rustdoc `[providers.x]`
   block and its shape validation in `ProviderInstance::collect` — name the bad
   key as `providers.<name>.<key>`.
2. Add the matching arm to `build_embedder` / `build_summarizer` in
   `crates/daemon/src/wiring.rs`, constructing from the instance and taking its
   key through `api_key_env`.
3. Prove it in `crates/daemon/tests/config_wiring.rs`: a fixture file naming the
   new kind resolves to it, and a missing key fails at startup naming the
   instance.

A new *port* — a third trait — is a stop-and-ask (workshop 001 rule 3).
