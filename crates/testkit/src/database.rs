//! The database a test is allowed to touch, and the refusal that keeps it that
//! way.
//!
//! # Why this exists
//!
//! On 2026-08-27 a `harness checks` run on a developer machine applied
//! migrations 0008 and 0009 to Jordan's **production** database. Nothing
//! malfunctioned; every piece behaved as designed:
//!
//! * the test helpers fell back to [`DatabaseConfig::DEFAULT_URL`] when
//!   `FS3_TEST_DATABASE_URL` was unset,
//! * that default is the *shipped* address, which on a developer machine is the
//!   real store rather than a container,
//! * and `flowspace3 doctor` — which several tests call — does not merely READ
//!   a schema, it MIGRATES it, because repairing is doctor's whole job.
//!
//! Three correct behaviours composing into a production write. The defect was
//! that "which database" had a DEFAULT at all: an implicit answer to a question
//! whose wrong answer is unrecoverable.
//!
//! # The rule
//!
//! Ruled by Jordan, 2026-08-27
//! (`.harness/government/rulings/2026-08-27-production-database.md`): **a test
//! that cannot prove its database is disposable does not run.** Refusal is the
//! default. There is no fallback, and adding one back is a change to a ruling,
//! not to a helper.
//!
//! # Why explicitness, and not "refuse the default URL"
//!
//! The obvious-looking rule — reject [`DatabaseConfig::DEFAULT_URL`] — is the
//! wrong one. CI sets `FS3_TEST_DATABASE_URL` to exactly that string, and it is
//! correct there: on a runner it names a disposable service container that dies
//! with the job. The same characters mean "throwaway" in one place and
//! "production" in another, so the URL cannot be the discriminator.
//!
//! What distinguishes the two is that somebody **said so on purpose**. That is
//! what this asks for, and it is all it asks for.

/// The environment variable that names the database tests may use.
pub const TEST_DATABASE_ENV: &str = "FS3_TEST_DATABASE_URL";

/// The database this test run is allowed to touch.
///
/// Deliberately uncached. The env lookup is free at this call rate, and a
/// cached refusal poisons: the first test would print the reason and every
/// other one would print "previously poisoned", burying the message under the
/// noise it exists to prevent.
///
/// # Panics
/// When [`TEST_DATABASE_ENV`] is unset or empty — naming what would have been
/// written to, and how to opt in. Panicking rather than skipping is deliberate
/// and matches the rest of this repo's test tier: a silently-skipped
/// integration test is how a regression reaches main.
#[must_use]
pub fn test_database_url() -> String {
    match std::env::var(TEST_DATABASE_ENV) {
        Ok(url) if !url.trim().is_empty() => url,
        _ => panic!("{}", refusal()),
    }
}

/// The message a refusal prints. Separated so it can be asserted on without a
/// test having to catch a panic in a subprocess.
#[must_use]
pub fn refusal() -> String {
    format!(
        "REFUSING TO RUN: {TEST_DATABASE_ENV} is not set.\n\
         \n\
         These tests write to a database — several of them run `flowspace3 doctor`, which \
         APPLIES MIGRATIONS. Without this variable there is no way to tell a throwaway \
         database from the one you actually use, so this refuses rather than guesses. On \
         2026-08-27 the old guess-the-default behaviour migrated a production database \
         (ruling: .harness/government/rulings/2026-08-27-production-database.md).\n\
         \n\
         Name a database you are happy to lose. A separate database on the SAME \
         compose stack is enough — the tests create it if it is missing, and \
         nothing you use lives in it:\n\
         \n    export {TEST_DATABASE_ENV}=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test\n\
         \n\
         CI sets this variable itself, which is why the gate is green there.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal has one job — telling a reader what nearly happened and what
    /// to do — so its content is asserted rather than merely its existence.
    #[test]
    fn the_refusal_names_the_variable_the_risk_and_the_way_out() {
        let message = refusal();

        assert!(message.contains(TEST_DATABASE_ENV));
        assert!(
            message.contains("APPLIES MIGRATIONS"),
            "the reader has to know doctor WRITES, or refusing looks like fussiness"
        );
        assert!(
            message.contains("export "),
            "a refusal without the command to fix it is half a message"
        );
        assert!(
            message.contains("CI sets this variable"),
            "otherwise the first thought is 'is the gate broken?'"
        );
    }
}
