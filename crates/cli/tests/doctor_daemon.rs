//! Doctor's daemon row (Jordan, live, 2026-08-26).
//!
//! The bug this defends against was not a crash: doctor reported a plain `ok`
//! on a machine with no daemon running. Everything it checked really was fine —
//! it just was not checking the thing standing between the user and a working
//! system.
//!
//! Two states, one property: the row reports what it found, and the VERDICT
//! reflects it. `ok: true` on the envelope stays either way, because the
//! command succeeded; it is the stack that is degraded, not the answer.

use fs3_cli::doctor;

/// Port 1 is privileged and never serves, so the probe is refused immediately
/// rather than waiting out its timeout.
const NOTHING_LISTENING: &str = "http://127.0.0.1:1";

fn database_url() -> String {
    std::env::var("FS3_TEST_DATABASE_URL")
        .unwrap_or_else(|_| fs3_core::DatabaseConfig::DEFAULT_URL.to_string())
}

/// A config pointing at the shared store and at `daemon_url`.
fn config_for(daemon_url: &str) -> fs3_core::Config {
    fs3_core::Config {
        database: fs3_core::DatabaseConfig {
            url: database_url(),
        },
        daemon: fs3_core::DaemonConfig {
            url: daemon_url.to_string(),
        },
        ..fs3_core::Config::default()
    }
}

#[tokio::test]
async fn doctor_reports_degraded_when_no_daemon_is_listening() {
    let report = doctor::run(&config_for(NOTHING_LISTENING)).await;

    assert!(
        report.ok,
        "the COMMAND succeeded — a missing daemon is a finding, not a failure: {:?}",
        report.error
    );
    let data = report.data.expect("doctor reports its steps");

    assert_eq!(
        data.verdict,
        doctor::DoctorReport::DEGRADED,
        "this is the bug: a plain ok with nothing serving was actively misleading"
    );
    assert!(
        data.healthy,
        "the STORE is still fine, and says so separately"
    );

    let row = data
        .steps
        .iter()
        .find(|step| step.check == "daemon")
        .expect("doctor must walk the daemon row");
    assert_eq!(row.outcome, "down");
    assert!(
        row.found.contains(NOTHING_LISTENING),
        "the row names the url it probed: {}",
        row.found
    );
    assert!(
        row.action
            .as_deref()
            .unwrap_or_default()
            .contains("flowspace3 daemon"),
        "and names the command that fixes it — one binary now (PRD req 51): {:?}",
        row.action
    );

    assert!(
        report
            .next_action
            .unwrap_or_default()
            .contains("flowspace3 daemon &"),
        "the steer must say what to do next"
    );
}

#[tokio::test]
async fn doctor_reports_ok_when_the_daemon_answers() {
    // A stand-in for the daemon's own `/health`: this test is about doctor's
    // probe and its verdict, not about the daemon's wiring, which
    // `fs3-daemon`'s own suite covers.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral port");
    let address = listener.local_addr().expect("bound");
    tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/health",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "status": "ok",
                    "version": "9.9.9",
                    "embedder": "fake",
                    "summarizer": "fake"
                }))
            }),
        );
        axum::serve(listener, app).await.expect("serves");
    });

    let report = doctor::run(&config_for(&format!("http://{address}"))).await;
    assert!(report.ok, "doctor failed: {:?}", report.error);
    let data = report.data.expect("doctor reports its steps");

    // The verdict is `degraded` here for a DIFFERENT reason — this config uses
    // the offline fake, which the providers row warns about — so asserting
    // `ok` would couple this test to a finding it is not about. What it must
    // prove is that the DAEMON row is satisfied and that nothing else is
    // unhappy.
    //
    // `update` is excluded deliberately, not conveniently: unlike every other
    // row, it reports state this test does not create and cannot control — a
    // machine with a newer release published, or an install path that cannot be
    // written, has a legitimate finding there. Including it would make this
    // test pass or fail on whether somebody had shipped a release that morning
    // (req-0054, w-auto-update).
    let unhappy: Vec<&str> = data
        .steps
        .iter()
        .filter(|step| step.outcome == "down" || step.outcome == "warn")
        .map(|step| step.check.as_str())
        .filter(|check| *check != "update")
        .collect();
    assert_eq!(
        unhappy,
        vec!["providers"],
        "with the daemon answering, the only finding should be the fake provider"
    );

    let row = data
        .steps
        .iter()
        .find(|step| step.check == "daemon")
        .expect("doctor must walk the daemon row");
    assert_eq!(row.outcome, "ok");
    assert!(
        row.found.contains("9.9.9"),
        "the version echo is free — it is already in the health body — and it is the fastest \
         way to see a stale binary still serving: {}",
        row.found
    );
}

/// A fresh install is not config-less — the defaults ship `[providers.fake]`
/// with both ports naming it. So the row must not claim "no provider
/// configured"; it must report the TRUE and useful thing: nothing real is
/// configured, everything will be embedded and summarised by a stand-in, and
/// here is the page that explains the choice.
///
/// This is the case Jordan hit on a fresh machine, where doctor said a plain
/// "ok" and the operator had no way to know their index would be built by a
/// deterministic fake.
#[tokio::test]
async fn doctor_warns_when_only_the_offline_fake_is_configured() {
    let report = doctor::run(&config_for(NOTHING_LISTENING)).await;
    let data = report.data.expect("doctor reports its steps");

    let row = data
        .steps
        .iter()
        .find(|step| step.check == "providers")
        .expect("doctor must walk the providers row");

    assert_eq!(
        row.outcome, "warn",
        "a fake-only stack works, so this is a finding, not a failure"
    );
    assert!(
        row.found.contains("offline fake"),
        "the row must say what is actually happening: {}",
        row.found
    );
    assert!(
        row.action
            .as_deref()
            .unwrap_or_default()
            .contains("flowspace3 docs get providers"),
        "and point at the page that fixes it: {:?}",
        row.action
    );
    assert_eq!(
        data.verdict,
        doctor::DoctorReport::DEGRADED,
        "a warn degrades the verdict — a plain ok here is the silence being fixed"
    );
}

/// A real provider whose key variable is unset fails at the first call, deep
/// inside a job, hours into an index. Naming it during diagnosis costs one
/// environment lookup.
#[tokio::test]
async fn doctor_warns_when_a_real_provider_has_no_key() {
    let mut config = config_for(NOTHING_LISTENING);
    config.providers.insert(
        "cloud".to_string(),
        fs3_core::ProviderInstance::OpenAi {
            model: "text-embedding-3-small".to_string(),
            api_base: None,
            api_key_env: "FS3_A_VARIABLE_NOBODY_HAS_SET".to_string(),
        },
    );
    config.embedder.active = "cloud".to_string();

    let report = doctor::run(&config).await;
    let data = report.data.expect("doctor reports its steps");
    let row = data
        .steps
        .iter()
        .find(|step| step.check == "providers")
        .expect("the providers row");

    assert_eq!(row.outcome, "warn");
    assert!(
        row.found.contains("FS3_A_VARIABLE_NOBODY_HAS_SET"),
        "the row must name the VARIABLE, since that is the thing to export: {}",
        row.found
    );
}

/// An active naming an instance that is not in the registry stops the daemon
/// from starting at all, so catching it where the fix is printable beats
/// meeting it as a boot failure.
#[tokio::test]
async fn doctor_warns_when_an_active_names_no_configured_instance() {
    let mut config = config_for(NOTHING_LISTENING);
    config.summarizer.active = "a-name-nobody-configured".to_string();

    let report = doctor::run(&config).await;
    let data = report.data.expect("doctor reports its steps");
    let row = data
        .steps
        .iter()
        .find(|step| step.check == "providers")
        .expect("the providers row");

    assert_eq!(row.outcome, "warn");
    assert!(
        row.found.contains("a-name-nobody-configured"),
        "the row names the dangling selection: {}",
        row.found
    );
}

/// `info` is the outcome a purely informational row uses: reported for
/// awareness, nothing wrong, verdict untouched. Added for req-0053's
/// skill-distribution row (pij-excellent-dingo) so it does not have to reach
/// for `warn` and make a healthy stack read as degraded.
#[test]
fn an_informational_row_reports_without_degrading_the_verdict() {
    let started = std::time::Instant::now();
    let note = doctor::Step::info("skills", "0 skills installed", "run `x`", started);

    assert_eq!(note.outcome, "info");
    assert!(
        !note.degrades(),
        "an informational row must not claim the stack is unhealthy"
    );
    assert!(
        note.asks_something(),
        "but it IS something the reader may act on, so it can steer"
    );

    for degrading in [
        doctor::Step::warn("x", "f", "a", started),
        doctor::Step::down("x", "f", started),
    ] {
        assert!(degrading.degrades(), "{} must degrade", degrading.outcome);
    }
    for settled in [
        doctor::Step::ok("x", "f", started),
        doctor::Step::repaired("x", "f", "a", started),
    ] {
        assert!(!settled.degrades());
        assert!(
            !settled.asks_something(),
            "{} asks nothing of the reader, so it must never steer",
            settled.outcome
        );
    }
}

/// req-0053: the skills row walks LAST (after providers) and reports
/// informationally — a stale or missing skill never degrades the verdict.
#[tokio::test]
async fn doctor_walks_the_skills_row_last_and_informationally() {
    let report = doctor::run(&config_for(NOTHING_LISTENING)).await;
    assert!(report.ok, "doctor failed: {:?}", report.error);
    let data = report.data.expect("doctor reports its steps");

    let row = data
        .steps
        .iter()
        .find(|step| step.check == "skills")
        .expect("doctor must walk the skills row");
    assert_eq!(
        row.outcome, "info",
        "the skills row reports; it never degrades"
    );
    assert!(
        !row.degrades(),
        "a stale or missing skill must not make the stack read degraded"
    );
    assert_eq!(
        data.steps.last().map(|step| step.check.as_str()),
        Some("skills"),
        "the skills row walks last, after providers"
    );
}
