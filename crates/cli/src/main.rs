//! `flowspace3` — the fs3 command-line client (PRD req 28).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fs3_cli::{DaemonClient, daemon_url, settings, show};

#[derive(Parser)]
#[command(
    name = "flowspace3",
    version,
    about = "Semantic code search over a central index.",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that the fs3 daemon is up and answering.
    Ping {
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Inspect fs3's configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the effective configuration and the layer each section came from.
    ///
    /// Secrets are never printed: the database password is masked, and a
    /// provider's key variable is reported as set or not set, never by value.
    Show {
        /// Read this directory instead of `$FS3_CONFIG_DIR` or
        /// `~/.config/flowspace3`.
        #[arg(long, value_name = "DIR")]
        config_dir: Option<PathBuf>,
    },
}

/// Not `#[tokio::main]`: the secrets chain writes `secrets.env` into the
/// process environment, which is only sound while the process is
/// single-threaded. Secrets load first, the runtime starts after.
fn main() -> ExitCode {
    let outcome = boot().and_then(|command| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("starting the Tokio runtime")?
            .block_on(run(command))
    });

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // `{error:#}` prints the whole anyhow context chain on one line,
            // so the doctor suggestion is never truncated away.
            eprintln!("flowspace3: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Parse the command line and load the secrets chain, single-threaded.
fn boot() -> Result<Command> {
    let command = Cli::parse().command;
    if let Ok(dir) = settings::config_dir() {
        // A broken secrets file is worth failing on; a missing one is normal.
        settings::load_secrets_from(&dir)?;
    }
    Ok(command)
}

async fn run(command: Command) -> Result<()> {
    match command {
        Command::Ping { daemon_url: url } => ping(url).await,
        Command::Config {
            command: ConfigCommand::Show { config_dir },
        } => config_show(config_dir),
    }
}

fn config_show(override_dir: Option<PathBuf>) -> Result<()> {
    let dir = match override_dir {
        Some(dir) => dir,
        None => settings::config_dir()?,
    };
    let effective = settings::load_effective_from(&dir)?;

    print!(
        "{}",
        show::render(
            &effective,
            &dir,
            settings::config_path(&dir).exists(),
            settings::secrets_path(&dir).exists(),
        )
    );
    Ok(())
}

async fn ping(override_url: Option<String>) -> Result<()> {
    let url = match override_url {
        Some(url) => url,
        None => daemon_url()?,
    };
    let client = DaemonClient::new(url)?;
    let health = client.health().await?;

    if !health.is_healthy() {
        anyhow::bail!(
            "fs3 daemon at {} reports status {:?}, not \"ok\"",
            client.base_url(),
            health.status
        );
    }

    println!(
        "healthy - fs3 daemon {} at {} (embedder: {}, summarizer: {})",
        health.version,
        client.base_url(),
        health.embedder,
        health.summarizer
    );
    Ok(())
}
