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

#[tokio::test]
async fn doctor_reports_degraded_when_no_daemon_is_listening() {
    let report = doctor::run(&database_url(), NOTHING_LISTENING).await;

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

    let report = doctor::run(&database_url(), &format!("http://{address}")).await;
    assert!(report.ok, "doctor failed: {:?}", report.error);
    let data = report.data.expect("doctor reports its steps");

    assert_eq!(data.verdict, doctor::DoctorReport::OK);
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
