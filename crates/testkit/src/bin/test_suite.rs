//! Whole-suite database isolation for `harness checks`.

use std::process::{Command, ExitCode};

use fs3_testkit::{FreshDatabase, TEST_DATABASE_ENV};

#[tokio::main]
async fn main() -> ExitCode {
    let base_url = match std::env::var(TEST_DATABASE_ENV) {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!("{}", fs3_testkit::refusal());
            return ExitCode::FAILURE;
        }
    };

    let sweep = match FreshDatabase::sweep_orphans_from(&base_url).await {
        Ok(report) => report,
        Err(error) => {
            eprintln!("orphan test database sweep failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "orphan test database sweep: threshold={}s swept={:?}",
        sweep.threshold.as_secs(),
        sweep.swept
    );

    let database = match FreshDatabase::create_from(&base_url, "test").await {
        Ok(database) => database,
        Err(error) => {
            eprintln!("minting the per-run test database failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("test database minted: {}", database.name());

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let child = Command::new(cargo)
        .args(["test", "--all"])
        .env(TEST_DATABASE_ENV, database.url())
        .status();

    let cleanup = database.cleanup().await;
    if let Err(error) = &cleanup {
        eprintln!("dropping the per-run test database failed: {error}");
    }

    match child {
        Ok(status) if status.success() && cleanup.is_ok() => ExitCode::SUCCESS,
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => {
            eprintln!("starting cargo test --all failed: {error}");
            ExitCode::FAILURE
        }
    }
}
