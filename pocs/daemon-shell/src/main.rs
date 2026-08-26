//! `daemon-shell` — run the prototype.
//!
//! ```text
//! cargo run -- --port 7474 --debounce-ms 10000 --watch .
//! ```

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use clap::Parser;
use daemon_shell::watcher::Added;

/// A host-native file watcher with a small local web service.
#[derive(Debug, Parser)]
#[command(name = "daemon-shell", version, about)]
struct Args {
    /// TCP port to serve on. 0 asks the OS for a free one and logs it.
    #[arg(long, default_value_t = 7474)]
    port: u16,

    /// Loopback address to bind. Anything non-loopback is refused at startup.
    #[arg(long, default_value = "127.0.0.1")]
    bind: IpAddr,

    /// Debounce window in milliseconds. 10000 is the fs3 default.
    #[arg(long, default_value_t = 10_000)]
    debounce_ms: u64,

    /// Directories to watch from startup. More can be added at runtime with
    /// `POST /watch`.
    #[arg(long = "watch", value_name = "DIR")]
    watch: Vec<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "daemon_shell=info".into()),
        )
        .init();

    let args = Args::parse();

    // Same rule as the real daemon (PRD req 17 / AC-0005): the surface is
    // unauthenticated, so a non-loopback bind is a startup failure rather than
    // a silent exposure of every indexed repo on the machine.
    if !is_loopback(args.bind) {
        bail!(
            "{} is not a loopback address; daemon-shell is local-only",
            args.bind
        );
    }
    let address = SocketAddr::new(args.bind, args.port);
    let debounce = Duration::from_millis(args.debounce_ms);
    let initial = args.watch.clone();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(daemon_shell::serve(
            address,
            debounce,
            move |bound, supervisor| {
                println!("daemon-shell listening on http://{bound}");
                for path in initial {
                    match supervisor.watch(&path) {
                        Ok(Added::Watching(root)) => println!("watching {}", root.display()),
                        Ok(Added::Rejected(conflict)) => {
                            eprintln!("refused {}: {conflict:?}", path.display());
                        }
                        Err(error) => eprintln!("cannot watch {}: {error:#}", path.display()),
                    }
                }
            },
        ))
}

/// `IpAddr::is_loopback` covers both families: `127.0.0.0/8` and `::1`. It is
/// the whole check — notably `0.0.0.0`, the address people type by habit, is
/// NOT loopback and is refused here rather than binding every interface.
fn is_loopback(address: IpAddr) -> bool {
    address.is_loopback()
}
