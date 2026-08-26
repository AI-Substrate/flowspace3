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
use fs3_core::{Config, Port, ProviderInstance, catalog};
use serde::{Deserialize, Serialize};

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

/// One step of the walk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// `engine`, `stack`, `database`, `schema`, `daemon`, `providers`, …
    pub check: String,
    /// What the reader should DO about this row. The vocabulary is closed, and
    /// each word is a promise:
    ///
    /// | outcome | meaning | degrades the verdict? |
    /// |---|---|---|
    /// | `ok` | already fine | no |
    /// | `repaired` | was broken; doctor fixed it | no |
    /// | `info` | reported for awareness; nothing is wrong | **no** |
    /// | `warn` | working, but not as it should be; decide something | yes |
    /// | `down` | not running; start something | yes |
    ///
    /// `info` exists so a row can be *reported* without claiming the stack is
    /// unhealthy. Without it the only way to surface a finding was `warn`,
    /// which degrades — and a purely informational row degrading the whole
    /// verdict is louder than it means to be, which is its own kind of
    /// misleading.
    pub outcome: String,
    /// What doctor found.
    pub found: String,
    /// What doctor did about it, or what you should do — absent when there was
    /// nothing to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// This row's contribution to the envelope's `next_action`, when it is the
    /// most important unmet thing.
    ///
    /// Carried by the ROW rather than computed from a chain of check names, so
    /// a new row supplies its own steer without editing the steering logic —
    /// and so the steer can never drift from the finding that produced it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steer: Option<String>,
    /// How long the step took.
    pub elapsed_ms: u128,
}

impl Step {
    // The constructors are public because `Step` is a public struct with
    // public fields — anyone can build one with a literal, so a private
    // constructor bought nothing and only made another module reach for the
    // literal and miss a field default.

    /// Already fine.
    pub fn ok(check: &str, found: impl Into<String>, started: Instant) -> Self {
        Step {
            check: check.to_string(),
            outcome: "ok".to_string(),
            found: found.into(),
            action: None,
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Found working but not as it should be — a finding, not a failure.
    ///
    /// Distinct from `down` because the subject is not absent, it is
    /// misconfigured or running on a stand-in, and the reader's next move is
    /// different: `down` means start something, `warn` means decide something.
    pub fn warn(
        check: &str,
        found: impl Into<String>,
        action: impl Into<String>,
        started: Instant,
    ) -> Self {
        Step {
            check: check.to_string(),
            outcome: "warn".to_string(),
            found: found.into(),
            action: Some(action.into()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Reported for awareness. Nothing is wrong and the verdict is untouched.
    ///
    /// For rows that inform rather than diagnose — a thing the reader may want
    /// to act on, where not having acted is not a fault. Use `warn` when
    /// something is genuinely not as it should be.
    pub fn info(
        check: &str,
        found: impl Into<String>,
        action: impl Into<String>,
        started: Instant,
    ) -> Self {
        Step {
            check: check.to_string(),
            outcome: "info".to_string(),
            found: found.into(),
            action: Some(action.into()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Attach this row's contribution to the envelope's `next_action`.
    #[must_use]
    pub fn with_steer(mut self, steer: impl Into<String>) -> Self {
        self.steer = Some(steer.into());
        self
    }

    /// Whether this row asks anything of the reader.
    ///
    /// `ok` and `repaired` do not: one was already fine and the other doctor
    /// handled. Everything else is a row the reader may need to act on, which
    /// is what makes it eligible to steer.
    #[must_use]
    pub fn asks_something(&self) -> bool {
        !matches!(self.outcome.as_str(), "ok" | "repaired")
    }

    /// Whether this row means the stack is not fully up.
    ///
    /// `info` deliberately does not: it reports, it does not diagnose.
    #[must_use]
    pub fn degrades(&self) -> bool {
        matches!(self.outcome.as_str(), "warn" | "down")
    }

    /// Found not running, and deliberately not started.
    pub fn down(check: &str, found: impl Into<String>, started: Instant) -> Self {
        Step {
            check: check.to_string(),
            outcome: "down".to_string(),
            found: found.into(),
            action: Some("not started — run `flowspace3 daemon &`".to_string()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Found broken, and fixed.
    pub fn repaired(
        check: &str,
        found: impl Into<String>,
        action: impl Into<String>,
        started: Instant,
    ) -> Self {
        Step {
            check: check.to_string(),
            outcome: "repaired".to_string(),
            found: found.into(),
            action: Some(action.into()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }
}

/// What `flowspace3 doctor` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Every step, in dependency order.
    pub steps: Vec<Step>,
    /// Whether the STORE is usable now.
    pub healthy: bool,
    /// The whole stack's verdict: `ok`, or `degraded` when something doctor
    /// cannot repair for you is not running.
    ///
    /// Separate from `healthy` because they answer different questions, and
    /// conflating them is what made doctor say a plain "ok" on a machine with
    /// no daemon running (Jordan, live, 2026-08-26). The store really was fine;
    /// the stack was not usable. `ok: true` on the envelope stays either way —
    /// the COMMAND succeeded, and it is the subject it reports on that is
    /// degraded.
    pub verdict: String,
}

impl DoctorReport {
    /// Everything doctor checked is up.
    pub const OK: &'static str = "ok";
    /// Doctor ran fine; something it checked is not up.
    pub const DEGRADED: &'static str = "degraded";
}

/// Walk the chain, repairing as it goes.
///
/// Stops at the first step it cannot fix, because every later step depends on
/// it: probing a schema on a server that is not running produces a second,
/// less useful copy of the same failure.
///
/// # Errors
/// The failure of the step that could not be repaired, with its own catalog
/// code and fix.
pub async fn run(config: &Config) -> Envelope<DoctorReport> {
    let mut steps = Vec::new();

    match walk(config, &mut steps).await {
        Ok(messages) => {
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

/// Returns the live user messages the walk observed, so doctor's own envelope
/// carries them like every other command's does (req-0059).
async fn walk(
    config: &Config,
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
    // name the command. It runs last because everything above it is what the
    // daemon needs in order to start at all.
    steps.push(check_daemon(daemon_url).await);
    steps.push(check_providers(config));
    // req-0053: the skills row walks last, after providers — informational,
    // never degrading, and the one row reporting state doctor will never
    // itself change.
    steps.push(check_skills());
    // req-0054 / req-0059. Both read the store, so they walk after the schema
    // row that guarantees the tables exist. Neither ever repairs: doctor does
    // not update binaries behind your back — `doctor upgrade` is the verb that
    // does, and this row names it.
    steps.push(check_update(database_url, config).await);
    let (row, messages) = check_messages(database_url).await;
    steps.push(row);
    Ok(messages)
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
    let state = fs3_store::update_state(&pool).await;
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
        let path = state.install_path.as_deref().unwrap_or("the install path");
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
        let path = state.install_path.as_deref().unwrap_or("the install path");
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
    let messages = fs3_store::live_messages(&pool).await;
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
/// Reported, never repaired. Doctor starts the compose stack because a
/// container is a background service the user already asked for by configuring
/// it; a daemon is a FOREGROUND process, and spawning one from a diagnostic
/// command would leave something running that the user did not ask for and
/// cannot see. So this row says what it found and names the command.
///
/// Not an error either: a store that is ready with no daemon in front of it is
/// a perfectly good outcome for `doctor` to report — it is the state right
/// before you start one. The envelope stays `ok: true`, because the COMMAND
/// succeeded; the STACK is what the verdict calls degraded.
///
/// This row exists because its absence was actively misleading: doctor said a
/// plain "ok" on a machine with no daemon running (Jordan, live, 2026-08-26).
async fn check_daemon(daemon_url: &str) -> Step {
    let started = Instant::now();
    let url = format!("{}/health", daemon_url.trim_end_matches('/'));

    let probe = reqwest::Client::builder()
        .timeout(DAEMON_PROBE_TIMEOUT)
        .build()
        .map(|client| client.get(&url));

    let response = match probe {
        Ok(request) => request.send().await,
        Err(error) => {
            return Step::down(
                "daemon",
                format!("cannot probe {daemon_url}: {error}"),
                started,
            );
        }
    };

    match response {
        Ok(response) if response.status().is_success() => {
            // The version echo is free — it is already in the health body — and
            // it is the fastest way to see a stale binary still serving.
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
        .with_steer(
            "the store is ready but the daemon is not running — start it with `flowspace3 \
             daemon &`, then `flowspace3 add <path>`",
        ),
    }
}

/// Step 5: is a real provider configured, or is everything the offline fake?
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
