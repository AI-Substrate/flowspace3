//! Turning fs3's failures into workshop-004 envelopes, and serving them.
//!
//! Two jobs, both boring on purpose:
//!
//! * [`Answer`] wraps an [`Envelope`] so axum serves it with the status its
//!   error code implies (workshop 004 D4). An endpoint returns a value; it never
//!   picks a status.
//! * [`IntoFailure`] maps each crate's own error type onto a catalog code, ONCE,
//!   here. Every endpoint that hits the store maps a [`StoreError`] the same
//!   way, so "database is unreachable" cannot be a 500 on one route and a 503 on
//!   another.
//!
//! The mapping lives in the daemon rather than in the crates that raise the
//! errors because a code is a *product* decision — what the user is told and
//! what to do about it — and `fs3-store` has no opinion about CLI verbs.

use axum::Json;
use axum::response::{IntoResponse, Response};
use fs3_core::catalog;
use fs3_core::envelope::{Envelope, Failure};
use fs3_store::StoreError;
use serde::Serialize;

use crate::wiring::AppState;

/// An envelope on its way out of an axum handler.
///
/// Wrapping rather than implementing [`IntoResponse`] on [`Envelope`] directly:
/// the envelope lives in `fs3-core`, which has no web framework and is not
/// getting one.
pub struct Answer<T>(pub Envelope<T>);

impl<T: Serialize> IntoResponse for Answer<T> {
    fn into_response(self) -> Response {
        let status = axum::http::StatusCode::from_u16(self.0.http_status())
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self.0)).into_response()
    }
}

impl<T> From<Envelope<T>> for Answer<T> {
    fn from(envelope: Envelope<T>) -> Self {
        Answer(envelope)
    }
}

/// Build a success answer carrying the daemon's live user messages.
///
/// Takes `&AppState` rather than being a free function, and that is the whole
/// enforcement mechanism for PRD req 59: an endpoint physically cannot build
/// an envelope without the thing that fetches the queue, so "every command
/// carries the messages" is a compile error rather than a review comment.
///
/// The alternative considered and refused was a response middleware. It is the
/// only shape that needs no call sites at all, but it works on serialised
/// BYTES — it would have to re-parse every response body to insert a field,
/// and it would have to sniff whether a body is an envelope at all (`/health`
/// is not), which is exactly the shape-sniffing workshop 004 D1 forbids.
pub async fn ok<T>(state: &AppState, command: &str, data: T) -> Answer<T> {
    Answer(Envelope::ok(command, data).with_messages(state.messages().await))
}

/// Build a failure answer carrying the daemon's live user messages.
///
/// A failure carries them too, deliberately. "Your search found nothing" plus
/// "a newer binary is waiting for a restart" is very often cause and effect,
/// and a user who only sees the first one debugs the wrong thing.
pub async fn failed<T>(state: &AppState, command: &str, failure: Failure) -> Answer<T> {
    Answer(Envelope::failed(command, failure).with_messages(state.messages().await))
}

/// Map an adapter's error onto the catalog, once, in one place.
pub trait IntoFailure {
    /// The catalog entry and concrete message for this failure.
    fn into_failure(self) -> Failure;
}

impl IntoFailure for StoreError {
    fn into_failure(self) -> Failure {
        // A missing database looks like a connection failure and has a
        // completely different fix, so it is separated before anything else.
        if fs3_store::is_missing_database(&self) {
            return Failure::new(&catalog::STORE_DATABASE_MISSING, self.to_string());
        }
        match self {
            StoreError::Unreachable { .. } => {
                Failure::new(&catalog::STORE_UNAVAILABLE, self.to_string())
            }
            StoreError::Dimensions { .. } => {
                Failure::new(&catalog::PROVIDER_DIMENSIONS, self.to_string())
            }
            StoreError::InvalidName(_) => Failure::new(&catalog::CONFIG_INVALID, self.to_string()),
            // A refused VALUE fails identically on every attempt, so it ends
            // the attempt instead of retrying toward the same wall (row 203).
            other if fs3_store::is_data_exception(&other) => {
                Failure::new(&catalog::STORE_QUERY_FAILED, other.to_string())
                    .with_fix(
                        "the input carries a value Postgres refuses, so repeating it fails the \
                         same way; the failed job's key names the input",
                    )
                    .retryable(false)
            }
            other => Failure::new(&catalog::STORE_QUERY_FAILED, other.to_string()),
        }
    }
}

impl IntoFailure for fs3_core::Error {
    fn into_failure(self) -> Failure {
        match &self {
            fs3_core::Error::Provider(_) | fs3_core::Error::InputTooLong { .. } => {
                Failure::new(&catalog::PROVIDER_FAILED, self.to_string())
            }
            // Congestion, not failure. The details carry what the SERVICE
            // said, because the worker parks on the provider's schedule rather
            // than inventing one — and `attempts` tells it how hard the
            // adapter already tried, which is the difference between "briefly
            // busy" and "genuinely saturated".
            fs3_core::Error::RateLimited {
                provider,
                retry_after,
                attempts,
            } => {
                let mut failure = Failure::new(&catalog::PROVIDER_RATE_LIMITED, self.to_string())
                    .with_detail("provider", provider)
                    .with_detail("provider_attempts", attempts);
                if let Some(wait) = retry_after {
                    failure = failure.with_detail("retry_after_secs", wait.as_secs_f64());
                }
                failure
            }
            fs3_core::Error::InvalidConfig(_) => {
                Failure::new(&catalog::CONFIG_INVALID, self.to_string())
            }
            _ => Failure::new(&catalog::STORE_QUERY_FAILED, self.to_string()),
        }
    }
}

impl IntoFailure for fs3_git::Error {
    fn into_failure(self) -> Failure {
        match &self {
            fs3_git::Error::NotAWorktree { .. } => {
                Failure::new(&catalog::GIT_NOT_A_WORKTREE, self.to_string())
            }
            _ => Failure::new(&catalog::SCAN_DISCOVERY_FAILED, self.to_string()),
        }
    }
}

impl IntoFailure for fs3_parsers::discovery::DiscoveryError {
    fn into_failure(self) -> Failure {
        use fs3_parsers::discovery::DiscoveryError;
        match &self {
            DiscoveryError::NotADirectory(_) => {
                Failure::new(&catalog::SCAN_ROOT_NOT_FOUND, self.to_string())
            }
            DiscoveryError::Glob { .. } => {
                Failure::new(&catalog::SCAN_DISCOVERY_FAILED, self.to_string())
            }
        }
    }
}

impl IntoFailure for crate::roots::RootError {
    fn into_failure(self) -> Failure {
        use crate::roots::RootError;
        match self {
            // The fix names the resolved path, not the typed one: a relative
            // path from an HTTP client resolves against the DAEMON's working
            // directory, and "no such path: ./src" sends the reader looking in
            // the wrong place.
            RootError::NotFound(path) => Failure::new(
                &catalog::SCAN_ROOT_NOT_FOUND,
                format!("no such path: {path}"),
            )
            .with_fix(format!(
                "check the path exists and is readable; pass an ABSOLUTE path — a \
                         relative one is resolved against the daemon's working directory, not \
                         yours. Tried: {path}"
            ))
            .with_detail("path", path),
            RootError::NotADirectory(path) => Failure::new(
                &catalog::SCAN_ROOT_NOT_FOUND,
                format!("not a directory: {path}"),
            )
            .with_fix("`flowspace3 add` takes a directory; pass the folder that contains the file")
            .with_detail("path", path),
            RootError::NotRegistered(path) => Failure::new(
                &catalog::SCAN_ROOT_NOT_REGISTERED,
                format!("no root is registered at {path}"),
            )
            .with_fix(format!(
                "run `flowspace3 add {path}` first — `flowspace3 status` lists what is registered"
            ))
            .with_detail("path", path),
            RootError::Git(error) => error.into_failure(),
            RootError::Discovery(error) => error.into_failure(),
            RootError::Store(error) => error.into_failure(),
        }
    }
}

impl IntoFailure for fs3_parsers::ScanError {
    fn into_failure(self) -> Failure {
        Failure::new(&catalog::SCAN_UNPARSEABLE, self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path mistake is the caller's to fix, and the fix has to name the path
    /// that was actually tried — a relative path resolves against the DAEMON's
    /// working directory, so echoing what the user typed sends them to the
    /// wrong place.
    #[test]
    fn a_missing_root_is_a_404_whose_fix_names_the_resolved_path() {
        let failure = crate::roots::RootError::NotFound("/srv/code/api".to_string()).into_failure();

        assert_eq!(failure.code, "FS3-E-SCAN-ROOT-NOT-FOUND");
        assert_eq!(failure.http_status(), 404);
        assert!(!failure.retryable, "the path will not appear by itself");
        assert!(failure.fix.contains("/srv/code/api"));
        assert!(
            failure.fix.contains("ABSOLUTE"),
            "the daemon-cwd trap has to be named, not implied"
        );
    }

    /// `scan` on an unregistered path must not read as "nothing changed": the
    /// fix is a different command, and naming it is the difference between a
    /// dead end and a next step.
    #[test]
    fn an_unregistered_root_points_at_add_rather_than_failing_silently() {
        let failure =
            crate::roots::RootError::NotRegistered("/srv/code/api".to_string()).into_failure();

        assert_eq!(failure.code, "FS3-E-SCAN-ROOT-NOT-REGISTERED");
        assert!(failure.fix.contains("flowspace3 add /srv/code/api"));
    }

    /// A vector of the wrong width is the provider's problem, not the store's:
    /// the fix is a different model, so it must not read as a database fault.
    #[test]
    fn a_dimension_mismatch_points_at_the_model_not_the_database() {
        let failure = StoreError::Dimensions {
            expected: 1024,
            actual: 32,
        }
        .into_failure();
        assert_eq!(failure.code, "FS3-E-PROVIDER-DIMENSIONS");
        assert!(
            !failure.retryable,
            "retrying cannot change a vector's width"
        );
    }

    /// Backlog row 203: a value Postgres refuses (SQLSTATE class 22) fails the
    /// same way on every attempt, so it must end the attempt rather than retry.
    /// Every other class keeps today's retry verdict. Real server errors, not
    /// hand-built ones, so the SQLSTATE is the one Postgres actually sends.
    #[tokio::test]
    async fn a_refused_value_is_final_and_every_other_store_failure_still_retries() {
        let database = fs3_testkit::FreshDatabase::create("answer-sqlstate").await;
        let pool = fs3_store::connect(&database.url()).await.unwrap();
        let cases = [
            // The prod failure: the `\u0000` escape in jsonb.
            (r#"SELECT '"a\u0000b"'::jsonb"#, "22P05", false),
            ("SELECT 'not-a-time'::timestamptz", "22007", false),
            ("SELECT 1 / 0", "22012", false),
            ("SELECT * FROM no_such_table", "42P01", true),
            ("SELECT 1 FROM", "42601", true),
            (
                "CREATE TEMP TABLE once (id int PRIMARY KEY); \
                 INSERT INTO once VALUES (1), (1)",
                "23505",
                true,
            ),
        ];
        for (sql, sqlstate, retryable) in cases {
            let error = StoreError::from(sqlx::raw_sql(sql).execute(&pool).await.unwrap_err());
            let code = match &error {
                StoreError::Query(inner) => inner
                    .as_database_error()
                    .and_then(|db| db.code())
                    .map(|code| code.into_owned()),
                _ => None,
            };
            assert_eq!(code.as_deref(), Some(sqlstate), "{sql}");
            let failure = error.into_failure();
            assert_eq!(failure.code, "FS3-E-STORE-QUERY-FAILED", "{sql}");
            assert_eq!(failure.retryable, retryable, "{sqlstate}: {sql}");
            assert_eq!(
                crate::runner::verdict(&failure, 1, 0) == crate::runner::Verdict::Fail,
                !retryable,
                "{sqlstate}: only a refused value ends the first attempt"
            );
        }
        database.destroy(pool).await;
    }
}
