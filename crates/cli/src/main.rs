//! `flowspace3` — the fs3 command-line client (PRD req 28).
//!
//! Every verb prints one workshop-004 envelope to stdout and exits by its
//! shape: 0 for `ok`, 1 for an error, 2 for a usage problem. JSON only in v1
//! (workshop 003 D5) — a human-readable layer renders from the same envelope
//! later, and building it now would mean two output paths to keep honest
//! instead of one.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fs3_cli::{DaemonClient, daemon_url, doctor, settings, show, skill};
use fs3_core::envelope::Envelope;

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
    /// Register a repository or folder and index it.
    Add {
        /// The directory to index.
        path: PathBuf,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Re-scan a root that is already registered.
    Scan {
        /// The registered directory to re-scan.
        path: PathBuf,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Report registered roots and what is left in the queue.
    Status {
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Ask a question of the index.
    Search {
        /// The question.
        query: String,
        /// Only this repository identity.
        #[arg(long, value_name = "IDENTITY")]
        repo: Option<String>,
        /// Only paths matching this glob.
        #[arg(long, value_name = "GLOB")]
        path: Option<String>,
        /// How many hits.
        #[arg(long, value_name = "N")]
        limit: Option<i64>,
        /// Similarity floor, 0.0-1.0.
        #[arg(long, value_name = "SCORE")]
        min_score: Option<f64>,
        /// Which vector space to search.
        #[arg(long, value_name = "SOURCE", value_parser = ["raw", "smart", "all"])]
        source: Option<String>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Diagnose the stack and repair what can be repaired.
    ///
    /// Walks engine -> stack -> database -> schema, starting the compose stack,
    /// creating the database and applying migrations as needed. This is the one
    /// command that talks to Postgres directly: it is the verb you run when the
    /// daemon is down, so it cannot be a client of it.
    ///
    /// `doctor install-skill` does not diagnose: it installs or updates the
    /// bundled agent skill into the agent skills roots (PRD req-0053). The
    /// diagnostic walk never installs — it only reports, so the skill reaches
    /// an agent's home directories by an explicit ask, never silently or by
    /// force.
    Doctor {
        /// Read this directory instead of `$FS3_CONFIG_DIR` or
        /// `~/.config/flowspace3`.
        #[arg(long, value_name = "DIR")]
        config_dir: Option<PathBuf>,
        /// Install or update the bundled agent skill.
        #[command(subcommand)]
        command: Option<DoctorCommand>,
    },
    /// Run the fs3 daemon in the foreground.
    ///
    /// The daemon lives in THIS binary (PRD req 51): one file to install, one
    /// version, and no way for a CLI and a daemon of different vintages to
    /// meet. It serves HTTP on `daemon.url`, migrates the store at boot, and
    /// drains the job queue until stopped.
    Daemon,
    /// Orient an agent that has just installed fs3: print the bundled agents
    /// guide — setup from scratch through doctor, provider and config
    /// creation, daemon, add and search (PRD req-0055).
    ///
    /// One obvious verb, answered offline like `docs`: no daemon, no network.
    /// It is a front door onto `docs get agents` with the steer replaced by
    /// the next operational step, so an agent that runs it lands pointed at
    /// `flowspace3 doctor` rather than at the topic list.
    AgentsStartHere,
    /// Read the documentation bundled into this binary.
    ///
    /// Answers offline, with no daemon and no network: an agent that has just
    /// installed fs3 can ask fs3 how to use fs3 before the stack is up.
    Docs {
        #[command(subcommand)]
        command: DocsCommand,
    },
    /// Inspect fs3's configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum DocsCommand {
    /// List every bundled topic.
    List,
    /// Print one bundled topic.
    Get {
        /// The topic name, as `docs list` reports it.
        topic: String,
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

/// Subcommands of `doctor` that are not the diagnostic walk.
#[derive(Subcommand)]
enum DoctorCommand {
    /// Install or update the bundled agent skill into `~/.agents/skills` and
    /// `~/.claude/skills`.
    ///
    /// Explicit by design (PRD req-0053): nothing writes these files silently or
    /// by force, and the diagnostic walk only ever reports their state.
    InstallSkill,

    /// Check for a newer release and install it now, whatever the update
    /// interval says.
    ///
    /// The same engine the daemon runs on its own schedule (PRD req 54) — this
    /// is the force-it-now path, and the one to reach for when the automatic
    /// one reported that it could not write the install path.
    Upgrade,
}

/// Exit codes, per workshop 004: 0 ok, 1 error, 2 usage.
const EXIT_ERROR: u8 = 1;

/// Not `#[tokio::main]`: the secrets chain writes `secrets.env` into the
/// process environment, which is only sound while the process is
/// single-threaded. Secrets load first, the runtime starts after.
fn main() -> ExitCode {
    let outcome = boot().and_then(|command| {
        // `daemon` is the one verb that must NOT run inside a runtime this
        // function built: it builds its own, sized for a server rather than for
        // one request, and it never returns. Routing it here rather than in
        // `run` is what keeps that true.
        if matches!(command, Command::Daemon) {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "fs3_daemon=info,tower_http=info".into()),
                )
                .init();
            return fs3_daemon::run().map(|()| ExitCode::SUCCESS);
        }

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("starting the Tokio runtime")?
            .block_on(run(command))
    });

    match outcome {
        Ok(code) => code,
        Err(error) => {
            // `{error:#}` prints the whole anyhow context chain on one line,
            // so the doctor suggestion is never truncated away.
            eprintln!("flowspace3: {error:#}");
            ExitCode::from(EXIT_ERROR)
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

async fn run(command: Command) -> Result<ExitCode> {
    match command {
        Command::Ping { daemon_url: url } => ping(url).await,
        Command::Add { path, daemon_url } => {
            let client = client_for(daemon_url)?;
            Ok(emit(&client.add(&display(&path)).await))
        }
        Command::Scan { path, daemon_url } => {
            let client = client_for(daemon_url)?;
            Ok(emit(&client.scan(&display(&path)).await))
        }
        Command::Status { daemon_url } => {
            let client = client_for(daemon_url)?;
            Ok(emit(&client.status().await))
        }
        Command::Search {
            query,
            repo,
            path,
            limit,
            min_score,
            source,
            daemon_url,
        } => {
            let client = client_for(daemon_url)?;
            let mut params = vec![("q".to_string(), query)];
            push(&mut params, "repo", repo);
            push(&mut params, "path", path);
            push(&mut params, "limit", limit.map(|v| v.to_string()));
            push(&mut params, "min_score", min_score.map(|v| v.to_string()));
            push(&mut params, "source", source);
            Ok(emit(&client.search(&params).await))
        }
        Command::Doctor {
            config_dir: _,
            command: Some(DoctorCommand::InstallSkill),
        } => Ok(emit(&skill::install()?)),
        Command::Doctor {
            config_dir,
            command: Some(DoctorCommand::Upgrade),
        } => {
            let dir = match config_dir {
                Some(dir) => dir,
                None => settings::config_dir()?,
            };
            let effective = settings::load_effective_from(&dir)?;
            Ok(emit(&fs3_cli::upgrade::upgrade(&effective.config).await))
        }
        Command::Doctor {
            config_dir,
            command: None,
        } => {
            let dir = match config_dir {
                Some(dir) => dir,
                None => settings::config_dir()?,
            };
            let effective = settings::load_effective_from(&dir)?;
            Ok(emit(&doctor::run(&effective.config).await))
        }
        Command::Docs { command } => Ok(match command {
            DocsCommand::List => emit(&fs3_cli::docs::list()),
            DocsCommand::Get { topic } => emit(&fs3_cli::docs::get(&topic)),
        }),
        Command::AgentsStartHere => {
            let envelope = fs3_cli::docs::get("agents");
            let envelope = match envelope.data {
                Some(page) => Envelope::ok("agents-start-here", page).with_next_action(
                    "the guide above ends where operation begins — `flowspace3 doctor` \
                     diagnoses what is missing and names the provider and config \
                     steps it finds",
                ),
                None => envelope,
            };
            Ok(emit(&envelope))
        }
        Command::Config {
            command: ConfigCommand::Show { config_dir },
        } => config_show(config_dir).map(|()| ExitCode::SUCCESS),
        // Routed before the runtime was built; see `main`.
        Command::Daemon => unreachable!("the daemon verb is handled in main"),
    }
}

/// Print an envelope and turn its `ok` into an exit code.
///
/// The envelope goes to STDOUT even when it is an error: it is the command's
/// answer, and a script piping stdout to `jq` must not have to also capture
/// stderr to find out what happened. The human-readable rendering of the same
/// failure goes to stderr, so a person reading a terminal sees the fix without
/// piping anything.
///
/// User messages (PRD req 59) get the same treatment for the same reason: they
/// are in the JSON for an agent, and on stderr for a person. Standing
/// conditions print BEFORE the failure, because a message like "a newer binary
/// is waiting for a restart" is very often the explanation for the failure
/// underneath it.
fn emit<T: serde::Serialize>(envelope: &Envelope<T>) -> ExitCode {
    match serde_json::to_string_pretty(envelope) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("flowspace3: cannot render the response: {error}"),
    }

    for message in &envelope.messages {
        eprintln!("flowspace3: {}", message.render());
    }

    match &envelope.error {
        None => ExitCode::SUCCESS,
        Some(failure) => {
            eprintln!("{}", failure.render());
            ExitCode::from(EXIT_ERROR)
        }
    }
}

fn push(params: &mut Vec<(String, String)>, name: &str, value: Option<String>) {
    if let Some(value) = value {
        params.push((name.to_string(), value));
    }
}

/// An absolute path, so the daemon never resolves a relative one against ITS
/// working directory.
///
/// This is the trap the `add` error message names, closed at the source: the
/// CLI knows where the user is standing and the daemon does not.
fn display(path: &PathBuf) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.clone())
        .to_string_lossy()
        .to_string()
}

fn client_for(override_url: Option<String>) -> Result<DaemonClient> {
    let url = match override_url {
        Some(url) => url,
        None => daemon_url()?,
    };
    DaemonClient::new(url)
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

async fn ping(override_url: Option<String>) -> Result<ExitCode> {
    let client = client_for(override_url)?;
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
    Ok(ExitCode::SUCCESS)
}
