//! The composition root is the entire IoC container, so its `match` is what
//! these tests exercise: a config file goes in, a wired arm comes out.

use fs3_daemon::{AppState, config, wiring};

mod support;

/// dw-0009: a temp config dir with `provider = "fake"` parses and selects the
/// fake arm.
///
/// Async because [`AppState::from_config`] builds the connection pool, and a
/// sqlx pool needs a Tokio context to own its idle-connection reaper. The
/// daemon always has one; a test has to say so.
#[tokio::test]
async fn a_fake_config_directory_selects_the_fake_arms() {
    let dir = support::temp_dir("fake-config");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [embedder]
        provider = "fake"

        [summarizer]
        provider = "fake"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config should load");
    assert_eq!(wiring::describe(&config.embedder), "fake");

    let state = AppState::from_config(config).expect("the fake arms wire with no keys");
    assert_eq!(wiring::describe(&state.config.summarizer), "fake");

    // The wired port really is the fake, not merely named after it: the fake is
    // deterministic, and nothing else in the workspace is.
    let texts = vec!["fn main() {}".to_string()];
    let first = state.embedder.embed(&texts).await.unwrap();
    let second = state.embedder.embed(&texts).await.unwrap();
    assert_eq!(first, second);

    std::fs::remove_dir_all(&dir).ok();
}

/// A missing file means defaults, and the defaults are a working offline stack.
#[test]
fn a_missing_config_file_is_the_offline_default_not_an_error() {
    let dir = support::temp_dir("no-config");
    let config = config::load_config_from(&dir).expect("a missing file means defaults");
    assert_eq!(wiring::describe(&config.embedder), "fake");
    std::fs::remove_dir_all(&dir).ok();
}

/// A malformed file must not silently degrade to defaults — that hides a typo
/// behind a working daemon.
#[test]
fn a_malformed_config_file_is_refused_loudly() {
    let dir = support::temp_dir("bad-config");
    std::fs::write(dir.join("config.toml"), "[embedder]\nprovider = \"nope\"\n")
        .expect("writing the fixture config");

    let error = config::load_config_from(&dir).expect_err("an unknown provider is not a default");
    let message = error.to_string();
    assert!(message.contains("config.toml"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

/// Choosing the OpenAI arm without a key must fail at wiring time with a
/// message that names the variable and the offline escape hatch — not at the
/// first embedding, hours into an index.
#[test]
fn the_openai_arm_fails_fast_and_names_the_missing_key() {
    let dir = support::temp_dir("openai-config");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [embedder]
        provider = "openai"
        model = "text-embedding-3-small"
        api_key_env = "FS3_TEST_DEFINITELY_UNSET_KEY"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config itself is valid");
    let error = AppState::from_config(config).expect_err("no key is set");
    let message = format!("{error:#}");

    assert!(
        message.contains("FS3_TEST_DEFINITELY_UNSET_KEY"),
        "{message}"
    );
    assert!(message.contains("fake"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}
