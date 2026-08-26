# Configuration system

**What it is** — fs3's single global configuration mechanism: typed sections in
`fs3-core`, a layered loader in the two shells, a separate secrets chain, and
`flowspace3 config show` to prove what actually resolved. One machine, one
configuration: `~/.config/flowspace3/`. Task-oriented instructions live in
[docs/how/configuration.md](../how/configuration.md); this page is what the
system *is* and what building it taught us.

Scope ruling (Jordan, 2026-08-26): **global only**. No per-repo or per-folder
overrides. The loader is layered-shaped so a fourth layer slots in later without
a redesign, but nothing today looks at the current directory.

## Shape

```
defaults (serde)  <  ~/.config/flowspace3/config.toml  <  FS3_*__* environment
                                   +
                     secrets.env → process environment (separate chain)
```

- **Types + merge + validation + redaction**: `crates/core/src/config.rs` (pure).
- **File/env IO**: `crates/daemon/src/config.rs`, `crates/cli/src/settings.rs`.
- **Choosing implementations**: `crates/daemon/src/wiring.rs` (the one composition root).
- **Printing**: `crates/cli/src/show.rs`.

Sections: `daemon`, `database`, `providers`, `embedder`, `summarizer`, `repos`,
`indexing`, `scan`.

### Providers are a registry

`[providers.<name>]` tables declare any number of instances, each a tagged shape
(`kind = "openai" | "fake"`). `[embedder]`/`[summarizer]` hold only `active =
"<name>"`, and `[repos."<identity>"]` may name a different instance for either
port. All of it lives in the one global `config.toml` — per-repo settings are
keyed *data*, not a second file, so the global-only ruling holds.

The loader resolves names at startup: an unknown name is an error listing the
configured ones, an instance failing its kind-shape names
`providers.<name>.<key>`. The composition root constructs each *referenced*
instance exactly once (two repos naming one instance share the `Arc`), and
`AppState::embedder_for(repo)` / `summarizer_for(repo)` is a map lookup over
objects that already exist — no per-job construction, no second `AppState`, and
no lookup-by-name at call time.

## Key decisions, and why

**The merge lives in core, the IO does not.** Workshop 001 rule 2 says core is
pure, and rule "cli is a thin client of core only" means the CLI cannot call the
daemon's loader. So the *meaning* of the layers (parse, merge, coerce, validate)
is one pure function, `fs3_core::resolve`, and each shell keeps ~30 lines of
`read_to_string` + `env::vars()` around it. The duplication is a shim, not a
second config system; a behavioural divergence would be a bug in one shim.

**Env overrides are typed by the key they target, never guessed.** The defaults
serialize to a TOML table, and that table is the type oracle: `max_file_bytes`
is an integer there, so `FS3_SCAN__MAX_FILE_BYTES=big` is a startup error naming
the variable. Keys that exist only under a non-default provider arm (`model`)
are unknown to the oracle and stay strings. No "does it look like a number"
heuristics, which is what makes a model called `4` impossible to mis-type.

**A nested `FS3_*` name that matches nothing is refused.** An override that
silently does nothing is the worst failure mode in a config system — you change
it, nothing happens, and you go looking in the wrong place. `FS3_DATABSE__URL`
fails at boot and lists the real section names.

**`scan.standard_ignores` (added 2026-08-26, by the discovery worker — note for
whoever owns this surface).** A plain bool, defaulting to `true`, honouring
`deny_unknown_fields` like every other `[scan]` key. It toggles
`fs3_parsers::discovery::STANDARD_IGNORES` — `node_modules`, `target`, `dist`
and kin, denied by whole path component even in a repo with no `.gitignore`.
The **list itself deliberately stays in code, not config**: it is fs3's opinion
about build output, and a repo that disagrees has two sharper tools already —
`force_include` to reach one directory, or `standard_ignores = false` to drop
the policy entirely. If a future need makes the names themselves configurable,
the field becomes a bool-or-list enum rather than a second key; `DiscoverySettings`
already carries the list shape. See `docs/services/discovery.md`.

**Validation collects; it never returns on the first problem.** One bad file
costs one edit round-trip. Every `Problem` carries the key, what is wrong, and a
pasteable example line, and the loader prefixes the file path.

**Secrets are a separate chain with no templating.** `config.toml` names a
variable (`api_key_env`), never a value. Values come from the process
environment, or from `secrets.env` which both binaries load *into* the
environment at startup — process env always wins. There is deliberately no
`${VAR}` templating: one mechanism, and it is the one everything else already
uses.

**Declaring an instance costs nothing.** Only instances a port or a repo names
are constructed, so a `[providers.…]` table for a provider you have no key for
does not stop the daemon starting. The flip side: a *referenced* instance with a
missing key fails at boot, naming the instance and the variable.

**Services are constructed with the narrow section they need.** `build_embedder(&ProviderInstance)`,
`build_store(&DatabaseConfig)` — never `&Config`, never a lookup. Read the
function, know the blast radius. `AppState` holds the whole `Config` only as the
composition root's record of what it wired.

## Gotchas (the expensive lessons)

**A secret variable named `FS3_…` collided with the override namespace.** The
first cut treated *any* `FS3_`-prefixed variable as a config override, so
`api_key_env = "FS3_DEMO_KEY"` — an entirely reasonable thing for a user to
write — made the daemon refuse to start with "names no configuration key". Found
by smoke-testing `config show`, not by a unit test. The rule is now: an override
is `FS3_` + section + `__` + key, and **every config key lives in a section, so
every override nests**. A prefixed name without `__` belongs to somebody else
(`FS3_CONFIG_DIR`, secrets) and is left alone. Do not name a secret
`FS3_SOMETHING__ELSE`.

**Rust 2024 makes `env::set_var` unsafe, and `#[tokio::main]` defeats it.** The
secrets chain mutates the process environment, which is only sound while the
process is single-threaded — and `#[tokio::main]` has already built a
multi-threaded runtime by the time your first statement runs. Both binaries are
now plain `fn main` that load secrets first and build the runtime afterwards. If
you add a startup step that touches the environment, it goes *before* the
runtime.

**Deep-merging two tagged arms produces a shape that belongs to neither.** The
default registry holds `[providers.fake] kind = "fake"`; a file redefining that
name as `kind = "openai"` must *replace* the table, not merge into it, or a stale
key from the other arm survives and `deny_unknown_fields` rejects the result.
`merge_tables` special-cases a changed `kind` discriminant.

**Env-mutating tests deadlock or race if you are casual about it.** Every test
in `crates/daemon/tests/config_loading.rs` takes one binary-wide mutex as its
first statement and holds it for the whole body — including the parts that only
*read* the environment, like `std::env::temp_dir`. Taking the lock twice in one
test (a scripted edit did exactly that) is a self-deadlock: `std::sync::Mutex`
is not reentrant, and the symptom is ten tests all "running for over 60 seconds".

**Redaction has to be on the type, not on each printer.** `Config::redacted()`
returns a copy with the database password masked, and `config show` serializes
*that*. A field added later cannot leak by someone forgetting to mask it at one
call site.

## How to verify

```bash
# unit + integration proof for the whole system
cargo test -p fs3-core -p fs3-daemon -p fs3-cli

# the layering, end to end, through the real binary
D=$(mktemp -d)
cat > $D/config.toml <<'TOML'
[daemon]
url = "http://127.0.0.1:7474"

[providers.small]
kind = "openai"
model = "text-embedding-3-small"
api_key_env = "FS3_DEMO_KEY"

[embedder]
active = "small"

[repos."github.com/acme/thing"]
summarizer = "fake"
TOML
printf 'FS3_DEMO_KEY=sk-not-a-real-key\n' > $D/secrets.env
FS3_CONFIG_DIR=$D FS3_SCAN__MAX_FILE_BYTES=4096 ./target/debug/flowspace3 config show
#  -> [daemon]/[providers]/[embedder]/[repos] from config.toml,
#     [scan] from FS3_* environment, "embedder -> small (FS3_DEMO_KEY: set)",
#     database password shown as <redacted>

# a typo must stop the process, not be ignored
FS3_CONFIG_DIR=$D FS3_DATABSE__URL=x ./target/debug/flowspace3 config show; echo $?
#  -> exit 1, "names no configuration section; sections are: …"

# the boot contract: no store, no daemon
cargo test -p fs3-daemon --test boot_contract
```

## Code pointers

| Thing | Where |
|---|---|
| Sections, defaults, examples in rustdoc | `crates/core/src/config.rs` |
| Registry types + selection (`ProviderInstance`, `PortSelection`, `RepoSelection`, `Port`) | same file |
| `Config::selected` / `provider` / `referenced_providers` | same file — name resolution |
| `resolve` / `Sources` / `Effective` / `Layer` | same file — the whole merge |
| `Problem` + `Config::problems` | same file — collect-all validation |
| `parse_env_file`, `redact_url_password` | same file |
| Daemon loader + secrets application | `crates/daemon/src/config.rs` |
| CLI loader shim | `crates/cli/src/settings.rs` |
| `config show` rendering | `crates/cli/src/show.rs` |
| Composition root / narrow injection | `crates/daemon/src/wiring.rs` |
| Loader + secrets tests | `crates/daemon/tests/config_loading.rs` |
| Registry + per-repo resolution tests | `crates/daemon/tests/config_wiring.rs` |
| Fail-fast boot contract | `crates/daemon/tests/boot_contract.rs` |
