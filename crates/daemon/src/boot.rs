//! Booting the daemon: config, the composition root, the runner, HTTP.
//!
//! This used to be `fs3-daemon`'s `main`. The daemon now ships INSIDE the
//! `flowspace3` binary as `flowspace3 daemon` (PRD req 51, Jordan 2026-08-26):
//! one file to install, one version, and no way for a CLI and a daemon of
//! different vintages to meet. The crate is unchanged in every other respect —
//! it is still the composition root, and still the only crate that sees every
//! other one.
//!
//! What did NOT move here is the secrets chain. Putting `secrets.env` into the
//! process environment is only sound while the process is single-threaded, so
//! it has to happen before a runtime exists — and the CLI already does exactly
//! that, first thing, for every verb. Doing it again here would be a second
//! implementation of a rule that is easy to get subtly wrong, so [`run`]
//! assumes the environment is already loaded and says so.

use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use fs3_core::{Config, Port, redact_url_password};

use crate::logging::Logging;
use crate::wiring::AppState;
use crate::{config, http, logging};

/// How often the reconcile runner compares desired state against actual.
///
/// Five seconds, and deliberately not configurable yet. The doctrine's own
/// note on the first implementor says why a few seconds is invisible: `add`
/// enqueues its initial scan directly, so nothing waits on this pass to index
/// a newly added root — the pass only has to get the WATCHER installed before
/// the next edit, and a human cannot edit a file they have not finished adding.
/// The `Notify` nudge handle in the doctrine lands only if that ever stops
/// being true.
const RECONCILE_EVERY_SECONDS: u64 = 5;

/// How much missing-vector backlog one boot re-queues.
///
/// A ceiling rather than "everything", because the sweep derives its backlog
/// from the whole content layer: an index that has been running without this
/// fix could answer with tens of thousands of rows, and turning a daemon start
/// into a queue flood is its own outage. Two thousand is a few minutes of
/// batched embedding work, and the sweep runs at EVERY boot — so a large
/// backlog heals across restarts instead of in one alarming burst.
const MISSING_VECTOR_SWEEP: i64 = 2_000;

/// How much missing-summary backlog one boot re-queues.
///
/// Smaller than the vector ceiling, because these are LLM calls rather than
/// batched embeddings: one job is one chat request, and a boot that queued
/// thousands would turn a restart into a bill. Five hundred is a slow, honest
/// trickle, and the sweep runs at every boot.
const MISSING_SUMMARY_SWEEP: i64 = 500;

/// Run the daemon until it is asked to stop.
///
/// Must be called from OUTSIDE a Tokio runtime: it builds its own, because the
/// caller's `main` is deliberately not `#[tokio::main]`.
///
/// # Errors
/// A configuration that cannot be read, a `daemon.url` that is not loopback, a
/// store that cannot be migrated, or an address that cannot be bound — all
/// startup failures on purpose (PRD req 37).
pub fn run() -> Result<()> {
    let directory = config::config_dir().context("locating the fs3 config directory")?;

    let configuration = config::load_effective_from(&directory)
        .with_context(|| format!("loading configuration from {}", directory.display()))?;

    refuse_a_defaulted_store_under_test(&configuration)?;

    // FIRST use of the configuration, before anything is logged: the log file's
    // path, its caps and its filter are all configuration, so a subscriber
    // installed any earlier could honour none of them. Everything above this
    // line therefore reports through `Result`, not through `tracing`.
    let logging = logging::init(&configuration.config.daemon);

    let address = bind_address(&configuration.config.daemon.url)?;
    // Stage without publishing. `serve` binds first and atomically publishes
    // before the accept loop starts, so a port-race loser cannot rotate the
    // winner's credential and no request sees an unpublished key.
    let auth = crate::auth::stage(&directory)?;
    tracing::info!(
        config = %directory.display(),
        // Named once, at startup, so "where are the logs" is answerable from
        // the logs themselves — and from the scrollback, when the file could
        // not be opened at all.
        log = %logging
            .file
            .as_ref()
            .map_or_else(|| "stdout only".to_string(), |path| path.display().to_string()),
        daemon = %configuration.layer("daemon"),
        database = %configuration.layer("database"),
        repos = configuration.config.repos.len(),
        "fs3 daemon starting"
    );

    // Prevention, beside the detection in the ask handler. A daemon can boot
    // healthy with an agent port that cannot answer a single question, and
    // nothing about its startup said so — which is how a production daemon
    // served placeholder answers without anyone noticing. Refusing to boot
    // would be wrong: search, get, tree and the whole indexing pipeline work
    // perfectly without a chat model, and the sandbox posture runs fakes on
    // purpose. So it is loud rather than fatal, and it names the fix.
    if matches!(
        configuration
            .config
            .provider(configuration.config.selected(Port::Agent, None)),
        Ok(fs3_core::ProviderInstance::Fake)
    ) {
        tracing::warn!(
            agent = %configuration.config.selected(Port::Agent, None),
            "the agent port is the offline fake: `flowspace3 ask` will REFUSE every question \
             with FS3-E-PROVIDER-CANNOT-ANSWER. Every other verb is unaffected. Point \
             `[agent] active` at a real chat deployment to enable it"
        );
    }

    if let Some(problem) = &logging.problem {
        tracing::warn!(
            directory = %logging.directory,
            %problem,
            "no log file: this process is logging to stdout only, so nothing survives it"
        );
    }

    // The notice the config loader can no longer give itself: it runs before
    // the subscriber exists, so the fact travels as data and is said here.
    if !configuration.has_file {
        tracing::info!(
            path = %directory.join(fs3_core::CONFIG_FILE_NAME).display(),
            "no config file: running on defaults. Create that file to change anything."
        );
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the Tokio runtime")?
        .block_on(serve(
            configuration.config,
            address,
            logging,
            auth,
            None,
            None,
            None,
        ))
}

/// Recover enrichment jobs the queue's own memory has written off, in the one
/// order that works: retire the unrunnable, THEN revive the rest.
///
/// Order is the whole contract of this function. [`fs3_store::requeue_failed`]
/// wakes every failed `summarize`/`embed` job that is not `terminal`, so a
/// retirement that ran afterwards would hand the empty-input jobs a fresh life
/// on every boot and merely close the door behind them. Retiring first is what
/// makes the poison stay dead.
///
/// The rows each half catches are both measured, not hypothetical. An
/// empty-bodied conversation turn mints an embed job whose text is the empty
/// string, and the provider rejects the WHOLE merged call it rides in: one such
/// turn rejected 530 innocent texts on 2026-08-30, and three of them cost nine
/// calls and 1,639 texts. Separately, fifty-nine elements of a live index sat
/// failed three-attempts-at-a-time against the model's per-input token cap
/// until the binary learned to split them. `enrich` now refuses to mint empty
/// inputs and `batch::plan` drops any handed to it, so no new poison appears —
/// this is how each fix reaches the rows its bug already claimed, with no
/// repair command to discover and no SQL to write.
///
/// Cheap by construction: a requeued job whose vectors already exist settles on
/// its own pre-check without a provider call.
///
/// Public so the boot recovery sequence can be proven by test rather than
/// assumed — a heal nothing invokes is a heal that does not exist.
pub async fn recover_enrichment_jobs(db: &fs3_store::PgPool) {
    match fs3_store::retire_empty_embed_jobs(db).await {
        Ok(0) => {}
        Ok(retired) => tracing::info!(
            retired,
            "terminally retired failed embed jobs containing only empty input"
        ),
        Err(error) => tracing::error!(%error, "cannot retire empty failed embed jobs"),
    }

    match fs3_store::requeue_failed(db, &[crate::enrich::SUMMARIZE, crate::enrich::EMBED]).await {
        Ok(0) => {}
        Ok(swept) => tracing::info!(
            swept,
            "requeued enrichment jobs that had run out of attempts; a fix in this binary may \
             cover them"
        ),
        Err(error) => tracing::error!(%error, "cannot requeue failed enrichment jobs"),
    }
}

/// Run an isolated daemon until it is asked to stop.
///
/// Only the ambient database location is reused, narrowly, to choose the
/// Postgres server and credentials. The configuration wired into the daemon is
/// loaded from the process-owned sandbox directory with ambient overrides
/// disabled, then every stateful or spend-bearing seam is forced locally.
pub fn run_sandbox() -> Result<()> {
    let ambient_directory = config::config_dir().context("locating the fs3 config directory")?;

    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").context("reserving a sandbox daemon port")?;
    listener
        .set_nonblocking(true)
        .context("making the sandbox listener nonblocking")?;
    let port = listener
        .local_addr()
        .context("reading the sandbox daemon port")?
        .port();

    let sandbox_directory = tempfile::Builder::new()
        .prefix("flowspace3-sandbox-")
        .tempdir()
        .context("creating the sandbox runtime directory")?;
    let (mut configuration, base_database_url) =
        sandbox_configuration(&ambient_directory, sandbox_directory.path(), port)?;

    let logging = logging::init(&configuration.daemon);
    let auth = crate::auth::stage(sandbox_directory.path())?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the Tokio runtime")?;

    let (database, outcome) = runtime.block_on(async move {
        let shutdown = shutdown_context()?;
        let listener = tokio::net::TcpListener::from_std(listener)
            .context("adopting the reserved sandbox daemon port")?;
        let database = fs3_testkit::FreshDatabase::create_from(&base_database_url, "sandbox")
            .await
            .context("creating the sandbox database")?;
        configuration.database.url = database.url();
        let ready = SandboxReady {
            database: database.name().to_string(),
            config: sandbox_directory.path().to_path_buf(),
        };
        let outcome = serve(
            configuration,
            format!("127.0.0.1:{port}"),
            logging,
            auth,
            Some(listener),
            Some(ready),
            Some(shutdown),
        )
        .await;
        Ok::<_, anyhow::Error>((database, outcome))
    })?;

    // Stop every worker before dropping its database. Otherwise a worker may
    // wake between DROP DATABASE and runtime teardown and emit a false failure.
    runtime.shutdown_timeout(Duration::from_secs(1));
    let database_name = database.name().to_string();
    let cleanup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the sandbox cleanup runtime")?
        .block_on(database.cleanup());
    match cleanup {
        Ok(()) => tracing::info!(database = %database_name, "sandbox database dropped"),
        Err(error) => {
            tracing::error!(
                database = %database_name,
                %error,
                "sandbox database left behind; remove it with: docker exec flowspace3-db \
                 psql -U flowspace3 -d postgres -c 'DROP DATABASE IF EXISTS {database_name} WITH (FORCE)'"
            );
            return Err(error).with_context(|| {
                format!("sandbox database {database_name} was left behind after shutdown")
            });
        }
    }
    outcome
}

struct SandboxReady {
    database: String,
    config: std::path::PathBuf,
}

struct ShutdownContext {
    receiver: tokio::sync::watch::Receiver<crate::runner::Shutdown>,
    task: tokio::task::JoinHandle<()>,
}

fn shutdown_context() -> Result<ShutdownContext> {
    let (sender, receiver) = tokio::sync::watch::channel(crate::runner::Shutdown::Running);
    let task = install_shutdown_handler(sender)?;
    Ok(ShutdownContext { receiver, task })
}

fn force_sandbox_config(config: &mut Config, port: u16, directory: &std::path::Path) {
    let defaults = Config::default();
    config.daemon.url = format!("http://127.0.0.1:{port}");
    config.daemon.log_dir = directory.join("logs").to_string_lossy().into_owned();
    config.providers = defaults.providers;
    config.embedder = defaults.embedder;
    config.summarizer = defaults.summarizer;
    config.agent = defaults.agent;
    config.repos.clear();
    config.update.auto = false;
}

fn sandbox_configuration(
    ambient_directory: &std::path::Path,
    sandbox_directory: &std::path::Path,
    port: u16,
) -> Result<(Config, String)> {
    // The database URL is the sole ambient value retained: it selects the
    // server and credentials used to create a disposable child. The resulting
    // `Config` is not derived from this object.
    let base_database_url = config::load_effective_from(ambient_directory)
        .with_context(|| {
            format!(
                "loading database location from {}",
                ambient_directory.display()
            )
        })?
        .config
        .database
        .url;
    let mut configuration = config::load_isolated_from(sandbox_directory).with_context(|| {
        format!(
            "loading isolated configuration from {}",
            sandbox_directory.display()
        )
    })?;
    force_sandbox_config(&mut configuration, port, sandbox_directory);
    Ok((configuration, base_database_url))
}

/// Refuse to boot when a TEST spawned us and nobody said which store to use.
///
/// # The hole this stands in
///
/// `fs3_testkit::spawn::sealed` is how a test hands this binary an
/// environment, and `crates/testkit/tests/spawn_isolation.rs` makes it the only
/// way — but that is a scan of source text, and it can only refuse the shapes
/// it knows. This is the same rule enforced from inside, where the shape does
/// not matter: whatever spawned us, if a test marker is present and nothing
/// chose a database, we do not touch one. Boot MIGRATES (see `serve`), so
/// "touch one" means "write to it".
///
/// # Why provenance and not the URL
///
/// The obvious rule — refuse [`fs3_core::DatabaseConfig::DEFAULT_URL`] — is
/// wrong for exactly the reason `fs3_testkit::database` gives: CI legitimately
/// sets that same string, where it names a disposable service container. The
/// same characters mean "throwaway" there and "production" on Jordan's laptop,
/// so the URL cannot be the discriminator.
///
/// [`fs3_core::Layer`] can. `Layer::Defaults` means no config file and no
/// environment override named a database — nobody DECIDED, which is the actual
/// defect. A CI daemon pinned to the service container reads `Layer::Env` and
/// boots normally.
///
/// # Cost to real users
///
/// None. `FS3_TEST_DATABASE_URL` is a test-run marker; production never sets
/// it, so this returns `Ok` before it looks at anything else. `DEFAULT_URL`
/// keeps meaning exactly what it means today for everyone who is not a test.
fn refuse_a_defaulted_store_under_test(configuration: &fs3_core::Effective) -> Result<()> {
    let under_test = std::env::var_os(fs3_testkit::TEST_DATABASE_ENV)
        .is_some_and(|value| !value.is_empty() && value != "0");
    if !under_test || configuration.layer("database") != fs3_core::Layer::Defaults {
        return Ok(());
    }

    bail!(
        "refusing to boot: {} is set — so a TEST spawned this daemon — and no \
         config file or environment override chose a database, so the store \
         would be {}.\n\n\
         That address is the SHIPPED default. On a developer machine it is the \
         real store, and boot MIGRATES before it serves: this is exactly how \
         migration 0012 reached Jordan's production database on 2026-08-27 and \
         took the installed CLI down on schema skew.\n\n\
         Spawn through `fs3_testkit::sealed(binary, config_dir, TestDatabase::…)`, \
         which scrubs every inherited FS3_* and pins both the config directory \
         and the database.",
        fs3_testkit::TEST_DATABASE_ENV,
        redact_url_password(&configuration.config.database.url),
    )
}

async fn serve(
    configuration: Config,
    address: String,
    logging: Logging,
    auth: crate::auth::StagedAuth,
    listener: Option<tokio::net::TcpListener>,
    sandbox: Option<SandboxReady>,
    shutdown: Option<ShutdownContext>,
) -> Result<()> {
    let ShutdownContext {
        receiver: shutdown,
        task: signal_task,
    } = match shutdown {
        Some(shutdown) => shutdown,
        None => shutdown_context()?,
    };

    let state = AppState::from_config(configuration).context("wiring the composition root")?;
    tracing::info!(
        embedder = %state.config.selected(Port::Embedder, None),
        embedder_kind = %state.active_kind(Port::Embedder),
        summarizer = %state.config.selected(Port::Summarizer, None),
        summarizer_kind = %state.active_kind(Port::Summarizer),
        agent = %state.config.selected(Port::Agent, None),
        agent_kind = %state.active_kind(Port::Agent),
        "providers wired"
    );
    let database = redact_url_password(&state.config.database.url);

    // BEFORE migrating, ask which direction the disagreement runs. Behind is
    // the ordinary case and migrating fixes it; AHEAD is not fixable at all by
    // this process, and sqlx's refusal for it — "migration N was previously
    // applied but is missing in the resolved migrations" — is its sentence
    // rather than ours, wrapped below in a `docker compose up -d` steer that is
    // actively wrong. The store is healthy; the binary is stale (req-0061).
    match fs3_store::schema_current(&state.db).await {
        Ok(status) => {
            let skew = status.skew(env!("CARGO_PKG_VERSION"));
            if skew.is_skewed() {
                bail!("{}", skew.explain());
            }
        }
        // Not fatal on its own: the read can fail for the same reasons the
        // migration below is about to, and that path already reports them with
        // the connection advice this one must not give.
        Err(error) => tracing::debug!(%error, "could not compare schema versions before migrating"),
    }

    // The daemon is the single writer, so startup is the only migration point.
    // It is also the only moment where refusing to run is cheaper than running:
    // a writer that cannot reach its own schema has nothing useful to serve, so
    // this fails loud rather than starting into a guaranteed error per request.
    fs3_store::migrate(&state.db).await.with_context(|| {
        format!(
            "applying store migrations to {database} — if the store is not running: {}",
            fs3_store::COMPOSE_UP
        )
    })?;
    tracing::info!(%database, "store schema is current");

    // Recover anything a previous process died holding, BEFORE the runner can
    // claim. A row left `running` has no lease and no heartbeat, so nothing
    // else would ever move it — and because `scan_file` dedupes on
    // (worktree, path), it would silently absorb every future add or scan of
    // that file. One SIGKILL during a large index would otherwise make those
    // files permanently unindexable, reported as success.
    //
    // Sound only here: fs3 is the single writer (PRD req 20), so at this
    // instant no worker exists to be holding a claim.
    match fs3_store::requeue_running(&state.db).await {
        Ok(0) => {}
        Ok(swept) => tracing::warn!(
            swept,
            "requeued jobs left running by a previous process — it did not shut down cleanly"
        ),
        Err(error) => tracing::error!(%error, "cannot requeue jobs left running"),
    }

    // Probe every registered worktree's ddocs tooling BEFORE serving, because
    // the snapshot map starts empty and is otherwise only filled by add_root
    // or rescan_root. Without this, a daemon restarted against an already
    // indexed corpus reports "the ddocs binary is unavailable" on every search
    // until someone happens to run a scan — a false explanation, and exactly
    // the confident-wrong-answer this feature exists to remove.
    //
    // Best-effort: a worktree that cannot be probed is left unprobed rather
    // than recorded as absent, so a missing entry never becomes a claim about
    // the binary. Failure here must not stop the daemon serving.
    match fs3_store::list_worktrees(&state.db).await {
        Ok(worktrees) => {
            for worktree in &worktrees {
                let tooling = crate::ddoc::probe(std::path::Path::new(&worktree.root_path)).await;
                state.set_ddoc_tooling(worktree.id, tooling).await;
            }
            tracing::info!(
                roots = worktrees.len(),
                "probed ddocs tooling for registered roots"
            );
        }
        Err(error) => {
            tracing::warn!(%error, "cannot list worktrees to probe ddocs tooling; searches will not claim binary absence for unprobed roots");
        }
    }

    recover_enrichment_jobs(&state.db).await;

    // Re-enqueue vectors that were never bought, also BEFORE the runner starts.
    //
    // The recovery half of this binary's level-0 GC fix. Until it, the
    // unreferenced-jobs predicate read every `embed` job as garbage — an embed
    // job carries a BATCH as `items` and has no `raw_hash` field for the
    // predicate to find — so any batch still pending when a pass landed was
    // deleted. GC runs at boot and on a cadence, so a daemon restarted mid-scan
    // with a full queue lost exactly the work it had not finished.
    //
    // Nothing recorded the loss. The elements are there, the summaries are
    // there, `status` reports an empty queue, and the content is simply absent
    // from every semantic search. The jobs cannot come back on their own
    // either: a scan of an unchanged tree enqueues nothing.
    //
    // So the backlog is re-derived from the SCHEMA — content with no vector row
    // — which is the same self-healing shape decision D6 gives summaries, and
    // why the fix arriving as a binary is enough with no repair verb to find.
    // Bounded per boot: a long-neglected index heals over several starts rather
    // than queueing its whole content layer at once.
    match crate::enrich::requeue_missing_vectors(&state, MISSING_VECTOR_SWEEP).await {
        Ok(0) => {}
        Ok(queued) => tracing::warn!(
            queued,
            "re-queued embeddings that were never bought — a previous GC pass reaped their \
             jobs before they ran; this content was not searchable until now"
        ),
        Err(error) => tracing::error!(%error, "cannot re-queue missing embeddings"),
    }

    // And the same reconciliation one shelf up. `missing_enrichment` — the
    // decision-D6 sweep written for exactly this — existed with NO production
    // caller: only tests ever ran it, so a summary lost to the reaped-jobs
    // defect, to a crash between parse and enrichment, or to a policy change
    // had no way back at all.
    //
    // Quieter than the vector sweep by design, and the difference is real: an
    // element with no summary still has its raw vector, so search can still
    // reach it — it is thinner, not invisible. It is still spend that was
    // authorised and never delivered, and nothing else would ever notice,
    // because a scan of an unchanged tree enqueues nothing.
    match crate::enrich::requeue_missing_summaries(&state, MISSING_SUMMARY_SWEEP).await {
        Ok(0) => {}
        Ok(queued) => tracing::info!(
            queued,
            "re-queued summaries the content layer was missing; enrichment is derived from \
             the schema, so this settles on its own once they land"
        ),
        Err(error) => tracing::error!(%error, "cannot re-queue missing summaries"),
    }

    // What logging managed to do, told to the person driving (req-0059).
    //
    // Declared ONCE rather than from a reconcile pass, because unlike schema
    // skew or a pending update this condition cannot change while the process
    // runs: the log file is opened at startup and never reopened. The
    // declaration is still level-triggered — a daemon that CAN write its log
    // declares an empty set, which is what retracts the previous run's
    // complaint.
    //
    // A store that will not take the message is not worth failing the boot
    // over: the same news is already on stdout and in the `logs` row of
    // `flowspace3 doctor`.
    // `None` scope: an unwritable log directory is a fact about this HOST, not
    // about one install path, so every installation sharing the store should
    // hear it.
    if let Err(error) = fs3_store::sync_messages(
        &state.db,
        fs3_core::LOGGING_SOURCE,
        None,
        &logging.desired_messages(),
    )
    .await
    {
        tracing::warn!(%error, "cannot record the state of logging in the messages queue");
    }

    // Binding precedes credential publication. Merely binding a listener does
    // not serve requests; publishing immediately afterwards and before
    // `axum::serve` starts preserves the no-unpublished-key window.
    let listener = match listener {
        Some(listener) => listener,
        None => tokio::net::TcpListener::bind(&address)
            .await
            .with_context(|| format!("cannot bind {address}"))?,
    };
    let bound = listener.local_addr().context("cannot read bound address")?;
    let auth = auth.publish()?;

    if *shutdown.borrow() == crate::runner::Shutdown::Running
        && let Some(ready) = sandbox
    {
        tracing::info!(
            "sandbox=true embedder={} summarizer={} db={} port={} config={}",
            state.active_kind(Port::Embedder),
            state.active_kind(Port::Summarizer),
            ready.database,
            bound.port(),
            ready.config.display()
        );
    }
    // The worker loop is a background task rather than a second process: it
    // shares the composition root's provider Arcs (and therefore their HTTP
    // clients and Entra token cache), and the queue's own SKIP LOCKED claim is
    // what makes concurrency safe, so nothing is gained by isolating it.
    //
    // It is spawned BEFORE the server starts listening, so a root added by the
    // very first request is already being drained by the time the response is
    // written.
    let workers = state.config.indexing.worker_concurrency;
    tracing::info!(workers, "starting the job runner");
    let runner = tokio::spawn(crate::runner::run_until_shutdown(
        state.clone(),
        workers,
        shutdown.clone(),
    ));

    // Roots become live watchers here, and by reconciling rather than by
    // reacting: one pass compares the `worktrees` table against the watchers
    // that exist. "Watch what was already registered at boot" and "watch what
    // was added a moment ago" are therefore the SAME code path — the first is
    // just the pass that runs immediately, which `tokio::interval` gives for
    // free by firing its first tick at once.
    //
    // Registered by behaviour, as a roster (doctrine, 2026-08-26): the runner
    // takes `Vec<Box<dyn Reconcile>>` even at one implementor, because that is
    // the argument the second one joins without touching this function.
    let cadence = Duration::from_secs(RECONCILE_EVERY_SECONDS);
    tracing::info!(
        cadence_seconds = RECONCILE_EVERY_SECONDS,
        debounce_seconds = state.config.indexing.debounce_seconds,
        worktree_reconcile_ticks = state.config.indexing.worktree_reconcile_ticks,
        "starting the reconcile runner"
    );
    let mut reconcilers: Vec<Box<dyn crate::reconcile::Reconcile>> = vec![Box::new(
        crate::watch::WatcherSupervisor::new(state.clone()),
    )];
    reconcilers.push(Box::new(crate::worktrees::WorktreeSupervisor::new(
        state.clone(),
    )));

    // The second implementor joins the roster as one more `Box`, exactly as
    // the doctrine predicted — no change to the runner and none to the trait.
    // It runs on the shared cadence and rate-limits itself against
    // `update_state.last_checked_at`, so "check once a day" survives a daemon
    // that is restarted every ten minutes (PRD req 54).
    //
    // A supervisor that cannot be built is not a reason to refuse to serve:
    // the only failure here is "this process cannot resolve its own path",
    // which breaks updating and nothing else.
    match crate::update::UpdateSupervisor::new(
        state.db.clone(),
        &state.config.update,
        env!("CARGO_PKG_VERSION"),
    ) {
        Ok(supervisor) => {
            tracing::info!(
                auto = state.config.update.auto,
                every_hours = state.config.update.check_interval_hours,
                "starting the update supervisor"
            );
            reconcilers.push(Box::new(supervisor));
        }
        Err(error) => tracing::warn!(%error, "auto-update is unavailable in this process"),
    }

    // Third implementor. Boot already refused a database that was ahead of us,
    // so this one exists for the case boot cannot see: the store getting ahead
    // AFTER we started, when a newer `doctor` or a colleague's daemon migrates
    // it out from under this process (req-0061). Level-triggered, so it says
    // nothing on a healthy daemon and retracts itself if the situation
    // resolves.
    reconcilers.push(Box::new(crate::skew::SchemaSupervisor::new(
        state.db.clone(),
        env!("CARGO_PKG_VERSION"),
    )));

    // Fourth implementor, and the slowest. It counts ticks rather than growing
    // the trait a per-loop cadence — the same shape the update supervisor's
    // clock takes, for the same reason (req-0057).
    reconcilers.push(Box::new(crate::gc::GcSupervisor::new(state.db.clone())));
    let reconcile = tokio::spawn(crate::reconcile::run_forever(reconcilers, cadence));

    let server = http::serve_listener(state, listener, auth, shutdown).await;
    runner.await.context("joining the job runner")?;
    reconcile.abort();
    signal_task.abort();
    server
}

#[cfg(unix)]
fn install_shutdown_handler(
    shutdown: tokio::sync::watch::Sender<crate::runner::Shutdown>,
) -> Result<tokio::task::JoinHandle<()>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut interrupt = signal(SignalKind::interrupt()).context("listening for SIGINT")?;
    let mut terminate = signal(SignalKind::terminate()).context("listening for SIGTERM")?;
    Ok(tokio::spawn(async move {
        let first = tokio::select! {
            _ = interrupt.recv() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        };
        tracing::info!(signal = first, "shutdown requested");
        let _ = shutdown.send(crate::runner::Shutdown::Draining);

        let second = tokio::select! {
            _ = interrupt.recv() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        };
        tracing::warn!(
            signal = second,
            "second shutdown signal; cancelling in-flight work"
        );
        let _ = shutdown.send(crate::runner::Shutdown::Forced);
    }))
}

#[cfg(not(unix))]
fn install_shutdown_handler(
    shutdown: tokio::sync::watch::Sender<crate::runner::Shutdown>,
) -> Result<tokio::task::JoinHandle<()>> {
    Ok(tokio::spawn(async move {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for shutdown signal");
            return;
        }
        tracing::info!(signal = "Ctrl-C", "shutdown requested");
        let _ = shutdown.send(crate::runner::Shutdown::Draining);
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::warn!(
                signal = "Ctrl-C",
                "second shutdown signal; cancelling in-flight work"
            );
            let _ = shutdown.send(crate::runner::Shutdown::Forced);
        }
    }))
}

/// Turn the configured daemon URL into a bind address, refusing any host that
/// is not loopback.
///
/// PRD req 17 / AC-0005: fs3's HTTP surface is local-only. It is
/// unauthenticated and it fronts an index of every repo on the machine, so
/// binding `0.0.0.0` would publish that to the network. A config typo has to be
/// a startup failure, not a silent exposure.
fn bind_address(url: &str) -> Result<String> {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, remainder)| remainder);
    let authority = without_scheme
        .split('/')
        .next()
        .filter(|authority| !authority.is_empty())
        .with_context(|| format!("daemon.url {url:?} has no host:port"))?;

    let (host, port) = split_authority(authority);
    ensure!(
        is_loopback(host),
        "daemon.url {url:?} binds {host:?}, which is not loopback. fs3's HTTP \
         surface is local-only and unauthenticated (PRD req 17) — use \
         127.0.0.1, ::1, or localhost."
    );

    Ok(if port.is_some() {
        authority.to_string()
    } else {
        format!("{authority}:80")
    })
}

/// Split an authority into host and optional port, understanding the bracketed
/// IPv6 form. Splitting `[::1]:7373` on `:` would tear the address apart and
/// leave `[` looking like a hostname.
fn split_authority(authority: &str) -> (&str, Option<&str>) {
    if let Some(rest) = authority.strip_prefix('[') {
        return match rest.split_once(']') {
            Some((host, tail)) => (host, tail.strip_prefix(':')),
            None => (authority, None),
        };
    }
    match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    }
}

/// A loopback address, or the one name that always resolves to one.
///
/// Anything else is refused rather than resolved: a name that happens to point
/// at a loopback address today is not a local-only guarantee.
fn is_loopback(host: &str) -> bool {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    host.eq_ignore_ascii_case("localhost")
}

#[cfg(test)]
mod tests {
    use super::{bind_address, sandbox_configuration};

    #[test]
    fn bind_address_strips_scheme_and_path() {
        assert_eq!(
            bind_address("http://127.0.0.1:7373").unwrap(),
            "127.0.0.1:7373"
        );
        assert_eq!(
            bind_address("http://127.0.0.1:7373/").unwrap(),
            "127.0.0.1:7373"
        );
        assert_eq!(bind_address("127.0.0.1:7373").unwrap(), "127.0.0.1:7373");
        assert_eq!(bind_address("http://localhost").unwrap(), "localhost:80");
    }

    /// The finding this kills: `http://0.0.0.0:7373` used to be accepted, and
    /// the daemon then served every interface.
    #[test]
    fn bind_address_refuses_every_non_loopback_host() {
        for url in [
            "http://0.0.0.0:7373",
            "0.0.0.0:7373",
            "http://[::]:7373",
            "http://192.168.1.10:7373",
            "http://example.com:7373",
            "http://0.0.0.0",
        ] {
            let error = bind_address(url)
                .expect_err("a non-loopback bind publishes the local index to the network");
            assert!(
                error.to_string().contains("not loopback"),
                "the refusal must say why, got: {error}"
            );
        }
    }

    #[test]
    fn bind_address_accepts_every_loopback_spelling() {
        assert_eq!(bind_address("http://[::1]:7373").unwrap(), "[::1]:7373");
        assert_eq!(
            bind_address("http://127.0.0.2:7373").unwrap(),
            "127.0.0.2:7373"
        );
        assert_eq!(
            bind_address("http://LocalHost:7373").unwrap(),
            "LocalHost:7373"
        );
    }

    #[tokio::test]
    async fn sandbox_ignores_ambient_provider_and_repo_selections() {
        let ambient = tempfile::tempdir().expect("an ambient config directory");
        std::fs::write(
            ambient.path().join(fs3_core::CONFIG_FILE_NAME),
            r#"
            [database]
            url = "postgres://sandbox:sandbox@127.0.0.1:5433/source"

            [providers.paid]
            kind = "openai"
            model = "text-embedding-3-small"
            api_key_env = "FS3_KEY_THAT_IS_NOT_SET"

            [embedder]
            active = "paid"
            [summarizer]
            active = "paid"
            [agent]
            active = "paid"

            [repos."github.com/acme/repo"]
            embedder = "paid"
            summarizer = "paid"
            "#,
        )
        .expect("writing ambient configuration");
        let sandbox = tempfile::tempdir().expect("a sandbox config directory");

        let (config, base_database_url) =
            sandbox_configuration(ambient.path(), sandbox.path(), 41234)
                .expect("ambient providers never reach sandbox wiring");

        assert_eq!(
            base_database_url,
            "postgres://sandbox:sandbox@127.0.0.1:5433/source"
        );
        assert_eq!(config.daemon.url, "http://127.0.0.1:41234");
        assert_eq!(
            config.daemon.log_dir,
            sandbox.path().join("logs").to_string_lossy()
        );
        assert_eq!(
            config.selected(fs3_core::Port::Embedder, None),
            fs3_core::DEFAULT_PROVIDER
        );
        assert_eq!(
            config.selected(fs3_core::Port::Summarizer, None),
            fs3_core::DEFAULT_PROVIDER
        );
        assert_eq!(
            config.selected(fs3_core::Port::Agent, None),
            fs3_core::DEFAULT_PROVIDER
        );
        assert_eq!(config.providers.len(), 1);
        assert!(config.repos.is_empty());
        assert!(!config.update.auto);
        crate::wiring::AppState::from_config(config)
            .expect("sandbox wiring uses only offline fake providers");
    }
}
