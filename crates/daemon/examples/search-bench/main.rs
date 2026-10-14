//! `search-bench`: the before/after gate for search speed.
//!
//! ```text
//! cargo run --release -p fs3-daemon --example search-bench -- generate \
//!     --server postgres://u:p@127.0.0.1:5435 --name bench_1x --scale 1
//! cargo run --release -p fs3-daemon --example search-bench -- run \
//!     --db postgres://u:p@127.0.0.1:5435/bench_1x --arm baseline --out base.json
//! cargo run --release -p fs3-daemon --example search-bench -- e2e \
//!     --queries crates/daemon/examples/search-bench/queries/code-questions.txt --cwd .
//! ```
//!
//! An example of the daemon crate rather than a crate of its own: it drives
//! the daemon's real search path in-process, and its database access (COPY for
//! the synthetic load) uses the daemon's dev-only `sqlx`, which must not ship.
//! See `docs/services/search-latency.md`.

mod generate;
mod model;
mod report;
mod run;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

const USAGE: &str = "\
usage: search-bench <command> [--flag value]...

  generate --server URL --name DB [--scale 1.0] [--seed 7] [--replace] [--skip-indexes]
      create DB holding a synthetic corpus shaped like prod (scale 1 ≈ 1.6M vectors)
  index --db URL
      build the vector and trigram indexes a --skip-indexes load left out
  run --db URL [--queries 30] [--rounds 1] [--warm 1] [--seed 1] [--repos 0,1,12]
      [--unscoped] [--source code] [--limit 10] [--arm NAME[:k=v,k=v]]... [--out FILE]
      time searches through the daemon's own search path, cold and warm
  e2e --queries FILE [--cwd DIR] [--binary flowspace3] [--warm 1] [--label e2e]
      [--out FILE] [-- extra search args]
      time the installed `flowspace3 search` against its configured daemon";

/// `--flag value` pairs (repeatable), bare `--switch`es, and what follows `--`.
struct Args {
    values: BTreeMap<String, Vec<String>>,
    switches: Vec<String>,
    rest: Vec<String>,
}

const SWITCHES: [&str; 3] = ["replace", "skip-indexes", "unscoped"];

impl Args {
    fn parse(raw: &[String]) -> Result<Self> {
        let mut args = Args {
            values: BTreeMap::new(),
            switches: Vec::new(),
            rest: Vec::new(),
        };
        let mut iter = raw.iter();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                args.rest = iter.cloned().collect();
                break;
            }
            let Some(flag) = arg.strip_prefix("--") else {
                bail!("unexpected argument {arg:?}\n\n{USAGE}");
            };
            if SWITCHES.contains(&flag) {
                args.switches.push(flag.to_string());
                continue;
            }
            let value = iter
                .next()
                .with_context(|| format!("--{flag} needs a value\n\n{USAGE}"))?;
            args.values
                .entry(flag.to_string())
                .or_default()
                .push(value.clone());
        }
        Ok(args)
    }

    fn get(&self, flag: &str) -> Option<&str> {
        self.values
            .get(flag)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    fn required(&self, flag: &str) -> Result<String> {
        self.get(flag)
            .map(ToString::to_string)
            .with_context(|| format!("--{flag} is required\n\n{USAGE}"))
    }

    fn number<T: std::str::FromStr>(&self, flag: &str, default: T) -> Result<T>
    where
        T::Err: std::fmt::Display,
    {
        match self.get(flag) {
            None => Ok(default),
            Some(value) => value
                .parse()
                .map_err(|error| anyhow::anyhow!("--{flag} {value:?}: {error}")),
        }
    }

    fn switch(&self, flag: &str) -> bool {
        self.switches.iter().any(|s| s == flag)
    }

    fn all(&self, flag: &str) -> Vec<String> {
        self.values.get(flag).cloned().unwrap_or_default()
    }
}

fn write_out(
    out: Option<&str>,
    command: &str,
    samples: &[run::Sample],
    summaries: &[report::GroupSummary],
) -> Result<()> {
    println!("{}", report::table(summaries));
    if let Some(path) = out {
        let path = PathBuf::from(path);
        let document = serde_json::json!({
            "command": command,
            "summary": summaries,
            "samples": samples,
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&document)?)
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = raw.split_first() else {
        bail!("{USAGE}");
    };
    let args = Args::parse(rest)?;
    match command.as_str() {
        "generate" => {
            generate::generate(&generate::Request {
                server: args.required("server")?,
                name: args.required("name")?,
                scale: args.number("scale", 1.0)?,
                seed: args.number("seed", 7)?,
                replace: args.switch("replace"),
                skip_indexes: args.switch("skip-indexes"),
            })
            .await
        }
        "index" => generate::build_indexes(&args.required("db")?).await,
        "run" => {
            let arms = match args.all("arm") {
                arms if arms.is_empty() => vec!["baseline".parse()?],
                arms => arms
                    .iter()
                    .map(|arm| arm.parse())
                    .collect::<Result<Vec<run::Arm>>>()?,
            };
            let repos = args
                .get("repos")
                .unwrap_or("0,1,12")
                .split(',')
                .map(|index| index.trim().parse::<usize>())
                .collect::<Result<Vec<_>, _>>()
                .context("--repos takes comma-separated indexes")?;
            let samples = run::run(&run::Request {
                db: args.required("db")?,
                queries: args.number("queries", 30)?,
                rounds: args.number("rounds", 1)?,
                warm: args.number("warm", 1)?,
                seed: args.number("seed", 1)?,
                repos,
                unscoped: args.switch("unscoped"),
                source: args.get("source").map(ToString::to_string),
                limit: args.number("limit", 10)?,
                arms,
            })
            .await?;
            write_out(
                args.get("out"),
                "run",
                &samples,
                &report::summarize(&samples),
            )
        }
        "e2e" => {
            let path = args.required("queries")?;
            let text = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
            let queries = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(ToString::to_string)
                .collect();
            let samples = run::e2e(&run::E2eRequest {
                binary: args.get("binary").unwrap_or("flowspace3").to_string(),
                queries,
                cwd: args.get("cwd").map(ToString::to_string),
                args: args.rest.clone(),
                warm: args.number("warm", 1)?,
                label: args.get("label").unwrap_or("e2e").to_string(),
            })
            .await?;
            write_out(
                args.get("out"),
                "e2e",
                &samples,
                &report::summarize(&samples),
            )
        }
        other => bail!("unknown command {other:?}\n\n{USAGE}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Args {
        let raw: Vec<String> = line.split_whitespace().map(ToString::to_string).collect();
        Args::parse(&raw).unwrap()
    }

    #[test]
    fn flags_switches_repeats_and_the_rest() {
        let args = parse("--db x --arm a --arm b:k=v --unscoped --warm 2 -- --source code");
        assert_eq!(args.get("db"), Some("x"));
        assert_eq!(args.all("arm"), ["a", "b:k=v"]);
        assert!(args.switch("unscoped"));
        assert_eq!(args.number("warm", 1).unwrap(), 2);
        assert_eq!(args.number("rounds", 1).unwrap(), 1);
        assert_eq!(args.rest, ["--source", "code"]);
    }

    #[test]
    fn a_flag_without_its_value_is_refused() {
        let raw = vec!["--db".to_string()];
        assert!(Args::parse(&raw).is_err());
    }
}
