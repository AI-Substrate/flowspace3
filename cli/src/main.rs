//! `flowspace3` — the fs3 command-line client (PRD req 28).

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use fs3_cli::{DaemonClient, daemon_url};

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
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // `{error:#}` prints the whole anyhow context chain on one line,
            // so the doctor suggestion is never truncated away.
            eprintln!("flowspace3: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Ping { daemon_url: url } => ping(url).await,
    }
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
