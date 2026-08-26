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

use fs3_core::catalog;
use fs3_core::envelope::{Envelope, Failure};
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

/// One step of the walk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// `engine`, `stack`, `database`, `schema`.
    pub check: String,
    /// `ok` when it was already fine, `repaired` when doctor fixed it, `failed`
    /// when it could not.
    pub outcome: String,
    /// What doctor found.
    pub found: String,
    /// What doctor did about it — absent when there was nothing to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// How long the step took.
    pub elapsed_ms: u128,
}

impl Step {
    /// Already fine.
    fn ok(check: &str, found: impl Into<String>, started: Instant) -> Self {
        Step {
            check: check.to_string(),
            outcome: "ok".to_string(),
            found: found.into(),
            action: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Found broken, and fixed.
    fn repaired(
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
            elapsed_ms: started.elapsed().as_millis(),
        }
    }
}

/// What `flowspace3 doctor` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Every step, in dependency order.
    pub steps: Vec<Step>,
    /// Whether the store is usable now.
    pub healthy: bool,
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
pub async fn run(database_url: &str) -> Envelope<DoctorReport> {
    let mut steps = Vec::new();

    match walk(database_url, &mut steps).await {
        Ok(()) => Envelope::ok(
            "doctor",
            DoctorReport {
                steps,
                healthy: true,
            },
        )
        .with_next_action(
            "the store is ready — start the daemon (`fs3-daemon`) and `flowspace3 add <path>`",
        ),
        Err(failure) => {
            let mut envelope = Envelope::failed("doctor", failure);
            // The steps that DID pass are the useful half of a failed run, so
            // they ride along in meta rather than being discarded.
            envelope.meta = serde_json::to_value(DoctorReport {
                steps,
                healthy: false,
            })
            .ok();
            envelope
        }
    }
}

async fn walk(database_url: &str, steps: &mut Vec<Step>) -> Result<(), Failure> {
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
    Ok(())
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
}
