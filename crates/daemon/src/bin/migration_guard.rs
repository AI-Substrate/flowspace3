//! `fs3-migration-guard` — snapshot the schema version of the database this
//! machine calls PRODUCTION, so a test run that writes to it cannot go
//! unnoticed.
//!
//! # Why a snapshot and not another rule
//!
//! Every other defence here is a rule about a KNOWN leak path:
//! `fs3_testkit::database` gates pool-opening tests, `fs3_testkit::spawn` gates
//! subprocess-spawning ones, `spawn_isolation.rs` refuses hand-built spawn
//! environments, and `fs3_daemon::boot` refuses to migrate a store nobody
//! chose. Each of those was written the day AFTER something got through.
//!
//! Twice now the leak came through a door nobody had thought to guard. The
//! first incident (migrations 0008/0009) went through the test helpers, and the
//! PR #18 gate closed that path — then the second (migration 0012, sixteen
//! seconds after the test database got it) went through a test that never calls
//! those helpers at all. A rule cannot refuse a shape it has not been told
//! about.
//!
//! A snapshot can. The harness asks the production database one question before
//! `cargo test --all` and the same question after, and fails the run on any
//! difference — whatever caused it, through whatever path, including one that
//! does not exist yet. That is what makes the breach class un-shippable rather
//! than merely un-repeated.
//!
//! # What it does NOT do
//!
//! It never writes, never migrates, never creates. It reads one number. A guard
//! that repairs things is a guard that can cause the incident it watches for —
//! which is exactly what `flowspace3 doctor` would be if the gate called it,
//! since doctor applies migrations by design (and that is how the FIRST
//! incident happened).
//!
//! # Output
//!
//! One line on stdout, compared by the harness as an opaque string:
//!
//! * `version=<n>` — the highest applied migration.
//! * `absent` — nothing reachable, or no migrations applied. Not an error: a
//!   machine with no production store has nothing to protect, and `absent`
//!   before and after is a passing comparison.
//! * `same-as-test` — the configured URL IS `FS3_TEST_DATABASE_URL`, so there
//!   is no production database distinct from the test one. This is the normal
//!   CI shape, where the shipped default legitimately names a disposable
//!   service container — and it is why the gate cannot just compare the URL
//!   against `DatabaseConfig::DEFAULT_URL`.
//!
//! All three exit 0: this binary REPORTS and the harness decides. A non-zero
//! exit means the question could not be asked, which is a broken gate rather
//! than a caught incident.

use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    match snapshot() {
        Ok(line) => {
            println!("{line}");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("fs3-migration-guard: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn snapshot() -> Result<String, String> {
    let url = configured_database_url()?;

    // Checked before connecting: when the test database and the configured one
    // are the same database, `cargo test --all` migrating it is correct
    // behaviour, and a diff would be a false alarm on every CI run.
    if let Ok(test_url) = std::env::var(fs3_testkit::TEST_DATABASE_ENV)
        && !test_url.trim().is_empty()
        && test_url.trim() == url.trim()
    {
        return Ok("same-as-test".to_string());
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("starting a runtime: {error}"))?
        .block_on(read_version(&url))
}

/// The highest applied migration, or `absent`.
///
/// Deliberately total in the "no schema here" direction: an unreachable host, a
/// missing database and a database with no migrations table are the SAME answer
/// for this gate — nothing to protect. Turning any of them into a failure would
/// make the gate fail closed on every machine without a production store.
///
/// Goes through `fs3_store`'s typed API rather than sqlx: this crate does not
/// ship sqlx (arch-allowlist), and the whole point is to ask the same question
/// the daemon's own boot path asks, of the same schema table.
async fn read_version(url: &str) -> Result<String, String> {
    // Lazy: nothing is dialled until the query below, and the pool's own
    // acquire timeout bounds the wait for a host that is not there.
    let pool = fs3_store::connect_lazy(url).map_err(|error| format!("{error}"))?;

    let answer = match fs3_store::schema_current(&pool).await {
        Ok(status) => status.applied.last().map_or_else(
            || "absent".to_string(),
            |version| format!("version={version}"),
        ),
        Err(_) => "absent".to_string(),
    };

    pool.close().await;
    Ok(answer)
}

/// The store URL a real `flowspace3` on this machine would use.
///
/// The same three layers, in the same order, through the same
/// [`fs3_core::resolve`] the daemon and the CLI use — because a probe that
/// resolved configuration its own way would guard a database nothing else
/// writes to, and report a clean run while production was being migrated.
fn configured_database_url() -> Result<String, String> {
    let directory = fs3_core::resolve_config_dir(
        std::env::var_os(fs3_core::CONFIG_DIR_ENV).as_deref(),
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
    )?;

    let path = directory.join(fs3_core::CONFIG_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };

    let label = path.display().to_string();
    let env = fs3_core::env_overrides(std::env::vars());
    let effective = fs3_core::resolve(fs3_core::Sources {
        file_label: &label,
        file_text: text.as_deref(),
        env: &env,
    })
    .map_err(|error| format!("configuration from {}: {error}", path.display()))?;

    Ok(effective.config.database.url)
}
