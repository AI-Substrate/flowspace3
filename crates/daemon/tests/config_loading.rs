//! Loading configuration from a directory: the file, the `FS3_*` overrides,
//! and the secrets chain.
//!
//! These tests mutate the process environment, which Rust 2024 makes `unsafe`
//! precisely because another thread may be reading it. Every test in this
//! binary therefore takes [`env_lock`] as its first statement and holds it to
//! the end, so exactly one test thread is ever running here — including the
//! parts that only *read* the environment, like `std::env::temp_dir`.

use std::path::Path;
use std::sync::{LazyLock, Mutex, MutexGuard};

use fs3_core::{CONFIG_DIR_ENV, Config, Layer, Port};
use fs3_daemon::config;

mod support;

/// Serialize this binary's tests. A poisoned lock is still a valid guard: one
/// failed test must not cascade into "all the others panicked too".
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Set a variable for the duration of one test.
///
/// SAFETY (every call site): the caller holds [`env_lock`], so no other test
/// thread in this binary is reading or writing the environment.
fn set(name: &str, value: &str) {
    unsafe { std::env::set_var(name, value) };
}

fn unset(name: &str) {
    unsafe { std::env::remove_var(name) };
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).expect("writing a fixture file");
}

#[test]
fn the_environment_overrides_the_file() {
    let _guard = env_lock();
    let dir = support::temp_dir("env-override");
    write(
        &dir,
        "config.toml",
        "[database]\nurl = \"postgres://from-file/db\"\n",
    );

    set("FS3_DATABASE__URL", "postgres://from-env/db");
    let effective = config::load_effective_from(&dir).expect("the config should load");
    unset("FS3_DATABASE__URL");

    assert_eq!(effective.config.database.url, "postgres://from-env/db");
    // The provenance is the debuggability anchor: it must name the layer that
    // actually won, not the one the file suggests.
    assert_eq!(effective.layer("database"), Layer::Env);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_typo_in_an_override_stops_the_daemon_rather_than_being_ignored() {
    let _guard = env_lock();
    let dir = support::temp_dir("env-typo");

    set("FS3_DATABSE__URL", "postgres://typo/db");
    let error = config::load_effective_from(&dir).expect_err("a typo is not an override");
    unset("FS3_DATABSE__URL");

    let message = error.to_string();
    assert!(message.contains("FS3_DATABSE__URL"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unknown_file_section_warns_and_is_inert_instead_of_stopping_the_daemon() {
    let _guard = env_lock();
    let dir = support::temp_dir("file-future-section");
    write(&dir, "config.toml", "[future]\nactive = \"nonsense\"\n");

    let effective = config::load_effective_from(&dir).expect("a newer file section is tolerated");

    assert_eq!(effective.config, Config::default());
    let warnings: Vec<_> = effective.warnings().collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].key, "[future]");
    assert!(
        warnings[0]
            .message
            .contains("none of its settings take effect")
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn every_problem_in_the_file_is_reported_in_one_pass() {
    let _guard = env_lock();
    let dir = support::temp_dir("all-errors");
    write(
        &dir,
        "config.toml",
        r#"
        [daemon]
        url = ""

        [database]
        url = "mysql://wrong"

        [indexing]
        summary_min_lines = 0
        "#,
    );

    let error = config::load_effective_from(&dir).expect_err("three things are wrong");
    let message = error.to_string();

    for key in ["daemon.url", "database.url", "indexing.summary_min_lines"] {
        assert!(message.contains(key), "{key} missing from:\n{message}");
    }
    assert!(message.contains("config.toml"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn secrets_reach_the_environment_without_being_named_in_the_config_file() {
    let _guard = env_lock();
    let dir = support::temp_dir("secrets");
    write(
        &dir,
        "config.toml",
        "[providers.keyed]\nkind = \"openai\"\nmodel = \"m\"\n\
         api_key_env = \"FS3_TEST_SECRET_FROM_FILE\"\n\n[embedder]\nactive = \"keyed\"\n",
    );
    write(
        &dir,
        "secrets.env",
        "# a comment\nFS3_TEST_SECRET_FROM_FILE=sk-from-the-file\n",
    );

    unset("FS3_TEST_SECRET_FROM_FILE");
    let loaded = config::load_secrets_from(&dir).expect("the secrets file loads");

    assert!(loaded.present);
    assert_eq!(
        loaded.applied,
        vec!["FS3_TEST_SECRET_FROM_FILE".to_string()]
    );
    assert_eq!(
        std::env::var("FS3_TEST_SECRET_FROM_FILE").unwrap(),
        "sk-from-the-file"
    );

    // The config file names the VARIABLE, never the value — and the wiring
    // finds the value in the environment the secrets file just populated.
    let config = config::load_config_from(&dir).expect("the config loads");
    let selected = config
        .provider(config.selected(Port::Embedder, None))
        .unwrap();
    assert_eq!(selected.api_key_env(), Some("FS3_TEST_SECRET_FROM_FILE"));

    unset("FS3_TEST_SECRET_FROM_FILE");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_exported_variable_beats_the_secrets_file() {
    let _guard = env_lock();
    let dir = support::temp_dir("secrets-precedence");
    write(
        &dir,
        "secrets.env",
        "FS3_TEST_SECRET_PRECEDENCE=from-file\n",
    );

    set("FS3_TEST_SECRET_PRECEDENCE", "from-the-shell");
    let loaded = config::load_secrets_from(&dir).expect("the secrets file loads");

    assert_eq!(
        std::env::var("FS3_TEST_SECRET_PRECEDENCE").unwrap(),
        "from-the-shell",
        "an explicit KEY=… fs3-daemon must beat a file the user forgot about"
    );
    assert_eq!(
        loaded.already_set,
        vec!["FS3_TEST_SECRET_PRECEDENCE".to_string()]
    );
    assert!(loaded.applied.is_empty());

    unset("FS3_TEST_SECRET_PRECEDENCE");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_secrets_value_never_appears_in_what_the_loader_reports() {
    let _guard = env_lock();
    let dir = support::temp_dir("secrets-quiet");
    write(&dir, "secrets.env", "FS3_TEST_QUIET_KEY=sk-super-secret\n");

    unset("FS3_TEST_QUIET_KEY");
    let loaded = config::load_secrets_from(&dir).expect("the secrets file loads");

    // `SecretsLoaded` is what the daemon logs at startup. If a value can be
    // reached through it, it can be logged by accident.
    let rendered = format!("{loaded:?}");
    assert!(!rendered.contains("sk-super-secret"), "{rendered}");
    assert!(rendered.contains("FS3_TEST_QUIET_KEY"), "{rendered}");

    unset("FS3_TEST_QUIET_KEY");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_broken_secrets_file_is_refused_by_line_number() {
    let _guard = env_lock();
    let dir = support::temp_dir("secrets-broken");
    write(&dir, "secrets.env", "GOOD=1\nthis-is-not-a-pair\n");

    let error = config::load_secrets_from(&dir).expect_err("line 2 is not a pair");
    let message = error.to_string();
    assert!(message.contains("secrets.env:2"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_missing_secrets_file_is_the_normal_case() {
    let _guard = env_lock();
    let dir = support::temp_dir("no-secrets");
    let loaded = config::load_secrets_from(&dir).expect("absent is not an error");
    assert!(!loaded.present);
    assert!(loaded.applied.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_env_override_wins_over_the_home_directory() {
    let _guard = env_lock();
    let expected = std::env::temp_dir().join("fs3-config-discovery");

    set(
        CONFIG_DIR_ENV,
        expected.to_str().expect("a utf-8 temp path"),
    );
    assert_eq!(config::config_dir().unwrap(), expected);

    unset(CONFIG_DIR_ENV);
    let fallback = config::config_dir().expect("HOME is set in a normal test environment");
    assert!(
        fallback.ends_with(".config/flowspace3"),
        "PRD req 28 puts config in ~/.config/flowspace3, got {}",
        fallback.display()
    );
}

#[test]
fn an_override_can_select_an_instance_with_no_file_at_all() {
    let _guard = env_lock();
    let dir = support::temp_dir("env-provider");

    set("FS3_PROVIDERS__FAKE__KIND", "openai");
    set("FS3_PROVIDERS__FAKE__MODEL", "text-embedding-3-small");
    let effective = config::load_effective_from(&dir).expect("env alone is a valid config");
    unset("FS3_PROVIDERS__FAKE__KIND");
    unset("FS3_PROVIDERS__FAKE__MODEL");

    assert_eq!(
        effective.config.provider("fake").unwrap().kind(),
        "openai",
        "an override may reshape a registry instance without a file"
    );
    assert_eq!(effective.layer("providers"), Layer::Env);
    assert_eq!(effective.layer("daemon"), Layer::Defaults);

    std::fs::remove_dir_all(&dir).ok();
}
