//! The environment a test is allowed to hand a `flowspace3` subprocess.
//!
//! # Why this exists
//!
//! [`crate::database`] closed the in-process hole: a test that opens a pool
//! must be told which database it may write to. On 2026-08-27 the SAME class of
//! incident happened again through a door that gate does not stand in — a test
//! that opens no pool at all, and instead SPAWNS the real binary.
//!
//! `crates/daemon/tests/health.rs` started `flowspace3 daemon` against a temp
//! config directory whose `config.toml` had no `[database]` section. Every
//! layer then did its job: config resolution fell through to
//! [`fs3_core::DatabaseConfig::DEFAULT_URL`], which is the SHIPPED address and
//! therefore the real store on a developer machine; daemon boot migrates before
//! it serves, because the daemon is the single writer. Migration 0012 landed on
//! Jordan's production database sixteen seconds after it landed on the test
//! one, the installed CLI hard-refused on the resulting skew, and production
//! was down until an emergency rebuild.
//!
//! # The finding this module is shaped by
//!
//! Before this module, **no test in the repo did both halves**. The scrub and
//! the pin each existed, in different files, and neither file was wrong on its
//! own:
//!
//! * `crates/cli/tests/boot_contract.rs` scrubbed inherited `FS3_*` — so an
//!   ambient override in a developer's shell could not beat the fixture — but
//!   pinned no database, because its fixture named one.
//! * `crates/cli/tests/daemon_logging.rs` and `docs_bundle.rs` pinned
//!   `FS3_DATABASE__URL` at something unreachable, but never scrubbed, so an
//!   ambient `FS3_*` still reached the child.
//!
//! A correct pattern that exists only as one half here and the other half there
//! is not a pattern; it is two coincidences. Copying either file faithfully
//! produced a leak. That is why this is a helper with a type in it rather than
//! a comment asking the next author to remember both.
//!
//! # The rule
//!
//! A test may hand a subprocess an environment in exactly one shape: every
//! inherited `FS3_*` removed, then [`fs3_core::CONFIG_DIR_ENV`] and
//! [`DATABASE_URL_ENV`] set explicitly. There is no "this one reads no
//! database" exemption — `docs get` opens no pool TODAY, and the incident above
//! is what one change to a startup path costs. Enforced mechanically by
//! `crates/testkit/tests/spawn_isolation.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The config override naming the store, spelled the way [`fs3_core`] nests:
/// [`fs3_core::ENV_PREFIX`] + section + [`fs3_core::ENV_NESTING`] + key.
pub const DATABASE_URL_ENV: &str = "FS3_DATABASE__URL";

/// A URL that parses as Postgres and connects to nothing.
///
/// Port 1 is reserved (`tcpmux`) and nothing in this project's compose stack
/// binds it, so a connection attempt fails fast rather than hanging.
pub const UNREACHABLE_DATABASE_URL: &str = "postgres://nobody@127.0.0.1:1/nothing";

/// Which non-production database a spawned binary is pointed at.
///
/// Every arm is safe; the choice is about what the test is proving, not about
/// blast radius. None of them can reach the shipped default, which is the
/// point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestDatabase {
    /// The disposable database this test run was told it may write to
    /// ([`crate::test_database_url`]).
    ///
    /// For subprocesses that must genuinely start: daemon boot migrates and
    /// exits non-zero if it cannot, so a daemon pointed anywhere unreachable
    /// does not fail the assertion under test — it fails to exist.
    Scratch,
    /// [`UNREACHABLE_DATABASE_URL`], for a subprocess that must prove it needs
    /// no store at all (`docs get`, `ping --daemon-url`).
    ///
    /// Stronger than [`Self::Scratch`] here: if the claim "this verb opens no
    /// pool" ever stops being true, the test says so instead of quietly
    /// starting to write somewhere.
    Unreachable,
    /// The `config.toml` in the config directory is the thing under test, and
    /// it sets `[database].url` itself — so pinning the environment would beat
    /// the fixture and prove nothing (`boot_contract.rs`).
    ///
    /// This arm sets no URL, which would be a hole in the seal if it were
    /// taken on trust. It is not: [`sealed`] READS the fixture and refuses
    /// unless it really does set `[database].url`. That check is the whole
    /// reason this arm may exist — and it is exactly the check that would have
    /// stopped the 2026-08-27 incident, whose fixture had no `[database]`
    /// section at all.
    FromConfigFile,
}

impl TestDatabase {
    /// The URL to pin, or [`None`] when the fixture file supplies it.
    ///
    /// # Panics
    /// For [`Self::Scratch`], when the run was never told which database it
    /// may use — the refusal in [`crate::database`].
    fn url(self, config_dir: &Path) -> Option<String> {
        match self {
            Self::Scratch => Some(crate::test_database_url()),
            Self::Unreachable => Some(UNREACHABLE_DATABASE_URL.to_string()),
            Self::FromConfigFile => {
                assert_fixture_names_a_database(config_dir);
                None
            }
        }
    }
}

/// Refuse a `FromConfigFile` spawn whose fixture does not actually name a
/// database — the shape that falls through to
/// [`fs3_core::DatabaseConfig::DEFAULT_URL`], which is production.
///
/// Deliberately a substring check rather than a TOML parse of the merged
/// config: the question is "did the AUTHOR of this fixture make the decision",
/// and a `[database]` section they wrote is the evidence. Parsing would also
/// accept a section that arrived from somewhere else.
fn assert_fixture_names_a_database(config_dir: &Path) {
    let path = config_dir.join(fs3_core::CONFIG_FILE_NAME);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "TestDatabase::FromConfigFile promises that {} sets [database].url, \
             but it cannot be read ({error}).\n\
             Without it the child falls through to DatabaseConfig::DEFAULT_URL, \
             which is the SHIPPED address — the production store on a developer \
             machine. Use TestDatabase::Scratch or ::Unreachable instead.",
            path.display()
        )
    });

    // `toml::Table`, not `toml::Value`: a bare `Value` parse reads a leading
    // `[database]` as an ARRAY literal and then rejects the rest of the file.
    // A config file is a document.
    let parsed: toml::Table = text
        .parse()
        .unwrap_or_else(|error| panic!("{} is not valid TOML ({error})", path.display()));
    let names_a_database = parsed
        .get("database")
        .and_then(|section| section.get("url"))
        .and_then(toml::Value::as_str)
        .is_some_and(|url| !url.trim().is_empty());

    assert!(
        names_a_database,
        "TestDatabase::FromConfigFile promises that {} sets [database].url, and it \
         does not.\n\
         This is the 2026-08-27 incident exactly: a fixture with no [database] \
         section makes the child resolve DatabaseConfig::DEFAULT_URL, which is the \
         SHIPPED address and therefore the production store on a developer machine, \
         and daemon boot MIGRATES it.\n\
         Either add [database].url to the fixture, or spawn with \
         TestDatabase::Scratch / ::Unreachable.",
        path.display()
    );
}

/// The `flowspace3` binary from this workspace's target directory.
///
/// `CARGO_BIN_EXE_*` only covers bins in the package being tested, and the CLI
/// lives in its own — so a daemon test cannot use it. Under the mandated gate
/// (`cargo test --all`) cargo builds every workspace binary before running any
/// test, so this path is populated.
///
/// # Panics
/// When the binary is absent, naming the build that produces it.
#[must_use]
pub fn flowspace3_binary() -> PathBuf {
    let mut directory = std::env::current_exe().expect("the test binary has a path");
    directory.pop(); // the test executable itself
    directory.pop(); // deps/
    let candidate = directory.join(format!("flowspace3{}", std::env::consts::EXE_SUFFIX));
    assert!(
        candidate.is_file(),
        "{} is missing. This test drives the real CLI, so build the workspace \
         first: cargo build --workspace",
        candidate.display()
    );
    candidate
}

/// A [`Command`] for `binary` that cannot inherit or default its way to the
/// production store.
///
/// `config_dir` should be a directory this test owns — a `tempfile::TempDir`,
/// or the fixture directory whose `config.toml` is the thing under test. It is
/// pinned even when empty, because an unpinned child reads the developer's real
/// `config.toml` AND their real `secrets.env`.
///
/// # Panics
/// For [`TestDatabase::Scratch`], when [`crate::TEST_DATABASE_ENV`] is unset.
/// For [`TestDatabase::FromConfigFile`], when the fixture does not set
/// `[database].url`.
#[must_use]
pub fn sealed(binary: &Path, config_dir: &Path, database: TestDatabase) -> Command {
    let mut command = Command::new(binary);

    // Order matters: scrub the whole namespace first, then set what this test
    // means. An ambient `FS3_DATABASE__URL` in a developer's shell is a HIGHER
    // precedence layer than any config file (fs3_core::config), so a fixture
    // that does not scrub is testing the developer's environment rather than
    // its own fixture — observed live, reported as a hang.
    //
    // This also removes `FS3_TEST_DATABASE_URL` itself, which is deliberate:
    // it is a marker for test PROCESSES, and a sealed child is already pinned,
    // so it has no business inheriting one. Its absence here is what lets
    // `fs3_daemon::boot` treat "marker present AND database came from the
    // defaults layer" as proof of an UNSEALED spawn.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with(fs3_core::ENV_PREFIX) {
            command.env_remove(&key);
        }
    }

    command.env(fs3_core::CONFIG_DIR_ENV, config_dir);
    if let Some(url) = database.url(config_dir) {
        command.env(DATABASE_URL_ENV, url);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scrub has to cover the whole prefix, not just the two names this
    /// module sets — `FS3_EMBEDDER__ACTIVE` in a shell would otherwise pick the
    /// provider a fixture is asserting about.
    #[test]
    fn every_fs3_variable_is_removed_before_the_pins_are_set() {
        // Asserted on the command's recorded env mutations rather than by
        // spawning: this is about what the child WOULD receive, and spawning a
        // real binary to find out is the cost this test exists to avoid.
        let command = sealed(
            Path::new("/bin/true"),
            Path::new("/tmp/fs3-sealed-test"),
            TestDatabase::Unreachable,
        );

        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().to_string())
            .collect();
        let set: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|v| {
                    (
                        key.to_string_lossy().to_string(),
                        v.to_string_lossy().to_string(),
                    )
                })
            })
            .collect();

        for (key, _) in std::env::vars_os() {
            let key = key.to_string_lossy().to_string();
            if key.starts_with(fs3_core::ENV_PREFIX) && !set.contains_key(&key) {
                assert!(
                    removed.contains(&key),
                    "{key} was inherited into a sealed command"
                );
            }
        }

        assert_eq!(
            set.get(DATABASE_URL_ENV).map(String::as_str),
            Some(UNREACHABLE_DATABASE_URL)
        );
        assert_eq!(
            set.get(fs3_core::CONFIG_DIR_ENV).map(String::as_str),
            Some("/tmp/fs3-sealed-test")
        );
    }

    /// The pins must survive the scrub — a loop that removed by prefix AFTER
    /// setting them would leave the child with neither, which is exactly the
    /// unsealed shape.
    #[test]
    fn the_pins_are_not_themselves_scrubbed() {
        // SAFETY: single-threaded within this test's own process view; the
        // variable is removed again immediately.
        unsafe { std::env::set_var("FS3_DATABASE__URL", "postgres://ambient/should-not-win") };
        let command = sealed(
            Path::new("/bin/true"),
            Path::new("/tmp/fs3-sealed-test"),
            TestDatabase::Unreachable,
        );
        unsafe { std::env::remove_var("FS3_DATABASE__URL") };

        let pinned = command
            .get_envs()
            .find(|(key, _)| key.to_string_lossy() == DATABASE_URL_ENV)
            .and_then(|(_, value)| value)
            .map(|v| v.to_string_lossy().to_string());
        assert_eq!(pinned.as_deref(), Some(UNREACHABLE_DATABASE_URL));
    }
}
