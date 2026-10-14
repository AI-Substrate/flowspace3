//! `search-bench run` and `e2e`: time searches, cold and warm.
//!
//! `run` drives the daemon's own search path in-process against a bench
//! database, with an embedder that turns each query text into a fresh vector in
//! the corpus's space. Every query text is new, so its first search is cold;
//! repeating it at once is warm. Each arm is the same search with different
//! connection settings, and arms take turns going first (ABBA), so neither
//! always pays for a cold cache (the A5 lesson, 2026-10-13).
//!
//! `e2e` runs the installed `flowspace3 search --json` against whatever daemon
//! it is configured for, reads `meta.timing`, and is read-only.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use fs3_core::views::search::SearchTiming;
use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::scope::{Scope, ScopeSource};
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;
use serde::Serialize;

use crate::generate::{GeneratedRepo, MODEL_KEY};
use crate::model::{Rng, Space, Words};

/// One timed search.
#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub arm: String,
    pub round: usize,
    pub query: usize,
    pub kind: &'static str,
    pub phase: &'static str,
    pub text: String,
    pub scope: Option<String>,
    pub wall_ms: u64,
    pub hits: usize,
    pub passes: usize,
    pub timing: Option<SearchTiming>,
    /// Shared-buffer hits and reads every statement made during this search
    /// (from `pg_stat_statements`, when the database has it).
    pub blocks_hit: Option<i64>,
    pub blocks_read: Option<i64>,
    pub error: Option<String>,
}

/// An embedder that places every query text at a fresh point of the bench
/// corpus's vector space. The same text always gets the same vector.
#[derive(Clone)]
struct BenchEmbedder {
    space: Arc<Space>,
}

impl std::fmt::Debug for BenchEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BenchEmbedder")
    }
}

#[async_trait]
impl fs3_core::ports::Embedder for BenchEmbedder {
    async fn embed(&self, texts: &[String]) -> fs3_core::Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| self.space.query(fnv(text)))
            .collect())
    }

    fn key(&self) -> String {
        MODEL_KEY.to_string()
    }

    fn concurrency_ceiling(&self) -> usize {
        8
    }

    fn max_input_tokens(&self) -> usize {
        8192
    }
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// One arm: a name and the session settings its connections start with.
#[derive(Clone, Debug)]
pub struct Arm {
    pub name: String,
    pub settings: Vec<String>,
}

impl std::str::FromStr for Arm {
    type Err = anyhow::Error;

    /// `name` or `name:key=value,key=value`.
    fn from_str(spec: &str) -> Result<Self> {
        let (name, settings) = spec.split_once(':').unwrap_or((spec, ""));
        if name.is_empty() {
            bail!("an arm needs a name: {spec:?}");
        }
        Ok(Arm {
            name: name.to_string(),
            settings: settings
                .split(',')
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect(),
        })
    }
}

/// What `run` was asked for.
pub struct Request {
    pub db: String,
    pub queries: usize,
    pub rounds: usize,
    pub warm: usize,
    pub seed: u64,
    pub repos: Vec<usize>,
    pub unscoped: bool,
    pub source: Option<String>,
    pub limit: i64,
    pub arms: Vec<Arm>,
}

/// A query: its text and what shape it is.
fn query(words: &Words, rng: &mut Rng, index: usize) -> (&'static str, String) {
    let topic = rng.below(crate::model::TOPICS);
    match index % 3 {
        // A meaning-shaped question: topical and mid-frequency words.
        0 => {
            let n = 4 + rng.below(3);
            let text = (0..n)
                .map(|i| {
                    if i % 2 == 0 {
                        words.topical(topic, rng).to_string()
                    } else {
                        words.at(200 + rng.below(2_800)).to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            ("question", text)
        }
        // Common words: the keyword leg's worst case.
        1 => {
            let text = (0..3)
                .map(|_| words.at(10 + rng.below(140)).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            ("common", text)
        }
        // One identifier.
        _ => ("identifier", words.identifier(topic, rng)),
    }
}

fn with_settings(db: &str, settings: &[String]) -> String {
    if settings.is_empty() {
        return db.to_string();
    }
    let options: String = settings
        .iter()
        .map(|setting| format!("-c {setting}"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace(' ', "%20")
        .replace('=', "%3D");
    let joiner = if db.contains('?') { '&' } else { '?' };
    format!("{db}{joiner}options={options}")
}

async fn buffers(pool: &sqlx::PgPool) -> Option<(i64, i64)> {
    sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
        "SELECT sum(shared_blks_hit)::bigint, sum(shared_blks_read)::bigint \
           FROM pg_stat_statements",
    )
    .fetch_one(pool)
    .await
    .ok()
    .and_then(|(hit, read)| Some((hit?, read?)))
}

pub async fn run(request: &Request) -> Result<Vec<Sample>> {
    let probe = fs3_store::connect(&request.db).await?;
    let corpus: serde_json::Value =
        sqlx::query_scalar("SELECT value FROM bench_meta WHERE key = 'corpus'")
            .fetch_one(&probe)
            .await
            .context("this database has no bench corpus; run `generate` first")?;
    let seed = corpus["seed"].as_u64().context("corpus seed")?;
    let clusters = corpus["clusters"].as_u64().context("corpus clusters")? as usize;
    let repos: Vec<GeneratedRepo> = serde_json::from_value(corpus["repos"].clone())?;
    // The same space the corpus was generated in, so queries land among it.
    let space = Arc::new(Space::new(seed, clusters));
    let words = Words::new(seed, 30_000);

    let scopes: Vec<Scope> = if request.unscoped {
        vec![Scope::unscoped()]
    } else {
        request
            .repos
            .iter()
            .map(|index| {
                let repo = repos
                    .get(*index)
                    .with_context(|| format!("the corpus has {} repositories", repos.len()))?;
                let root = repo.worktrees.first().cloned();
                Ok(Scope {
                    repo: Some(repo.identity.clone()),
                    source: ScopeSource::Cwd,
                    cwd: root.clone(),
                    worktree: root,
                    warnings: Vec::new(),
                })
            })
            .collect::<Result<_>>()?
    };

    let mut states = Vec::new();
    for arm in &request.arms {
        let config = Config {
            database: DatabaseConfig {
                url: with_settings(&request.db, &arm.settings),
            },
            ..Config::default()
        };
        let mut state = AppState::from_config(config)?;
        state.embedder = Arc::new(BenchEmbedder {
            space: Arc::clone(&space),
        });
        states.push((arm.clone(), state));
    }

    let mut samples = Vec::new();
    let mut rng = Rng::derive(request.seed, 77);
    for round in 0..request.rounds {
        for index in 0..request.queries {
            // ABBA: even queries run the arms forwards, odd ones backwards.
            let order: Vec<usize> = if index % 2 == 0 {
                (0..states.len()).collect()
            } else {
                (0..states.len()).rev().collect()
            };
            for arm_index in order {
                let (arm, state) = &states[arm_index];
                // Each arm gets its own fresh query, so one arm's cold run
                // never warms the next arm's.
                let (kind, text) = query(&words, &mut rng, index);
                let scope = &scopes[index % scopes.len()];
                for repeat in 0..=request.warm {
                    let search_request = SearchRequest {
                        q: text.clone(),
                        limit: Some(request.limit),
                        source: request.source.clone(),
                        ..SearchRequest::default()
                    };
                    let before = buffers(&probe).await;
                    let started = Instant::now();
                    let outcome = search(state, &search_request, scope).await;
                    let wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    let after = buffers(&probe).await;
                    let (blocks_hit, blocks_read) = match (before, after) {
                        (Some(b), Some(a)) => (Some(a.0 - b.0), Some(a.1 - b.1)),
                        _ => (None, None),
                    };
                    let mut sample = Sample {
                        arm: arm.name.clone(),
                        round,
                        query: index,
                        kind,
                        phase: if repeat == 0 { "cold" } else { "warm" },
                        text: text.clone(),
                        scope: scope.repo.clone(),
                        wall_ms,
                        hits: 0,
                        passes: 0,
                        timing: None,
                        blocks_hit,
                        blocks_read,
                        error: None,
                    };
                    match outcome {
                        Ok(outcome) => {
                            let mut timing = outcome.timing;
                            timing.total_ms = wall_ms;
                            sample.hits = outcome.results.len();
                            sample.passes = outcome.passes;
                            sample.timing = Some(timing);
                        }
                        Err(failure) => sample.error = Some(failure.message.clone()),
                    }
                    eprintln!(
                        "{:>9} r{round} q{index:<3} {:<10} {:<4} {:>6} ms  hits {:>3}  read {:>7}{}",
                        sample.arm,
                        sample.kind,
                        sample.phase,
                        sample.wall_ms,
                        sample.hits,
                        sample
                            .blocks_read
                            .map_or("-".to_string(), |r| r.to_string()),
                        sample
                            .error
                            .as_deref()
                            .map_or(String::new(), |e| format!("  ERROR {e}")),
                    );
                    samples.push(sample);
                }
            }
        }
    }
    Ok(samples)
}

/// What `e2e` was asked for.
pub struct E2eRequest {
    pub binary: String,
    pub queries: Vec<String>,
    pub cwd: Option<String>,
    pub args: Vec<String>,
    pub warm: usize,
    pub label: String,
}

/// Run the installed CLI once per query (and `warm` repeats), reading the
/// daemon's own split from `meta.timing` when it reports one.
pub async fn e2e(request: &E2eRequest) -> Result<Vec<Sample>> {
    let mut samples = Vec::new();
    for (index, text) in request.queries.iter().enumerate() {
        for repeat in 0..=request.warm {
            let mut command = tokio::process::Command::new(&request.binary);
            command
                .arg("search")
                .arg(text)
                .arg("--json")
                .args(&request.args)
                .env("FS3_OUTPUT", "json");
            if let Some(cwd) = &request.cwd {
                command.current_dir(cwd);
            }
            let started = Instant::now();
            let output = command.output().await.context("running flowspace3")?;
            let wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let envelope: Option<serde_json::Value> = stdout
                .find('{')
                .and_then(|start| serde_json::from_str(&stdout[start..]).ok());
            let mut sample = Sample {
                arm: request.label.clone(),
                round: 0,
                query: index,
                kind: "e2e",
                phase: if repeat == 0 { "cold" } else { "warm" },
                text: text.clone(),
                scope: None,
                wall_ms,
                hits: 0,
                passes: 0,
                timing: None,
                blocks_hit: None,
                blocks_read: None,
                error: None,
            };
            match &envelope {
                Some(envelope) if envelope["ok"] == true => {
                    sample.hits = envelope["data"]["results"].as_array().map_or(0, Vec::len);
                    sample.passes = envelope["meta"]["passes"].as_u64().unwrap_or(0) as usize;
                    sample.scope = envelope["data"]["scope"]["repo"]
                        .as_str()
                        .map(ToString::to_string);
                    sample.timing = serde_json::from_value(envelope["meta"]["timing"].clone()).ok();
                }
                Some(envelope) => {
                    sample.error = Some(envelope["error"]["message"].to_string());
                }
                None => sample.error = Some(format!("no envelope (exit {:?})", output.status)),
            }
            eprintln!(
                "{:>9} q{index:<3} {:<4} {:>6} ms  hits {:>3}  daemon total {}",
                sample.arm,
                sample.phase,
                sample.wall_ms,
                sample.hits,
                sample
                    .timing
                    .as_ref()
                    .map_or("-".to_string(), |t| format!("{} ms", t.total_ms)),
            );
            samples.push(sample);
        }
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arm_parses_its_settings() {
        let arm: Arm = "ef80:hnsw.ef_search=80,jit=off".parse().unwrap();
        assert_eq!(arm.name, "ef80");
        assert_eq!(arm.settings, ["hnsw.ef_search=80", "jit=off"]);
        let bare: Arm = "baseline".parse().unwrap();
        assert!(bare.settings.is_empty());
    }

    #[test]
    fn settings_ride_in_the_connection_options() {
        assert_eq!(
            with_settings(
                "postgres://u:p@h:5435/db",
                &["hnsw.ef_search=80".to_string()]
            ),
            "postgres://u:p@h:5435/db?options=-c%20hnsw.ef_search%3D80"
        );
        assert_eq!(with_settings("postgres://h/db", &[]), "postgres://h/db");
    }
}
