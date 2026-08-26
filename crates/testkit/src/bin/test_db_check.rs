//! `fs3-test-db-check` — assert that a test run has been told which database it
//! may write to.
//!
//! Wired into `harness checks` as the `testdb` gate, ahead of everything that
//! compiles. It changes no outcome — `fs3_testkit::test_database_url` refuses on
//! its own, inside whichever test binary reaches it first — it changes WHEN and
//! HOW LEGIBLY you find out: one line before the compile, instead of the same
//! refusal buried in test output three minutes in.
//!
//! See `fs3_testkit::database` for the ruling and the incident behind it.

fn main() -> std::process::ExitCode {
    match std::env::var(fs3_testkit::TEST_DATABASE_ENV) {
        Ok(url) if !url.trim().is_empty() => {
            // Echo it. "Which database am I about to write to" is the question
            // this whole gate exists to make un-guessable, so answering it out
            // loud costs one line and removes the last place to be wrong.
            println!(
                "tests will use {}",
                fs3_core::redact_url_password(url.trim())
            );
            std::process::ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{}", fs3_testkit::refusal());
            std::process::ExitCode::FAILURE
        }
    }
}
