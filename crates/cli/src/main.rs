//! `flowspace3` — the fs3 command-line client (PRD req 28).
//!
//! Every verb produces one workshop-004 envelope and exits by its shape: 0 for
//! `ok`, 1 for an error, 2 for a usage problem. What stdout RECEIVES depends on
//! who is reading it: a terminal gets the human rendering, and a pipe, a file,
//! a CI log or an agent gets the frozen JSON envelope, byte for byte as before
//! (`fs3_core::output`, Jordan's ruling 2026-08-28). `--json` forces JSON
//! anywhere, `--human` forces the renderer anywhere, and `FS3_OUTPUT` settles
//! it for a harness whose terminal probe would lie — an agent inside a tmux
//! PTY looks exactly like a person.
//!
//! There is still one output path: the envelope is serialised first, in both
//! modes, and the human screen is rendered FROM those bytes
//! (`fs3_cli::render`). The two views cannot disagree, because one is made out
//! of the other.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fs3_cli::{DaemonClient, doctor, render, settings, show, skill, watch};
use fs3_core::envelope::Envelope;
use fs3_core::output::{OUTPUT_ENV, OutputMode};

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
    /// Print the JSON envelope, even at a terminal.
    ///
    /// Global so it can be written where a person naturally writes it — after
    /// the verb, next to the thing they are looking at.
    #[arg(long, global = true, conflicts_with = "human")]
    json: bool,
    /// Print the human rendering, even into a pipe.
    #[arg(long, global = true)]
    human: bool,
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
    /// Unregister a root: stop watching it, forget its files, and kill its
    /// queued scans.
    ///
    /// Indexed CONTENT is not deleted here. It is content-addressed and may be
    /// shared with another registered root, so what becomes unreferenced is
    /// reclaimed by garbage collection — on its own schedule, or now with
    /// `flowspace3 gc`.
    Remove {
        /// The registered directory to unregister.
        path: PathBuf,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Reclaim stored rows that no registered root references any more.
    ///
    /// The daemon does this on a slow schedule by itself; this runs a pass now
    /// and reports what it freed.
    Gc {
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Report registered roots and what is left in the queue.
    Status {
        /// Keep reading the daemon's live NDJSON event stream.
        #[arg(long)]
        watch: bool,
        /// Override the stream heartbeat cadence.
        #[arg(long, value_name = "MILLISECONDS", requires = "watch")]
        heartbeat_ms: Option<u64>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Open the live terminal dashboard.
    Tui {
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
        #[arg(long, value_name = "SOURCE", value_parser = ["raw", "smart", "conversation", "all"])]
        source: Option<String>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Read one address in full: an element with its children, or a whole file.
    ///
    /// The other half of the query surface (workshop 003). `search` finds an
    /// address; this reads what is at it — from the INDEX, so it answers for
    /// every registered repository, not only the one you are standing in.
    Get {
        /// `el:<repo>/<path>::<name>`, as printed by every search hit.
        address: String,
        /// How many levels of children to outline. Default 1.
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        /// The first line of the element to read, when several share an
        /// address (`struct Rect` and `impl Rect` are one address, two
        /// elements).
        #[arg(long, value_name = "LINE")]
        span: Option<u32>,
        /// How many turns before the addressed one to show, for a `conv:`
        /// address. Default 10.
        #[arg(long, value_name = "N")]
        before: Option<u32>,
        /// How many after it. Default 20 — what happened NEXT is usually what
        /// the reader wanted.
        #[arg(long, value_name = "N")]
        after: Option<u32>,
        /// Resolve a repo-less address in this repository, or `all`.
        #[arg(long, value_name = "IDENTITY")]
        repo: Option<String>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Browse what is indexed: repositories, directories, files, or one file's
    /// declarations.
    ///
    /// The navigation companion to `get`. With no target it shows where you
    /// are standing; with a path or an address it shows what is under it.
    Tree {
        /// An `el:` address, a repo-relative path, or an absolute path.
        target: Option<String>,
        /// How many levels to show. Default 2.
        #[arg(long, value_name = "N")]
        depth: Option<u32>,
        /// How many files to list before reporting a count instead.
        #[arg(long, value_name = "N")]
        limit: Option<i64>,
        /// Browse this repository, or `all`.
        #[arg(long, value_name = "IDENTITY")]
        repo: Option<String>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Store, list and remove indexed conversations (workshop 005).
    ///
    /// Conversations carry the WHY that code cannot: the rejected
    /// alternatives, the rulings, the debugging trail. `import` is the intake
    /// endpoint's first client — hand it a transcript and its turns become
    /// searchable content like any other, findable with
    /// `search --source conversation` and readable with `get conv:<guid>#t<n>`.
    Conversation {
        #[command(subcommand)]
        command: ConversationCommand,
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
enum ConversationCommand {
    /// Store a conversation from a JSONL transcript, or `-` for stdin.
    ///
    /// Append-friendly: re-importing a file that has GROWN stores only the
    /// turns that are new and enqueues enrichment only for those, so the
    /// obvious loop — import as you go — costs what it should.
    Import {
        /// The transcript, or `-` to read stdin.
        file: String,
        /// Reuse this conversation guid instead of minting one. Required to
        /// grow a conversation whose file does not name its own guid.
        #[arg(long, value_name = "UUID")]
        guid: Option<String>,
        /// Anchor the conversation to this repository identity, overriding
        /// what the working directory says.
        #[arg(long, value_name = "IDENTITY")]
        repo: Option<String>,
        /// Anchor it to this checkout path.
        #[arg(long, value_name = "PATH")]
        worktree: Option<String>,
        /// A title, when the transcript does not carry one.
        #[arg(long, value_name = "TEXT")]
        title: Option<String>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// List indexed conversations, newest first.
    List {
        /// Only conversations anchored to this repository identity.
        #[arg(long, value_name = "IDENTITY")]
        repo: Option<String>,
        /// Only conversations whose anchor checkout starts with this path.
        #[arg(long, value_name = "PREFIX")]
        path: Option<String>,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
    },
    /// Forget one conversation: its turns and their indexed content.
    ///
    /// Symmetric with `remove` for a root, and it stops in the same place: the
    /// summaries and vectors its turns paid for are keyed by content and may
    /// still be shared, so `gc` decides those on its own cadence.
    Remove {
        /// The conversation's guid.
        guid: String,
        /// Override the daemon URL from configuration.
        #[arg(long, value_name = "URL")]
        daemon_url: Option<String>,
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
            // The subscriber used to be built HERE, on stdout with a hardcoded
            // filter. It moved into `fs3_daemon::boot`, which is the first
            // place that has read the configuration — and the log file's path,
            // its size caps and its filter are all configuration.
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
        // The reader hung up. `flowspace3 search … | head` ends this way every
        // time, and it is a normal end to a command rather than a failure of
        // it: exit 0, say nothing (main, 086f812).
        Err(error) if is_broken_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            // `{error:#}` prints the whole anyhow context chain on one line,
            // so the doctor suggestion is never truncated away.
            match write_stderr(format_args!("flowspace3: {error:#}\n")) {
                Ok(()) => ExitCode::from(EXIT_ERROR),
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(EXIT_ERROR),
            }
        }
    }
}

/// How this process prints, decided once at startup.
///
/// A process-wide cell rather than a parameter threaded through twenty-five
/// `emit` call sites: "who is reading stdout" is a property of the PROCESS,
/// settled before the first byte and never changing mid-run. Threading it
/// would also mean every future verb author has to remember to pass it, which
/// is the kind of thing nobody remembers on the twenty-sixth arm.
///
/// Unset reads as [`OutputMode::Json`] — the safe direction, and the shape any
/// caller that bypassed `boot` (the daemon verb) would want anyway.
static OUTPUT: OnceLock<OutputMode> = OnceLock::new();

/// What stdout should receive, as `boot` resolved it.
fn output_mode() -> OutputMode {
    OUTPUT.get().copied().unwrap_or(OutputMode::Json)
}

/// Parse the command line, settle the output mode, and load the secrets chain,
/// single-threaded.
fn boot() -> Result<Command> {
    let cli = Cli::parse();

    // The terminal probe lives here and nowhere else: `fs3_core::output` is
    // pure and takes the answer as an argument, so the rule can be tested
    // exhaustively without a tty.
    let mode = fs3_core::output::resolve(
        std::io::stdout().is_terminal(),
        cli.json,
        cli.human,
        std::env::var(OUTPUT_ENV).ok().as_deref(),
    );
    let _ = OUTPUT.set(mode);

    if let Ok(dir) = settings::config_dir() {
        // A broken secrets file is worth failing on; a missing one is normal.
        settings::load_secrets_from(&dir)?;
    }
    Ok(cli.command)
}

async fn run(command: Command) -> Result<ExitCode> {
    match command {
        Command::Ping { daemon_url: url } => ping(url).await,
        Command::Add { path, daemon_url } => {
            let client = client_for(daemon_url)?;
            let path = display(&path);
            let envelope = match output_mode() {
                OutputMode::Human => {
                    render::progress::while_pending(&client, &path, client.add(&path)).await
                }
                OutputMode::Json => client.add(&path).await,
            };
            emit(&envelope)
        }
        Command::Remove { path, daemon_url } => {
            let client = client_for(daemon_url)?;
            emit(&client.remove(&display(&path)).await)
        }
        Command::Gc { daemon_url } => {
            let client = client_for(daemon_url)?;
            emit(&client.gc().await)
        }
        Command::Scan { path, daemon_url } => {
            let client = client_for(daemon_url)?;
            let path = display(&path);
            let envelope = match output_mode() {
                OutputMode::Human => {
                    render::progress::while_pending(&client, &path, client.scan(&path)).await
                }
                OutputMode::Json => client.scan(&path).await,
            };
            emit(&envelope)
        }
        Command::Status {
            watch: watching,
            heartbeat_ms,
            daemon_url,
        } => {
            let client = client_for(daemon_url)?;
            if watching {
                watch::run(&client, heartbeat_ms, output_mode() == OutputMode::Human).await?;
                Ok(ExitCode::SUCCESS)
            } else {
                emit(&client.status().await)
            }
        }
        Command::Tui { daemon_url } => {
            let client = client_for(daemon_url)?;
            fs3_cli::tui::run(client).await?;
            Ok(ExitCode::SUCCESS)
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
            push(&mut params, "cwd", here());
            emit(&client.search(&params).await)
        }
        Command::Get {
            address,
            depth,
            span,
            before,
            after,
            repo,
            daemon_url,
        } => {
            let client = client_for(daemon_url)?;
            let mut params = vec![("address".to_string(), address)];
            push(&mut params, "depth", depth.map(|v| v.to_string()));
            push(&mut params, "span", span.map(|v| v.to_string()));
            push(&mut params, "before", before.map(|v| v.to_string()));
            push(&mut params, "after", after.map(|v| v.to_string()));
            push(&mut params, "repo", repo);
            push(&mut params, "cwd", here());
            emit(&client.get(&params).await)
        }
        Command::Tree {
            target,
            depth,
            limit,
            repo,
            daemon_url,
        } => {
            let client = client_for(daemon_url)?;
            let mut params = Vec::new();
            push(&mut params, "address", target);
            push(&mut params, "depth", depth.map(|v| v.to_string()));
            push(&mut params, "limit", limit.map(|v| v.to_string()));
            push(&mut params, "repo", repo);
            push(&mut params, "cwd", here());
            emit(&client.tree(&params).await)
        }
        Command::Conversation {
            command:
                ConversationCommand::Import {
                    file,
                    guid,
                    repo,
                    worktree,
                    title,
                    daemon_url,
                },
        } => {
            let client = client_for(daemon_url)?;
            // The anchor defaults to where the caller is standing, which is
            // almost always the repository the conversation was about. An
            // explicit `--worktree` beats it, because the flag is the more
            // recent decision.
            let import =
                fs3_cli::conversation::read(&file, guid, repo, worktree.or_else(here), title)?;
            emit(&client.conversation_import(&import.body).await)
        }
        Command::Conversation {
            command:
                ConversationCommand::List {
                    repo,
                    path,
                    daemon_url,
                },
        } => {
            let client = client_for(daemon_url)?;
            let mut params = Vec::new();
            push(&mut params, "repo", repo);
            push(&mut params, "path", path);
            emit(&client.conversation_list(&params).await)
        }
        Command::Conversation {
            command: ConversationCommand::Remove { guid, daemon_url },
        } => {
            let client = client_for(daemon_url)?;
            emit(&client.conversation_remove(&guid).await)
        }
        Command::Doctor {
            config_dir: _,
            command: Some(DoctorCommand::InstallSkill),
        } => emit(&skill::install()?),
        Command::Doctor {
            config_dir,
            command: Some(DoctorCommand::Upgrade),
        } => {
            let dir = match config_dir {
                Some(dir) => dir,
                None => settings::config_dir()?,
            };
            let effective = settings::load_effective_from(&dir)?;
            emit(&fs3_cli::upgrade::upgrade(&effective.config).await)
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
            emit(&doctor::run(&effective.config, &dir).await)
        }
        Command::Docs { command } => match command {
            DocsCommand::List => emit(&fs3_cli::docs::list()),
            DocsCommand::Get { topic } => emit(&fs3_cli::docs::get(&topic)),
        },
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
            emit(&envelope)
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
///
/// # The seam
///
/// The envelope is serialised FIRST, the same way, in both modes — and the
/// human screen is rendered from those exact bytes, never from the typed value
/// beside them. Two consequences, both deliberate:
///
/// * the JSON path is a byte-for-byte passthrough of what it always printed
///   (`crates/cli/tests/envelope_goldens.rs` asserts it against goldens
///   captured before this layer existed), and
/// * the human view cannot drift ahead of the machine view, because it is made
///   out of it. A fact a person needs that the envelope does not carry is a gap
///   in the contract, not something to fetch on the side.
///
/// A renderer that declines, or bytes that will not round-trip, fall through to
/// the JSON — the reader sees the answer either way.
fn emit<T: serde::Serialize>(envelope: &Envelope<T>) -> Result<ExitCode> {
    // Whether a human screen was drawn, which decides what stderr still owes
    // the reader below.
    let mut rendered = false;

    match serde_json::to_string_pretty(envelope) {
        Ok(json) => {
            let screen = match output_mode() {
                OutputMode::Json => None,
                OutputMode::Human => serde_json::from_str::<Envelope<serde_json::Value>>(&json)
                    .ok()
                    .as_ref()
                    .and_then(render::render),
            };
            match screen {
                Some(text) => {
                    rendered = true;
                    // Through anstream, which owns the colour decision and
                    // strips at write time — and fallibly, because the reader
                    // may have walked away mid-screen exactly as they may
                    // mid-JSON.
                    write!(anstream::stdout().lock(), "{text}")?;
                }
                None => write_stdout(format_args!("{json}\n"))?,
            }
        }
        Err(error) => write_stderr(format_args!(
            "flowspace3: cannot render the response: {error}\n"
        ))?,
    }

    // The stderr copies exist for the JSON path: an agent gets the news in the
    // envelope, and a PERSON reading a terminal gets it without piping
    // anything. When a screen was drawn those two readers are the same reader,
    // and the screen already carries the message and the fix — so repeating
    // them below prints the diagnosis twice in the same terminal, which is how
    // a careful error surface becomes noise (found by u-r, 2026-08-28).
    if !rendered {
        for message in &envelope.messages {
            write_stderr(format_args!("flowspace3: {}\n", message.render()))?;
        }
    }

    Ok(match &envelope.error {
        None => ExitCode::SUCCESS,
        Some(failure) => {
            if !rendered {
                write_stderr(format_args!("{}\n", failure.render()))?;
            }
            ExitCode::from(EXIT_ERROR)
        }
    })
}

/// Write to stdout, and let a closed pipe be an error rather than a panic.
///
/// `println!` panics when the reader has gone away, which turns
/// `flowspace3 search … | head` — an ordinary thing a person does — into a
/// crash with a Rust backtrace. Main's commit 086f812 replaced every macro on
/// this path for that reason; this plan's human layer goes through the same
/// door, and `crates/cli/tests/epipe.rs` holds both of us to it.
fn write_stdout(arguments: fmt::Arguments<'_>) -> io::Result<()> {
    io::stdout().lock().write_fmt(arguments)
}

/// Write to stderr, fallibly, for the same reason as [`write_stdout`].
fn write_stderr(arguments: fmt::Arguments<'_>) -> io::Result<()> {
    io::stderr().lock().write_fmt(arguments)
}

/// Whether this failure is really "the reader hung up".
///
/// Anywhere in the chain: the error may arrive wrapped in context by the time
/// it reaches `main`, and a pipe that closed is a normal end to a command
/// rather than a failure of it.
fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
    })
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

/// Where the caller is standing, absolute, for workshop 003's D6 scoping.
///
/// The daemon has a working directory of its own and it is never the user's,
/// so a query that means "this repository" has to carry the directory with it.
/// Sent on every query verb; a directory that cannot be read (deleted out from
/// under the shell) simply omits it, and the query answers unscoped rather
/// than failing over a detail it can live without.
fn here() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    Some(
        std::fs::canonicalize(&cwd)
            .unwrap_or(cwd)
            .to_string_lossy()
            .to_string(),
    )
}

fn client_for(override_url: Option<String>) -> Result<DaemonClient> {
    let directory = settings::config_dir()?;
    let url = match override_url {
        Some(url) => url,
        None => settings::daemon_url_from(&directory)?,
    };
    DaemonClient::new(url, &directory)
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
