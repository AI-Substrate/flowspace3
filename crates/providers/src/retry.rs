//! Transient-failure retry, shared by every HTTP adapter in this crate.
//!
//! ## Why this is narrow on purpose
//!
//! The daemon's job runner already retries **any** failed job three times with
//! its own backoff, without discriminating by error kind. So a second retry
//! layer here is not free: whatever this loop retries, the runner will retry
//! again around it, and the two multiply.
//!
//! Therefore this loop retries exactly one thing — a transient HTTP status
//! ([`is_transient`]: 429, 502, 503, 504) — and hands everything else straight
//! up on the first failure. A wrong deployment name, a rejected credential, a
//! malformed response: none of those get better by asking again, and the runner
//! already covers "the world changed underneath us".
//!
//! ## Who waits, and for how long
//!
//! A brief squeeze is the adapter's problem: the service said "not now", and
//! sleeping a few hundred milliseconds inside the call is cheaper than
//! unwinding a job and re-claiming it. A *sustained* squeeze is the scheduler's
//! problem, so once the attempts are spent this surfaces
//! [`Error::RateLimited`] carrying the service's own `Retry-After`, and the
//! runner parks the claim for exactly that long rather than guessing — or
//! burning one of the job's three attempts on a job that was never broken.
//!
//! Single sleeps are capped ([`RetryPolicy::cap`]) so a job cannot sit on a
//! concurrency permit for minutes while holding it against every other job.

use std::time::Duration;

use fs3_core::Error;

/// Statuses worth asking again about.
///
/// 429 is the service pacing us. 502/503/504 are a proxy or a backend that was
/// briefly not there — Azure's own SDK retries exactly this set, and fs2 shipped
/// {429, 502, 503} for years. 504 joins them because a gateway timeout is the
/// same shape of transient, and 408 does not: a client-side timeout usually
/// means the request itself was too big.
fn is_transient(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 502 | 503 | 504)
}

/// A request the service refused, kept structured so callers can decide.
pub(crate) struct Rejection {
    pub status: reqwest::StatusCode,
    /// The response body, trimmed. Read by the structured-output downgrade.
    pub detail: String,
    /// What the service asked us to wait, if it said.
    pub retry_after: Option<Duration>,
    /// The adapter's fully explained error, for when this is not retried.
    pub error: Error,
}

/// Why a POST did not produce a body.
///
/// Two callers need the distinction: the retry loop, which must not retry a
/// transport error into a duplicate request, and the summarizer's
/// structured-output downgrade, which must tell "this endpoint does not
/// understand `response_format`" from "the network is down".
pub(crate) enum PostFailure {
    Fatal(Error),
    Rejected(Rejection),
}

impl PostFailure {
    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Fatal(error) | Self::Rejected(Rejection { error, .. }) => error,
        }
    }

    /// Whether this rejection means "I do not support that `response_format`".
    ///
    /// Endpoints disagree about how to say it — OpenAI names the parameter,
    /// Azure on an older `api-version` calls it unknown, and compat servers
    /// each phrase it their own way — so the test is a client error that
    /// mentions the thing we asked for. Anything else is a real failure and
    /// must not be retried into a weaker request.
    pub(crate) fn rejects_structured_output(&self) -> bool {
        match self {
            Self::Fatal(_) => false,
            Self::Rejected(rejection) => {
                rejection.status.is_client_error() && {
                    let detail = rejection.detail.to_ascii_lowercase();
                    detail.contains("response_format") || detail.contains("json_schema")
                }
            }
        }
    }
}

impl From<PostFailure> for Error {
    fn from(failure: PostFailure) -> Self {
        failure.into_error()
    }
}

/// Read `Retry-After` in both forms the RFC allows.
///
/// Delta-seconds is what Azure and OpenAI send. An HTTP-date is legal and rarer,
/// and is converted to a wait by subtracting now — a date already in the past
/// yields no wait rather than an error, because a clock skew of a second should
/// not become a failure. Anything unparseable is `None`: the header is advice,
/// and bad advice is the same as none.
pub(crate) fn parse_retry_after(header: Option<&str>) -> Option<Duration> {
    let header = header?.trim();

    if let Ok(seconds) = header.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let deadline = azure_core::time::parse_rfc7231(header).ok()?;
    let now = azure_core::time::OffsetDateTime::now_utc();
    Some(
        (deadline - now)
            .try_into()
            .unwrap_or(Duration::from_secs(0)),
    )
}

/// Pull `Retry-After` off a response before its body is consumed.
pub(crate) fn retry_after_of(response: &reqwest::Response) -> Option<Duration> {
    parse_retry_after(
        response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
    )
}

/// How hard to try, and how long to wait.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryPolicy {
    /// Total attempts including the first. `1` disables retrying.
    pub attempts: usize,
    /// The first backoff; each subsequent one doubles.
    pub base: Duration,
    /// The longest any single sleep may be.
    pub cap: Duration,
}

impl Default for RetryPolicy {
    /// Three attempts, half a second, twenty-second ceiling.
    ///
    /// Three because the runner adds three more around this one and the two
    /// multiply — nine requests is already generous toward a service that is
    /// telling us to slow down. Twenty seconds because a job holds a
    /// concurrency permit while it sleeps, and a permit held for minutes is
    /// indistinguishable from a stall.
    fn default() -> Self {
        Self {
            attempts: 3,
            base: Duration::from_millis(500),
            cap: Duration::from_secs(20),
        }
    }
}

impl RetryPolicy {
    /// The wait before attempt `index + 1`, honouring the service's own advice.
    ///
    /// `Retry-After` wins when present: the service knows when its window
    /// reopens and we do not. Otherwise exponential backoff with **full
    /// jitter** — a uniform draw from `[0, backoff]` rather than
    /// `backoff ± noise`, because the failure mode being defended against is
    /// every worker waking at once, and only full jitter actually spreads them.
    fn wait(&self, index: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(advised) = retry_after {
            return advised.min(self.cap);
        }
        let backoff = self
            .base
            .saturating_mul(1u32 << index.min(16))
            .min(self.cap);
        backoff.mul_f64(jitter())
    }
}

/// A fraction in `[0, 1)` for full jitter.
///
/// Deliberately not the `rand` crate: this picks how long to sleep, and a
/// dependency whose only job is to make a sleep less synchronised is a
/// dependency that does not pay for itself. The low bits of the wall clock are
/// unpredictable enough for that, and nothing here is security-sensitive.
fn jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    f64::from(nanos) / f64::from(1_000_000_000u32)
}

/// Run `call`, retrying only transient HTTP failures.
///
/// Returns [`Error::RateLimited`] when the attempts are spent on a 429, so the
/// scheduler can park the claim; any other exhaustion surfaces the adapter's
/// own explained error unchanged.
pub(crate) async fn with_retry<T, F, Fut>(
    policy: RetryPolicy,
    provider: &str,
    mut call: F,
) -> std::result::Result<T, PostFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, PostFailure>>,
{
    let mut attempt = 0u32;
    loop {
        let failure = match call().await {
            Ok(value) => return Ok(value),
            Err(failure) => failure,
        };

        let rejection = match failure {
            PostFailure::Rejected(rejection) if is_transient(rejection.status) => rejection,
            // Not transient: hand it back UNCHANGED. The caller may still need
            // to read it — the summarizer's structured-output downgrade lives
            // on exactly this path — so this must not be flattened to an Error.
            other => return Err(other),
        };

        if usize::try_from(attempt + 1).unwrap_or(usize::MAX) >= policy.attempts {
            // A 429 that outlived our patience is a scheduling fact, not a
            // defect: hand the scheduler the service's own wait so it can park
            // the claim rather than guess, and so it can decline to spend one
            // of the job's attempts on a job that was never broken. Any other
            // exhausted transient keeps the adapter's explained message.
            return Err(if rejection.status.as_u16() == 429 {
                PostFailure::Fatal(Error::RateLimited {
                    provider: provider.to_string(),
                    retry_after: rejection.retry_after,
                    attempts: policy.attempts,
                })
            } else {
                PostFailure::Rejected(rejection)
            });
        }

        tokio::time::sleep(policy.wait(attempt, rejection.retry_after)).await;
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_transient_statuses_are_worth_asking_again() {
        for code in [429, 502, 503, 504] {
            assert!(
                is_transient(reqwest::StatusCode::from_u16(code).unwrap()),
                "{code} is transient"
            );
        }
        // The ones that never get better by asking again. 408 is included
        // deliberately: a client timeout usually means the request was too big.
        for code in [400, 401, 403, 404, 408, 422, 500, 501] {
            assert!(
                !is_transient(reqwest::StatusCode::from_u16(code).unwrap()),
                "{code} must not be retried"
            );
        }
    }

    #[test]
    fn retry_after_reads_delta_seconds() {
        assert_eq!(parse_retry_after(Some("30")), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after(Some(" 5 ")), Some(Duration::from_secs(5)));
    }

    /// Format an instant the way RFC 7231 says `Retry-After` may be written.
    /// Hand-rolled because the SDK parses that shape but does not expose a
    /// formatter, and a test that cannot produce the input proves nothing.
    fn http_date(at: azure_core::time::OffsetDateTime) -> String {
        let weekday = format!("{:?}", at.weekday());
        let month = format!("{:?}", at.month());
        format!(
            "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
            &weekday[..3],
            at.day(),
            &month[..3],
            at.year(),
            at.hour(),
            at.minute(),
            at.second()
        )
    }

    #[test]
    fn retry_after_reads_an_http_date() {
        let then = azure_core::time::OffsetDateTime::now_utc() + Duration::from_secs(120);

        let wait =
            parse_retry_after(Some(&http_date(then))).expect("an HTTP-date is legal Retry-After");

        // Slack for the clock moving between the two calls, and for the format
        // having one-second resolution; the point is that a date became a
        // duration at all.
        assert!(
            wait <= Duration::from_secs(120) && wait >= Duration::from_secs(115),
            "expected about two minutes, got {wait:?}"
        );
    }

    /// A date already gone is a clock skew, not a failure.
    #[test]
    fn a_past_http_date_means_no_wait() {
        assert_eq!(
            parse_retry_after(Some("Sun, 06 Nov 1994 08:49:37 GMT")),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn unparseable_advice_is_no_advice() {
        assert_eq!(parse_retry_after(Some("soon")), None);
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    /// The service's own number wins over our guess — it knows when its window
    /// reopens and we do not — but never past the cap, or one hostile header
    /// could pin a permit for an hour.
    #[test]
    fn the_services_advice_beats_the_backoff_and_still_respects_the_cap() {
        let policy = RetryPolicy::default();
        assert_eq!(
            policy.wait(0, Some(Duration::from_secs(7))),
            Duration::from_secs(7)
        );
        assert_eq!(
            policy.wait(0, Some(Duration::from_secs(3600))),
            policy.cap,
            "a permit must not be held for an hour on one header's say-so"
        );
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        let policy = RetryPolicy {
            attempts: 8,
            base: Duration::from_secs(1),
            cap: Duration::from_secs(10),
        };
        // Full jitter means each wait is a draw from [0, backoff], so the
        // ceiling is what can be asserted deterministically.
        for (index, ceiling) in [(0, 1), (1, 2), (2, 4), (3, 8), (4, 10), (9, 10)] {
            assert!(
                policy.wait(index, None) <= Duration::from_secs(ceiling),
                "attempt {index} must not exceed {ceiling}s"
            );
        }
    }

    /// Doubling must not overflow into a panic on a long-lived process.
    #[test]
    fn a_large_attempt_index_saturates_instead_of_overflowing() {
        let policy = RetryPolicy::default();
        assert!(policy.wait(64, None) <= policy.cap);
    }
}
