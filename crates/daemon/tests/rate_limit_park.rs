//! Congestion is not poison.
//!
//! A rate-limited job has not failed — the provider is working perfectly and is
//! busy. If a 429 spends one of the job's three attempts, a sustained squeeze
//! terminally fails a backlog of work that was never broken, and the index ends
//! up permanently short of vectors nobody will ever be told are missing.
//!
//! So the park path is deliberately NOT the retry path, and these tests pin the
//! difference.

mod support;

use std::time::Duration;

use fs3_core::catalog;
use fs3_core::envelope::Failure;
use fs3_daemon::runner::{MAX_ATTEMPTS, MAX_PARKS, Verdict, park_delay, retry_after_of, verdict};
use sqlx::Row;

/// A rate-limit failure as `IntoFailure` builds one.
fn rate_limited(retry_after: Option<f64>) -> Failure {
    let failure = Failure::new(&catalog::PROVIDER_RATE_LIMITED, "azure rate limited us");
    match retry_after {
        Some(secs) => failure.with_detail("retry_after_secs", secs),
        None => failure,
    }
}

/// Congestion parks, and parking is chosen INDEPENDENTLY of the attempt count.
///
/// A job on its last attempt is still parked, because the attempt ladder is
/// about the job being broken and this job is not.
#[test]
fn a_rate_limit_parks_whatever_the_attempt_count() {
    for attempts in 0..=MAX_ATTEMPTS + 2 {
        assert_eq!(
            verdict(&rate_limited(None), attempts, 0),
            Verdict::Park,
            "attempt {attempts} is irrelevant to congestion"
        );
    }
}

/// Parking is bounded. A provider that throttles forever must eventually be
/// reported rather than parked in silence for the life of the daemon.
#[test]
fn parking_stops_being_the_answer_once_it_stops_being_congestion() {
    assert_eq!(
        verdict(&rate_limited(None), 1, MAX_PARKS - 1),
        Verdict::Park
    );
    assert_eq!(
        verdict(&rate_limited(None), 1, MAX_PARKS),
        Verdict::Retry,
        "at the bound it rejoins the ordinary ladder rather than parking again"
    );
    assert_eq!(
        verdict(&rate_limited(None), MAX_ATTEMPTS, MAX_PARKS),
        Verdict::Fail,
        "and once the ladder is spent too, it is a failure like any other"
    );
}

/// An ordinary provider failure is untouched by any of this.
#[test]
fn a_normal_failure_still_retries_then_fails() {
    let failure = Failure::new(&catalog::PROVIDER_FAILED, "deployment not found");
    assert_eq!(verdict(&failure, 1, 0), Verdict::Retry);
    assert_eq!(verdict(&failure, MAX_ATTEMPTS, 0), Verdict::Fail);

    let terminal = Failure::new(&catalog::QUEUE_JOB_FAILED, "bad payload").retryable(false);
    assert_eq!(
        verdict(&terminal, 1, 0),
        Verdict::Fail,
        "non-retryable stays non-retryable"
    );
}

/// When the service says how long, we wait at least that long.
///
/// Inventing our own interval when the provider named one is how you get
/// throttled again immediately.
#[test]
fn the_services_own_interval_is_honoured() {
    let asked = Duration::from_secs(30);
    let delay = park_delay(0, Some(asked));
    assert!(
        delay >= asked,
        "never come back sooner than we were asked: {delay:?}"
    );
    assert!(
        delay < asked * 2,
        "and not absurdly later either: {delay:?}"
    );

    assert_eq!(
        retry_after_of(&rate_limited(Some(12.5))),
        Some(Duration::from_secs_f64(12.5))
    );
    assert_eq!(retry_after_of(&rate_limited(None)), None);
}

/// With no interval given, the wait grows and is capped — and is never
/// identical twice, so a merged batch does not wake as a thundering herd.
#[test]
fn the_default_wait_grows_is_capped_and_is_jittered() {
    let first = park_delay(0, None);
    let later = park_delay(3, None);
    assert!(later > first, "backs off as parks accumulate");
    assert!(
        park_delay(MAX_PARKS, None) <= Duration::from_secs(75),
        "capped rather than growing without limit"
    );

    // Jitter: not a strict guarantee on any two samples, but over a spread of
    // calls the values must not all be identical.
    let samples: Vec<Duration> = (0..24).map(|_| park_delay(2, None)).collect();
    assert!(
        samples.iter().any(|d| *d != samples[0]),
        "k jobs parked by one merged call must not all wake in the same instant"
    );
}

/// The store half: parking gives the attempt BACK.
#[tokio::test]
async fn parking_returns_the_attempt_it_was_claimed_with() {
    let database = support::FreshDatabase::create("park_attempts").await;
    let pool = fs3_store::connect(&database.url()).await.expect("connects");
    fs3_store::migrate(&pool).await.expect("migrates");

    fs3_store::enqueue_job(
        &pool,
        "embed",
        "embed:park:1",
        &serde_json::json!({ "identity": "git:a", "source": "raw", "items": [] }),
        Duration::ZERO,
    )
    .await
    .expect("enqueues");

    let job = fs3_store::claim_job(&pool, &["embed"])
        .await
        .expect("claims")
        .expect("a job");
    assert_eq!(job.attempts, 1, "claiming spends an attempt");
    assert_eq!(job.parks, 0);

    let parks = fs3_store::park_job(&pool, job.id, Duration::from_secs(1))
        .await
        .expect("parks");
    assert_eq!(parks, 1, "and the park is counted");

    let row = sqlx::query("SELECT state, attempts, parks FROM jobs WHERE id = $1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(row.try_get::<String, _>("state").expect("state"), "pending");
    assert_eq!(
        row.try_get::<i32, _>("attempts").expect("attempts"),
        0,
        "the attempt is GIVEN BACK — congestion costs the job nothing"
    );
    assert_eq!(row.try_get::<i32, _>("parks").expect("parks"), 1);

    database.destroy(pool).await;
}
