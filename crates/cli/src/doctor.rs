//! `flowspace3 doctor` — repair as it goes.
//!
//! Doctor walks the dependency chain and FIXES what it can, reporting each move
//! as one row: what was checked, what was found, what it did. It errors only
//! where it genuinely cannot act.
//!
//! ```text
//! engine   → is there a docker/podman to talk to?     (cannot fix: report)
//! stack    → is the database container running?       (fix: compose up -d)
//! database → does the configured database exist?      (fix: CREATE DATABASE)
//! schema   → is it current with this binary?          (fix: apply migrations)
//! ```
//!
//! Jordan's ruling, 2026-08-26: no manual compose step, ever, and no second
//! command to apply migrations. A diagnosis that ends in "now go and run this
//! other thing" is a diagnosis that has stopped one step early.
//!
//! # Why this verb, alone, opens a pool
//!
//! Every other CLI verb is a thin HTTP client and the daemon is the single
//! writer (PRD req 20). Doctor is the named exception, ruled by o-prime on
//! 2026-08-26: its writes are CONTROL plane — creating a database, applying
//! migrations — bootstrap operations that logically precede a daemon existing.
//! It is also precisely the verb you reach for when the daemon is DOWN, so it
//! cannot be a client of an endpoint that is not listening.
//!
//! Doctor ORCHESTRATES and implements nothing: every step below is one call
//! into `fs3_store`'s admin module or one process spawn. A second
//! implementation of "is the schema current" living here is exactly the drift
//! the split refuses.

use std::process::Stdio;
use std::time::{Duration, Instant};

use fs3_core::envelope::{Envelope, Failure};
use fs3_core::views::doctor::{DoctorReport, Step};
use fs3_core::{Config, Effective, Port, ProviderInstance, catalog};

/// Environment variable naming the container engine.
///
/// `docker` by default; `podman` and `nerdctl` speak the same compose dialect,
/// and a developer on one of them should not need a code change.
pub const ENGINE_ENV: &str = "FS3_ENGINE";

/// The engine used when [`ENGINE_ENV`] is unset.
pub const DEFAULT_ENGINE: &str = "docker";

/// How long to wait for the stack to become reachable after starting it.
const STACK_READY_TIMEOUT: Duration = Duration::from_secs(60);

/// How often to re-probe while waiting.
const STACK_POLL: Duration = Duration::from_millis(500);

/// How long to wait for the daemon's health endpoint. Short: a daemon that is
/// up answers immediately, and one that is not should not cost a diagnostic
/// command three seconds of silence.
const DAEMON_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The actionable next step when the store is ready but the daemon is down.
/// Shared with the combined-state ordering test so the asserted line is the
/// exact line an operator or agent receives.
const DAEMON_DOWN_STEER: &str = "the store is ready but the daemon is not running — start it with `flowspace3 daemon &`, then `flowspace3 add <path>`";

/// Walk the chain, repairing as it goes.
///
/// Stops at the first step it cannot fix, because every later step depends on
/// it: probing a schema on a server that is not running produces a second,
/// less useful copy of the same failure.
///
/// # Errors
/// The failure of the step that could not be repaired, with its own catalog
/// code and fix.
pub async fn run(effective: &Effective, config_dir: &std::path::Path) -> Envelope<DoctorReport> {
    let mut steps = Vec::new();
    let config_warning = check_config(effective);

    match walk(&effective.config, config_dir, &mut steps).await {
        Ok(messages) => {
            insert_config_warning(&mut steps, config_warning);
            // `warn` counts as degraded: a stack running entirely on the
            // offline fake is working, and is almost never what the operator
            // believes they configured. Reporting a plain "ok" there is the
            // same class of silence as reporting ok with no daemon.
            let degraded = steps.iter().any(Step::degrades);
            let verdict = if degraded {
                DoctorReport::DEGRADED
            } else {
                DoctorReport::OK
            };
            let steer = next_action(&steps);
            Envelope::ok(
                "doctor",
                DoctorReport {
                    steps,
                    healthy: true,
                    verdict: verdict.to_string(),
                },
            )
            .with_next_action(steer)
            // Doctor is the one local verb that holds a pool, so it is the one
            // that can carry the queue without a daemon answering (req-0059).
            .with_messages(messages)
        }
        Err(failure) => {
            insert_config_warning(&mut steps, config_warning);
            let mut envelope = Envelope::failed("doctor", failure);
            // The steps that DID pass are the useful half of a failed run, so
            // they ride along in meta rather than being discarded.
            envelope.meta = serde_json::to_value(DoctorReport {
                steps,
                healthy: false,
                verdict: DoctorReport::DEGRADED.to_string(),
            })
            .ok();
            envelope
        }
    }
}

/// Report top-level sections this binary did not understand and therefore
/// ignored. This is a warning rather than an error: refusing the whole file
/// would make every additive config section a flag-day across installed
/// versions, while silence would let a misspelled section survive indefinitely.
fn check_config(effective: &Effective) -> Option<Step> {
    let started = Instant::now();
    let warnings: Vec<_> = effective.warnings().collect();
    if warnings.is_empty() {
        return None;
    }

    let found = warnings
        .iter()
        .map(|warning| format!("{}: {}", warning.key, warning.message))
        .collect::<Vec<_>>()
        .join("; ");
    let action = warnings
        .iter()
        .map(|warning| warning.example.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    Some(
        Step::warn("config", found, action, started).with_steer(
            "unknown config sections were ignored — remove typos or upgrade flowspace3 for newer sections",
        ),
    )
}

/// Place config warnings after runtime checks but before the deliberately-last
/// skills row. The warning stays actionable, while dependency blockers keep
/// their priority in [`next_action`]. On an early failed walk there is no
/// skills row, so appending still preserves the rows already reached.
fn insert_config_warning(steps: &mut Vec<Step>, warning: Option<Step>) {
    let Some(warning) = warning else {
        return;
    };
    let before_skills = steps
        .iter()
        .position(|step| step.check == "skills")
        .unwrap_or(steps.len());
    steps.insert(before_skills, warning);
}

/// Returns the live user messages the walk observed, so doctor's own envelope
/// carries them like every other command's does (req-0059).
async fn walk(
    config: &Config,
    config_dir: &std::path::Path,
    steps: &mut Vec<Step>,
) -> Result<Vec<fs3_core::UserMessage>, Failure> {
    let database_url = config.database.url.as_str();
    let daemon_url = config.daemon.url.as_str();

    steps.push(check_engine()?);

    let (maintenance, name) = fs3_store::maintenance_url(database_url).map_err(|error| {
        Failure::new(&catalog::CONFIG_INVALID, error.to_string()).with_fix(
            "database.url must be postgres://host:port/database — check `flowspace3 config \
                 show`",
        )
    })?;

    steps.push(check_stack(&maintenance).await?);

    let admin = fs3_store::connect(&maintenance).await.map_err(map_store)?;
    steps.push(check_database(&admin, &name).await?);
    admin.close().await;

    steps.push(check_schema(database_url).await?);

    // The daemon is the one step doctor deliberately does NOT repair. Starting
    // a foreground server from a diagnostic command would leave a process the
    // user did not ask for and cannot see; the honest move is to report it and
    // name the command. Auth follows immediately: it proves both the on-disk
    // credential and that the running daemon accepts those exact bytes.
    steps.push(check_daemon(daemon_url, config_dir).await);
    steps.push(check_auth(daemon_url, config_dir).await);
    steps.push(check_providers(config));
    // req-0054 / req-0059. Both read the store, so they walk after the schema
    // row that guarantees the tables exist, and after `daemon` and `providers`
    // so the steer order stays right: a reader with no daemon running is told
    // to start one before being told to restart it for a new binary.
    //
    // Neither ever repairs. Doctor does not update binaries behind your back —
    // `flowspace3 doctor upgrade` is the verb that does, and the row names it.
    steps.push(check_update(database_url, config).await);
    let (messages_row, messages) = check_messages(database_url).await;
    steps.push(messages_row);
    // The daemon's log destination (2026-08-27). After the store rows because
    // it asks nothing of them, and after `daemon` so the steer order stays
    // right: a reader with no daemon running is told to start one before being
    // told where it would have written its logs.
    steps.push(check_logs(config));
    // req-0053: the skills row walks LAST — informational, never degrading, and
    // the one row reporting state doctor will never itself change. Anything new
    // goes ABOVE it; `doctor_walks_the_skills_row_last_and_informationally`
    // holds the line.
    steps.push(check_skills());
    Ok(messages)
}

/// This CLI's own resolved binary path — WHICH installation `doctor` speaks
/// for.
///
/// `doctor` is the one verb holding its own pool, so unlike a daemon-served
/// envelope it reports on the binary the user just typed rather than on the
/// binary answering them. That is deliberate and it is the answer to "why does
/// my `add` not mention the update my `doctor` does": on a machine with two
/// installs they are two installations with two truths (Jordan, 2026-08-27).
///
/// Empty when the executable cannot be resolved, which matches no install and
/// so degrades to the store-wide messages only.
fn this_install() -> String {
    fs3_daemon::update::install_path()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// Step 7: what does the auto-updater think the situation is (req-0054)?
///
/// Three states worth telling apart, because the reader's next move differs in
/// each: current (nothing to do), a newer binary already downloaded and waiting
/// on a restart, and notify-only — something newer exists that this machine
/// could not install, with the reason.
async fn check_update(database_url: &str, config: &Config) -> Step {
    let started = Instant::now();
    let running = env!("CARGO_PKG_VERSION");

    let Ok(pool) = fs3_store::connect(database_url).await else {
        return Step::info(
            "update",
            "cannot read the update state — the store is not answering",
            "the rows above say why",
            started,
        );
    };
    let state = fs3_store::update_state(&pool, &this_install()).await;
    pool.close().await;

    let Ok(state) = state else {
        return Step::info(
            "update",
            "the update state could not be read",
            "run `flowspace3 doctor` again once the schema row is green",
            started,
        );
    };

    let cadence = if config.update.auto {
        format!(
            "auto-update on, every {}h",
            config.update.check_interval_hours
        )
    } else {
        "auto-update off".to_string()
    };
    let checked = state
        .last_checked
        .as_deref()
        .map_or_else(|| "never checked".to_string(), |at| format!("checked {at}"));

    // Something is waiting on a restart. `warn`, not `info`: the user asked
    // for auto-update and is running something other than what they now have.
    if let Some(installed) = state
        .installed_version
        .as_deref()
        .filter(|installed| *installed != running)
    {
        let path = &state.install_path;
        return Step::warn(
            "update",
            format!("{installed} is installed at {path}; this CLI is {running} ({cadence})"),
            "restart the fs3 daemon to run the new binary",
            started,
        )
        .with_steer("restart the fs3 daemon — a newer flowspace3 is already installed");
    }

    // Something newer exists and this machine could not take it.
    if let Some(reason) = state.blocked_reason.as_deref() {
        let path = &state.install_path;
        let latest = state
            .latest_seen
            .as_deref()
            .map_or(String::new(), |latest| format!(" ({latest} is published)"));
        return Step::warn(
            "update",
            format!("cannot update {path}: {reason}{latest}"),
            format!(
                "run `flowspace3 doctor upgrade` from a shell that can write it, or reinstall: \
                 `{}`",
                fs3_core::update::REINSTALL_COMMAND
            ),
            started,
        )
        .with_steer(
            "run `flowspace3 doctor upgrade` — an update is available but could not install",
        );
    }

    Step::ok(
        "update",
        format!("running {running} ({cadence}, {checked})"),
        started,
    )
}

/// Step 8: what is the daemon currently telling the user (req-0059)?
///
/// The queue is normally empty, and an empty queue is the healthy row. This
/// exists so that "why does every command keep telling me to restart" has a
/// place to be answered, and so a message with no live producer is visible
/// rather than mysterious.
/// Returns the live queue as well as the row, so [`run`] can put the same
/// messages on doctor's own envelope without asking the store twice.
async fn check_messages(database_url: &str) -> (Step, Vec<fs3_core::UserMessage>) {
    let started = Instant::now();

    let unreadable = |found: &str| {
        (
            Step::info(
                "messages",
                found.to_string(),
                "the rows above say why",
                started,
            ),
            Vec::new(),
        )
    };

    let Ok(pool) = fs3_store::connect(database_url).await else {
        return unreadable("cannot read the user messages queue — the store is not answering");
    };
    let messages = fs3_store::live_messages(&pool, &this_install()).await;
    pool.close().await;

    let Ok(messages) = messages else {
        return unreadable("the user messages queue could not be read");
    };

    if messages.is_empty() {
        return (
            Step::ok("messages", "no standing messages", started),
            messages,
        );
    }

    let found = messages
        .iter()
        .map(fs3_core::UserMessage::render)
        .collect::<Vec<_>>()
        .join(" · ");
    let action = format!(
        "{} message(s) ride on every command's envelope until their cause clears",
        messages.len()
    );
    (Step::info("messages", found, action, started), messages)
}

/// What to do next, from the FIRST unmet step rather than a generic line.
///
/// Order matters: a reader with no daemon AND no real provider should be told
/// to start the daemon first, because that is the step that blocks the other
/// from being observable.
fn next_action(steps: &[Step]) -> String {
    // The FIRST row that asks something and carries a steer wins, in walk
    // order — which is dependency order, so a reader with no daemon AND no real
    // provider is told to start the daemon first, because that is the step
    // blocking the other from being observable.
    //
    // Data-driven rather than a chain of check names: a new row supplies its
    // own steer and is picked up here without touching this function, and its
    // steer cannot drift from the finding that produced it.
    steps
        .iter()
        .filter(|step| step.asks_something())
        .find_map(|step| step.steer.clone())
        .unwrap_or_else(|| {
            "everything is up — `flowspace3 add <path>` to index, then `flowspace3 search`"
                .to_string()
        })
}

/// Step 0: is there an engine at all?
///
/// The one step doctor cannot repair — installing a container runtime is not
/// something a CLI should attempt — so it reports and stops.
fn check_engine() -> Result<Step, Failure> {
    let started = Instant::now();
    let engine = engine();

    let found = std::process::Command::new(&engine)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match found {
        Ok(status) if status.success() => {
            Ok(Step::ok("engine", format!("{engine} present"), started))
        }
        _ => Err(Failure::new(
            &catalog::STORE_UNAVAILABLE,
            format!("no container engine: `{engine} --version` did not run"),
        )
        .with_fix(format!(
            "install a container engine (OrbStack or Docker Desktop), or point {ENGINE_ENV} at \
             the one you have: `export {ENGINE_ENV}=podman`"
        ))
        .with_detail("engine", engine)),
    }
}

/// Step 1: is the stack running? Start it if not.
///
/// The probe is a real connection rather than `compose ps`: a container that is
/// up but not yet accepting connections is indistinguishable from a healthy one
/// by `ps`, and this is exactly the window a freshly-started stack sits in.
async fn check_stack(maintenance_url: &str) -> Result<Step, Failure> {
    let started = Instant::now();

    if let Ok(pool) = fs3_store::connect(maintenance_url).await {
        pool.close().await;
        return Ok(Step::ok(
            "stack",
            "postgres is accepting connections",
            started,
        ));
    }

    let engine = engine();
    let output = std::process::Command::new(&engine)
        .args(["compose", "up", "-d"])
        .output()
        .map_err(|error| {
            Failure::new(
                &catalog::STORE_UNAVAILABLE,
                format!("`{engine} compose up -d` could not be run: {error}"),
            )
        })?;

    if !output.status.success() {
        return Err(Failure::new(
            &catalog::STORE_UNAVAILABLE,
            format!(
                "`{engine} compose up -d` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        )
        .with_fix(
            "run it by hand from the repository root to see the whole output; a missing \
             docker-compose.yml means the command was run from the wrong directory",
        ));
    }

    // Started is not ready. Postgres accepts TCP before it accepts queries, so
    // the wait is for a real connection, not for the container to exist.
    let deadline = Instant::now() + STACK_READY_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(pool) = fs3_store::connect(maintenance_url).await {
            pool.close().await;
            return Ok(Step::repaired(
                "stack",
                "the database container was not accepting connections",
                format!("ran `{engine} compose up -d` and waited for it to answer"),
                started,
            ));
        }
        tokio::time::sleep(STACK_POLL).await;
    }

    Err(Failure::new(
        &catalog::STORE_UNAVAILABLE,
        format!(
            "the stack was started but did not answer within {}s",
            STACK_READY_TIMEOUT.as_secs()
        ),
    )
    .with_fix(format!(
        "check the container's own logs: `{engine} compose logs db`"
    )))
}

/// Step 2: does the database exist? Create it if not.
async fn check_database(admin: &fs3_store::PgPool, name: &str) -> Result<Step, Failure> {
    let started = Instant::now();

    if fs3_store::database_exists(admin, name)
        .await
        .map_err(map_store)?
    {
        return Ok(Step::ok("database", format!("{name} exists"), started));
    }

    fs3_store::create_database(admin, name)
        .await
        .map_err(map_store)?;

    Ok(Step::repaired(
        "database",
        format!("{name} did not exist"),
        format!("created the database {name}"),
        started,
    ))
}

/// Step 3: is the schema current? Apply what is missing.
///
/// This is the step every other command's stale-schema rejection points at, so
/// it has to actually fix it — a doctor that reports "two migrations behind"
/// and stops has told the reader what they already knew.
async fn check_schema(database_url: &str) -> Result<Step, Failure> {
    let started = Instant::now();
    let pool = fs3_store::connect(database_url).await.map_err(map_store)?;

    let status = fs3_store::schema_current(&pool).await.map_err(map_store)?;

    // AHEAD is checked BEFORE `is_current`, and that ordering is the whole fix
    // (req-0061). `is_current` is `missing.is_empty()`, so a database carrying
    // migrations this binary has never heard of satisfies it — doctor reported
    // a cheerful green on exactly the machine Jordan could not start a daemon
    // on. Migrating cannot repair it either: there is nothing to apply.
    let skew = status.skew(env!("CARGO_PKG_VERSION"));
    if skew.is_skewed() {
        pool.close().await;
        return Ok(
            Step::warn("schema", skew.summary(), skew.fix(), started).with_steer(
                "upgrade this flowspace3 — the database is NEWER than this binary, so migrating \
             cannot help: `flowspace3 doctor upgrade`",
            ),
        );
    }

    if status.is_current() {
        let found = format!("{} migrations applied", status.applied.len());
        pool.close().await;
        return Ok(Step::ok("schema", found, started));
    }

    let missing = status.missing_summary();
    let result = fs3_store::migrate(&pool).await;
    pool.close().await;
    result.map_err(map_store)?;

    Ok(Step::repaired(
        "schema",
        format!("missing migration(s) {missing}"),
        format!("applied {missing}"),
        started,
    ))
}

/// Step 4: is the daemon answering?
///
/// Reported, never repaired. The current key is attached when readable so the
/// probe still gets the version echo; a 401 also proves that an fs3 daemon is
/// answering, while the following auth row names the credential fault.
async fn check_daemon(daemon_url: &str, config_dir: &std::path::Path) -> Step {
    let started = Instant::now();
    let url = format!("{}/health", daemon_url.trim_end_matches('/'));

    let client = match reqwest::Client::builder()
        .timeout(DAEMON_PROBE_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return Step::down(
                "daemon",
                format!("cannot probe {daemon_url}: {error}"),
                started,
            );
        }
    };
    let mut request = client.get(&url);
    if let Ok(key) = std::fs::read_to_string(fs3_core::daemon_key_path(config_dir))
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key.trim());
    }

    match request.send().await {
        Ok(response) if response.status().is_success() => {
            let version = response
                .json::<crate::HealthReport>()
                .await
                .map(|health| health.version)
                .unwrap_or_default();
            let found = if version.is_empty() {
                format!("answering at {daemon_url}")
            } else {
                format!("answering at {daemon_url} (version {version})")
            };
            Step::ok("daemon", found, started)
        }
        Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => Step::ok(
            "daemon",
            format!("answering at {daemon_url} and requiring authentication"),
            started,
        ),
        Ok(response) => Step::down(
            "daemon",
            format!(
                "{daemon_url} answered {} rather than a health report",
                response.status()
            ),
            started,
        ),
        Err(_) => Step::down(
            "daemon",
            format!("nothing is listening on {daemon_url}"),
            started,
        )
        .with_steer(DAEMON_DOWN_STEER),
    }
}

/// Step 5: is daemon authentication securely configured and accepted?
async fn check_auth(daemon_url: &str, config_dir: &std::path::Path) -> Step {
    let started = Instant::now();
    let key_path = fs3_core::daemon_key_path(config_dir);
    let restart = format!(
        "restart the fs3 daemon so it publishes a fresh mode-0600 key at {}",
        key_path.display()
    );
    let key = match std::fs::read_to_string(&key_path) {
        Ok(key) if !key.trim().is_empty() => key,
        Ok(_) => {
            return Step::warn(
                "auth",
                format!("{} is empty", key_path.display()),
                &restart,
                started,
            )
            .with_steer(restart);
        }
        Err(error) => {
            return Step::warn(
                "auth",
                format!("cannot read {}: {error}", key_path.display()),
                &restart,
                started,
            )
            .with_steer(restart);
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = match std::fs::metadata(&key_path) {
            Ok(metadata) => metadata.permissions().mode() & 0o777,
            Err(error) => {
                return Step::warn(
                    "auth",
                    format!("cannot inspect {}: {error}", key_path.display()),
                    &restart,
                    started,
                )
                .with_steer(restart);
            }
        };
        if mode != 0o600 {
            let action = format!("run `chmod 600 {}`", key_path.display());
            return Step::warn(
                "auth",
                format!("{} has mode {mode:04o}, expected 0600", key_path.display()),
                &action,
                started,
            )
            .with_steer(action);
        }
    }

    let url = format!("{}/health", daemon_url.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(DAEMON_PROBE_TIMEOUT)
        .build()
        .map(|client| client.get(url).bearer_auth(key.trim()));
    let response = match response {
        Ok(request) => request.send().await,
        Err(error) => {
            return Step::warn(
                "auth",
                format!("cannot construct the authenticated probe: {error}"),
                &restart,
                started,
            )
            .with_steer(restart);
        }
    };

    match response {
        Ok(response) if response.status().is_success() => Step::ok(
            "auth",
            format!(
                "{} is mode 0600 and accepted by {daemon_url}",
                key_path.display()
            ),
            started,
        ),
        Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => Step::warn(
            "auth",
            format!("{} is stale: {daemon_url} rejected it", key_path.display()),
            &restart,
            started,
        )
        .with_steer(restart),
        Ok(response) => Step::warn(
            "auth",
            format!(
                "{daemon_url} answered {} to the authenticated probe",
                response.status()
            ),
            "fix the daemon row above, then re-run `flowspace3 doctor`",
            started,
        ),
        Err(_) => Step::info(
            "auth",
            format!(
                "{} is mode 0600; daemon acceptance is not observable",
                key_path.display()
            ),
            "start the daemon, then re-run `flowspace3 doctor` to prove acceptance",
            started,
        ),
    }
}

/// Step 6: is a real provider configured, or is everything the offline fake?
///
/// A fresh install is NOT config-less — the defaults ship `[providers.fake]`
/// with both ports naming it, and that is deliberate: it is what makes the
/// whole stack, search included, work before anyone has a key. So "no provider
/// configured" would be false, and reporting it would teach the reader
/// something untrue.
///
/// What IS true, and is what an operator on a fresh machine needs to hear: no
/// REAL provider is configured, so everything indexed will be embedded and
/// summarised by a deterministic stand-in. That is fine if it was chosen and
/// surprising if it was not — which is exactly the shape of a warn rather than
/// an error. Doctor does not know which, so it reports and points at the page
/// that explains the choice.
///
/// Never repaired: choosing a model and supplying its credentials is a
/// decision, and a diagnostic command must not make it for you.
fn check_providers(config: &Config) -> Step {
    let started = Instant::now();
    let mut findings = Vec::new();
    let mut fake_ports = Vec::new();

    for port in Port::ALL {
        let selected = config.selected(port, None);
        match config.provider(selected) {
            // An active naming an instance that is not in the registry. The
            // daemon refuses to start on this, so catching it here — where the
            // fix is printable — beats meeting it as a boot failure.
            Err(_) => findings.push(format!(
                "{port}.active names {selected:?}, which is not in [providers]"
            )),
            Ok(ProviderInstance::Fake) => fake_ports.push(port.to_string()),
            Ok(instance) => {
                // A real provider whose key variable is unset fails at the
                // first call, deep inside a job, hours into an index. Naming it
                // now costs one environment lookup.
                if let Some(variable) = instance.api_key_env()
                    && std::env::var_os(variable).is_none()
                {
                    findings.push(format!(
                        "{port} uses {selected:?} ({}), whose key variable ${variable} is not set",
                        instance.kind()
                    ));
                }
            }
        }
    }

    if !findings.is_empty() {
        return Step::warn(
            "providers",
            findings.join("; "),
            "run `flowspace3 docs get providers` — it covers the registry, both Azure auth \
             modes, and setting actives",
            started,
        )
        .with_steer(
            "a provider selection is unusable — `flowspace3 docs get providers` explains the \
             registry and how to set the actives",
        );
    }

    if fake_ports.len() == Port::ALL.len() {
        return Step::warn(
            "providers",
            "no real provider is configured — both ports use the offline fake, so everything \
             indexed is embedded and summarised by a deterministic stand-in",
            "if that is deliberate, nothing to do. Otherwise run `flowspace3 docs get \
             providers` to register one",
            started,
        )
        .with_steer(
            "everything is up, but indexing would use the offline fake — `flowspace3 docs get \
             providers` explains how to register a real one, or carry on if offline is what you \
             wanted",
        );
    }

    let described: Vec<String> = Port::ALL
        .iter()
        .map(|port| {
            let name = config.selected(*port, None);
            let kind = config
                .provider(name)
                .map_or("unknown", ProviderInstance::kind);
            format!("{port}={name} ({kind})")
        })
        .collect();
    Step::ok("providers", described.join(", "), started)
}

/// The spec-verbatim asks of the skills row (req-0053).
const SKILL_MISSING_STEER: &str =
    "Did you know you can install the agent skill? Run: `flowspace3 doctor install-skill`";

const SKILL_STALE_STEER: &str = "Your skill is out of date; run the same command.";

/// Shape the skills row from audit states.
///
/// Pure: states in, row out — the spec-verbatim strings live here and only
/// here, where tests can reach them. Mixed states take the stronger ask: a
/// missing copy is the bigger gap, and its ask names the command outright.
fn skills_row(states: &[crate::skill::RootState], started: Instant) -> Step {
    let (missing, stale) = states
        .iter()
        .fold((0usize, 0usize), |(missing, stale), state| match state {
            crate::skill::RootState::Missing => (missing + 1, stale),
            crate::skill::RootState::Stale => (missing, stale + 1),
            crate::skill::RootState::Current => (missing, stale),
        });
    let asks = match (missing, stale) {
        (0, 0) => None,
        (_, 0) => Some(SKILL_MISSING_STEER),
        (0, _) => Some(SKILL_STALE_STEER),
        (_, _) => Some(SKILL_MISSING_STEER),
    };
    let found = match (missing, stale) {
        (0, 0) => "the agent skill is installed and current in both skills roots".to_string(),
        (missing, 0) => format!("the agent skill is missing from {missing} of 2 skills roots"),
        (0, stale) => {
            format!("the installed agent skill is out of date in {stale} of 2 skills roots")
        }
        (missing, stale) => format!(
            "the agent skill is missing from {missing} and out of date in {stale} of 2 skills roots"
        ),
    };
    let step = Step::info(
        "skills",
        found,
        asks.map_or_else(|| "nothing to do".to_string(), ToOwned::to_owned),
        started,
    );
    match asks {
        Some(steer) => step.with_steer(steer),
        None => step,
    }
}

/// The skill-distribution row: informational, walked last, never degrading.
///
/// Doctor never installs (req-0053). This reports where installed copies of
/// the bundled skill stand under the skills roots and names the explicit
/// command when they do not. `$HOME` resolution is the only impure step; the
/// shaping is `skills_row`, pure and tested.
fn check_skills() -> Step {
    let started = Instant::now();
    match std::env::var_os("HOME") {
        Some(home) => skills_row(&crate::skill::audit(std::path::Path::new(&home)), started),
        None => Step::info(
            "skills",
            "no skills roots locatable: HOME is not set",
            SKILL_MISSING_STEER,
            started,
        )
        .with_steer(SKILL_MISSING_STEER),
    }
}

/// The name doctor writes and deletes to prove a log directory is writable.
///
/// Not the log file itself: doctor reports, and creating an empty
/// `flowspace3.log` on a machine where no daemon has ever run would be doctor
/// inventing the very thing it was asked to look for.
const LOG_WRITE_PROBE: &str = ".fs3-log-probe";

/// The daemon's log destination — where evidence goes, and whether it can get
/// there.
///
/// This row exists because of a specific incident: on 2026-08-27 the summarize
/// lane panicked and the only copy of the evidence was a terminal's
/// scrollback. "Where are the logs" now has an answer you can read off a
/// command rather than infer from source.
///
/// Never repaired, and deliberately: the daemon creates its own log directory
/// at startup, so a doctor that created one would be reporting on its own
/// handiwork rather than on what the daemon will find.
fn check_logs(config: &Config) -> Step {
    let started = Instant::now();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);

    let directory = match fs3_core::resolve_log_dir(&config.daemon.log_dir, home.as_deref()) {
        Ok(directory) => directory,
        Err(reason) => {
            let steer = "set `[daemon] log_dir` to an absolute path — the configured one needs a \
                         home directory and this environment has none";
            return Step::warn("logs", reason, steer, started).with_steer(steer);
        }
    };

    let file = directory.join(fs3_core::LOG_FILE_NAME);
    let ceiling = config.daemon.log_max_bytes * u64::from(config.daemon.log_max_files);

    if !directory.is_dir() {
        // "Does not exist yet" would be a lie about a path that exists and is
        // a FILE — and that is a real typo, not a hypothetical: the daemon
        // cannot create a directory under it and will log to stdout alone.
        if directory.exists() {
            const STEER: &str = "point `[daemon] log_dir` at a directory: the configured path \
                                 exists and is not one, so the daemon cannot create its log \
                                 directory there";
            return Step::warn(
                "logs",
                format!("{} exists and is not a directory", directory.display()),
                STEER,
                started,
            )
            .with_steer(STEER);
        }

        return Step::info(
            "logs",
            format!("{} does not exist yet", directory.display()),
            format!(
                "the daemon creates it at startup and logs to {} (at most {} files, {ceiling} \
                 bytes in total)",
                file.display(),
                config.daemon.log_max_files
            ),
            started,
        );
    }

    if let Err(error) = probe_writable(&directory) {
        const STEER: &str = "point `[daemon] log_dir` at a writable directory (or fix its permissions): the \
             daemon will log to stdout only, and nothing will survive the process";
        return Step::warn(
            "logs",
            format!("{} is not writable ({error})", directory.display()),
            STEER,
            started,
        )
        .with_steer(STEER);
    }

    let kept = kept_log_files(&directory, config.daemon.log_max_files);
    let found = match std::fs::metadata(&file) {
        Ok(metadata) => format!(
            "{} ({} bytes, {kept} of at most {} files)",
            file.display(),
            metadata.len(),
            config.daemon.log_max_files
        ),
        // The directory is there and writable but nothing has been written:
        // a machine where the daemon has not run since logging was configured.
        Err(_) => format!("{} (not written yet)", file.display()),
    };

    Step::ok("logs", found, started)
}

/// Prove a directory is writable by writing in it, then clean up.
///
/// A permissions bit is not proof — a directory can be mode 755 and owned by
/// root, or sit on a read-only mount — so this does the only thing that
/// actually answers the question.
fn probe_writable(directory: &std::path::Path) -> std::io::Result<()> {
    let probe = directory.join(LOG_WRITE_PROBE);
    std::fs::write(&probe, b"")?;
    // A probe that cannot be removed is not a failure of the thing being
    // tested: the write succeeded, which is what was asked.
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// How many of the daemon's log files are on disk right now.
///
/// Counted by NAME rather than by listing the directory, so an unrelated file
/// somebody parked beside the logs is never counted as one.
fn kept_log_files(directory: &std::path::Path, max_files: u32) -> u32 {
    (0..max_files)
        .filter(|generation| directory.join(fs3_core::rolled_name(*generation)).exists())
        .count()
        .try_into()
        .unwrap_or(max_files)
}

/// The engine to drive.
fn engine() -> String {
    std::env::var(ENGINE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ENGINE.to_string())
}

/// Map a store failure onto the catalog.
///
/// Duplicated from the daemon's mapping on purpose, and it is a small
/// duplication with a reason: sharing it would mean `fs3-cli` depending on
/// `fs3-daemon`, which would put the whole HTTP surface inside the CLI binary
/// to reuse eight lines.
fn map_store(error: fs3_store::StoreError) -> Failure {
    if fs3_store::is_missing_database(&error) {
        return Failure::new(&catalog::STORE_DATABASE_MISSING, error.to_string());
    }
    match &error {
        fs3_store::StoreError::Unreachable { .. } => {
            Failure::new(&catalog::STORE_UNAVAILABLE, error.to_string())
        }
        fs3_store::StoreError::InvalidName(_) => {
            Failure::new(&catalog::CONFIG_INVALID, error.to_string())
        }
        _ => Failure::new(&catalog::STORE_QUERY_FAILED, error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_is_overridable_and_defaults_to_docker() {
        // Not a test of std::env — a test that an empty override does not
        // become an empty command, which would fail with a message naming
        // nothing at all.
        assert_eq!(DEFAULT_ENGINE, "docker");
        assert!(!engine().is_empty());
    }

    /// Each row says what was checked, what was found, and what was done. A row
    /// that only says "ok" is a row a reader cannot verify.
    #[test]
    fn a_repaired_step_records_both_the_finding_and_the_action() {
        let step = Step::repaired(
            "database",
            "flowspace3 did not exist",
            "created it",
            Instant::now(),
        );
        assert_eq!(step.outcome, "repaired");
        assert_eq!(step.found, "flowspace3 did not exist");
        assert_eq!(step.action.as_deref(), Some("created it"));

        let fine = Step::ok("database", "flowspace3 exists", Instant::now());
        assert_eq!(fine.outcome, "ok");
        assert!(
            fine.action.is_none(),
            "nothing was done, so nothing is claimed"
        );
    }

    /// A config whose log directory is `path`, and nothing else unusual.
    fn config_logging_to(path: &std::path::Path) -> Config {
        Config {
            daemon: fs3_core::DaemonConfig {
                log_dir: path.display().to_string(),
                ..fs3_core::DaemonConfig::default()
            },
            ..Config::default()
        }
    }

    /// The row a user reads on a machine where the daemon has already run: the
    /// active path, named, so "where do I look" needs no source-reading.
    #[test]
    fn the_logs_row_names_the_active_file_when_it_can_be_written() {
        let directory = tempfile::tempdir().expect("a temp dir");
        std::fs::write(directory.path().join("flowspace3.log"), b"an event\n").expect("a log");

        let row = check_logs(&config_logging_to(directory.path()));

        assert_eq!(row.check, "logs");
        assert_eq!(row.outcome, "ok");
        assert!(
            row.found.contains("flowspace3.log"),
            "the row must name the file: {row:?}"
        );
        assert!(!row.degrades(), "a healthy log is not a degraded stack");
    }

    /// Doctor reports; it does not create. A machine where the daemon has never
    /// run must still learn where the logs WILL be, and must not come back
    /// from a diagnostic command with a new empty directory on disk.
    #[test]
    fn a_log_directory_that_does_not_exist_yet_is_reported_not_created() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let planned = directory.path().join("not-yet");

        let row = check_logs(&config_logging_to(&planned));

        assert_eq!(row.outcome, "info", "nothing is wrong yet: {row:?}");
        assert!(!row.degrades());
        assert!(row.found.contains("not-yet"), "{row:?}");
        assert!(
            !planned.exists(),
            "doctor must not create the directory it is reporting on"
        );
    }

    /// The unwritable case is the one the packet exists for: it has to be
    /// visible, it has to degrade, and it has to steer.
    #[test]
    fn an_unwritable_log_directory_warns_and_steers() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let blocked = directory.path().join("blocked");
        std::fs::create_dir(&blocked).expect("the directory");
        let mut permissions = std::fs::metadata(&blocked).expect("stat").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&blocked, permissions).expect("making it read-only");

        let row = check_logs(&config_logging_to(&blocked));

        assert_eq!(row.outcome, "warn", "{row:?}");
        assert!(row.degrades(), "evidence going nowhere is a degraded stack");
        assert!(
            row.steer.is_some_and(|steer| steer.contains("log_dir")),
            "the steer must name the key to change"
        );
    }

    /// A `log_dir` that names an existing FILE is a typo with a specific
    /// consequence, and the row must say which one rather than reporting the
    /// path as merely absent.
    #[test]
    fn a_log_dir_that_is_a_file_says_so_rather_than_calling_it_missing() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let occupied = directory.path().join("logs");
        std::fs::write(&occupied, b"not a directory").expect("the blocker");

        let row = check_logs(&config_logging_to(&occupied));

        assert_eq!(row.outcome, "warn", "{row:?}");
        assert!(row.found.contains("not a directory"), "{row:?}");
    }

    /// The warning must reach the same doctor row channel as every other
    /// actionable finding; retaining it only in `Effective` would still leave
    /// a mistyped section invisible to the operator.
    #[test]
    fn an_ignored_config_section_reaches_a_warning_row() {
        let effective = fs3_core::resolve(fs3_core::Sources {
            file_label: "/tmp/fs3/config.toml",
            file_text: Some("[typo]\nactive = \"big\"\n"),
            env: &[],
        })
        .unwrap();

        let row = check_config(&effective).expect("the unknown section must produce a row");
        assert_eq!(row.check, "config");
        assert_eq!(row.outcome, "warn");
        assert!(row.found.contains("[typo]"), "{row:?}");
        assert!(row.found.contains("ignored"), "{row:?}");
        assert!(row.degrades(), "an ignored section must be loud");
        assert!(
            row.steer.is_some(),
            "the warning must tell the user what next"
        );
    }

    /// A config warning is actionable, but it must not outrank the runtime
    /// dependency that makes the product unusable. This combined state is the
    /// regression: testing either row alone cannot prove priority.
    #[test]
    fn a_down_daemon_steers_before_an_ignored_config_section() {
        let effective = fs3_core::resolve(fs3_core::Sources {
            file_label: "/tmp/fs3/config.toml",
            file_text: Some("[typo]\nactive = \"big\"\n"),
            env: &[],
        })
        .unwrap();
        let daemon = Step::down(
            "daemon",
            "nothing is listening on http://127.0.0.1:7373",
            Instant::now(),
        )
        .with_steer(DAEMON_DOWN_STEER);
        let skills = Step::info("skills", "current", "nothing to do", Instant::now());
        let mut steps = vec![daemon, skills];
        insert_config_warning(&mut steps, check_config(&effective));

        assert_eq!(steps[1].check, "config", "{steps:?}");
        assert_eq!(steps[2].check, "skills", "skills must remain last");
        let observed = next_action(&steps);
        assert_eq!(observed, DAEMON_DOWN_STEER);
        assert!(!observed.contains("unknown config"), "{observed}");
    }

    /// req-0053: the skills row's asks are spec-verbatim; mixed states take the
    /// stronger ask. Pure: fabricated states in, row out.
    #[test]
    fn the_skills_row_shapes_its_states_verbatim() {
        let started = Instant::now();

        let clean = skills_row(
            &[
                crate::skill::RootState::Current,
                crate::skill::RootState::Current,
            ],
            started,
        );
        assert_eq!(clean.outcome, "info");
        assert!(clean.steer.is_none(), "nothing to ask, nothing to steer");

        let missing = skills_row(
            &[
                crate::skill::RootState::Missing,
                crate::skill::RootState::Missing,
            ],
            started,
        );
        assert_eq!(missing.action.as_deref(), Some(SKILL_MISSING_STEER));
        assert_eq!(missing.steer.as_deref(), Some(SKILL_MISSING_STEER));

        let stale = skills_row(
            &[
                crate::skill::RootState::Stale,
                crate::skill::RootState::Stale,
            ],
            started,
        );
        assert_eq!(stale.action.as_deref(), Some(SKILL_STALE_STEER));
        assert_eq!(stale.steer.as_deref(), Some(SKILL_STALE_STEER));

        let mixed = skills_row(
            &[
                crate::skill::RootState::Missing,
                crate::skill::RootState::Stale,
            ],
            started,
        );
        assert_eq!(
            mixed.action.as_deref(),
            Some(SKILL_MISSING_STEER),
            "mixed states take the stronger ask"
        );
    }
}
