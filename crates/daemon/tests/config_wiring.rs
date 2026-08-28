//! The composition root is the entire IoC container, so its resolution of
//! *names to instances* is what these tests exercise: a config file goes in, a
//! wired provider comes out.

use fs3_core::Port;
use fs3_daemon::{AppState, config};

mod support;

/// dw-0009: a temp config dir whose ports name the offline `fake` instance
/// parses and wires with no keys.
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
        active = "fake"

        [summarizer]
        active = "fake"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config should load");
    assert_eq!(config.selected(Port::Embedder, None), "fake");

    let state = AppState::from_config(config).expect("the fake arms wire with no keys");
    assert_eq!(state.active_kind(Port::Summarizer), "fake");

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
    assert_eq!(config.selected(Port::Embedder, None), "fake");
    std::fs::remove_dir_all(&dir).ok();
}

/// A malformed file must not silently degrade to defaults — that hides a typo
/// behind a working daemon.
#[test]
fn a_malformed_config_file_is_refused_loudly() {
    let dir = support::temp_dir("bad-config");
    std::fs::write(
        dir.join("config.toml"),
        "[providers.mine]\nkind = \"nope\"\n",
    )
    .expect("writing the fixture config");

    let error = config::load_config_from(&dir).expect_err("an unknown kind is not a default");
    let message = error.to_string();
    assert!(message.contains("config.toml"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

/// Selecting an instance that is not in the registry must name the instances
/// that *are* — the whole point of a registry is that the names are knowable.
#[test]
fn an_unknown_instance_name_lists_the_configured_ones() {
    let dir = support::temp_dir("unknown-instance");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [providers.small]
        kind = "openai"
        model = "text-embedding-3-small"

        [embedder]
        active = "smal"
        "#,
    )
    .expect("writing the fixture config");

    let error = config::load_config_from(&dir).expect_err("`smal` is not configured");
    let message = error.to_string();
    assert!(message.contains("embedder.active"), "{message}");
    assert!(
        message.contains("configured providers are: fake, small"),
        "{message}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Selecting an OpenAI instance without a key must fail at wiring time with a
/// message that names the instance, the variable, and the offline escape hatch
/// — not at the first embedding, hours into an index.
#[test]
fn the_openai_arm_fails_fast_and_names_the_missing_key() {
    let dir = support::temp_dir("openai-config");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [providers.small]
        kind = "openai"
        model = "text-embedding-3-small"
        api_key_env = "FS3_TEST_DEFINITELY_UNSET_KEY"

        [embedder]
        active = "small"
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
    assert!(message.contains("small"), "{message}");
    assert!(message.contains("fake"), "{message}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn openai_compat_missing_key_is_a_config_answer_naming_secrets_file() {
    let dir = support::temp_dir("openai-compat-missing-key");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [providers.openrouter]
        kind = "openai_compat"
        base_url = "https://openrouter.ai/api/v1"
        model = "z-ai/glm-5.3-flash"
        api_key_env = "FS3_TEST_OPENROUTER_KEY_NOT_SET"

        [agent]
        active = "openrouter"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config shape is valid");
    let message = format!(
        "{:#}",
        AppState::from_config(config).expect_err("wiring must resolve the named key")
    );
    assert!(
        message.contains("FS3_TEST_OPENROUTER_KEY_NOT_SET"),
        "{message}"
    );
    assert!(message.contains("secrets.env"), "{message}");
    assert!(message.contains("config.toml"), "{message}");
    assert!(!message.contains("Bearer"), "{message}");
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn mixed_surfaces_wire_without_cross_contamination() {
    const KEY_ENV: &str = "FS3_TEST_MIXED_SURFACE_PROVIDER_KEY";
    // SAFETY: this test owns a uniquely named variable and removes it before
    // returning; no other test reads this name.
    unsafe { std::env::set_var(KEY_ENV, "offline-fixture-key") };
    let dir = support::temp_dir("mixed-surface-providers");
    std::fs::write(
        dir.join("config.toml"),
        format!(
            r#"
            [providers.azure-embed]
            kind = "azure_openai"
            endpoint = "https://example.openai.azure.com"
            deployment = "text-embedding-3-small"
            api_version = "2024-02-01"
            api_key_env = "{KEY_ENV}"
            dimensions = 1024

            [providers.azure-summary]
            kind = "azure_openai"
            endpoint = "https://example.openai.azure.com"
            deployment = "gpt-4o"
            api_version = "2024-12-01-preview"
            api_key_env = "{KEY_ENV}"

            [providers.openrouter-glm]
            kind = "openai_compat"
            base_url = "https://openrouter.ai/api/v1"
            model = "z-ai/glm-5.3-flash"
            api_key_env = "{KEY_ENV}"

            [embedder]
            active = "azure-embed"
            [summarizer]
            active = "azure-summary"
            [agent]
            active = "openrouter-glm"
            "#
        ),
    )
    .expect("writing the mixed fixture");

    let config = config::load_config_from(&dir).expect("the mixed config is valid");
    let state = AppState::from_config(config).expect("all three selected surfaces wire");
    assert_eq!(state.active_kind(Port::Embedder), "azure_openai");
    assert_eq!(state.active_kind(Port::Summarizer), "azure_openai");
    assert_eq!(state.active_kind(Port::Agent), "openai_compat");
    assert!(state.embedder.key().starts_with("text-embedding-3-small@"));
    assert!(state.summarizer.key().starts_with("gpt-4o@"));
    assert_eq!(state.agent.key(), "z-ai/glm-5.3-flash");

    unsafe { std::env::remove_var(KEY_ENV) };
    std::fs::remove_dir_all(&dir).ok();
}

/// An instance nobody selects is never constructed, so declaring a provider you
/// have no key for must not stop the daemon starting.
#[tokio::test]
async fn an_unselected_instance_costs_nothing_at_startup() {
    let dir = support::temp_dir("spare-instance");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [providers.spare]
        kind = "openai"
        model = "gpt-4o"
        api_key_env = "FS3_TEST_DEFINITELY_UNSET_KEY"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config is valid");
    let state = AppState::from_config(config).expect("nothing references `spare`");
    assert_eq!(state.active_kind(Port::Embedder), "fake");

    std::fs::remove_dir_all(&dir).ok();
}

/// A repo that names a different instance gets a different port object, and
/// every other repo keeps the default.
#[tokio::test]
async fn a_repo_override_resolves_to_its_own_instance() {
    let dir = support::temp_dir("repo-override");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [providers.other]
        kind = "fake"

        [repos."github.com/acme/thing"]
        summarizer = "other"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config is valid");
    let state = AppState::from_config(config).expect("both instances are fakes");

    let overridden = state.summarizer_for("github.com/acme/thing");
    let default = state.summarizer_for("github.com/acme/other-thing");

    assert!(
        !std::sync::Arc::ptr_eq(overridden, default),
        "the overriding repo must get its own instance"
    );
    assert!(
        std::sync::Arc::ptr_eq(default, &state.summarizer),
        "every other repo keeps the active default"
    );

    // The embedder was never overridden, so both repos share one object.
    assert!(std::sync::Arc::ptr_eq(
        state.embedder_for("github.com/acme/thing"),
        &state.embedder
    ));

    std::fs::remove_dir_all(&dir).ok();
}

/// Two repos naming the same instance share one client rather than building it
/// twice.
#[tokio::test]
async fn repos_naming_the_same_instance_share_one_object() {
    let dir = support::temp_dir("shared-instance");
    std::fs::write(
        dir.join("config.toml"),
        r#"
        [providers.other]
        kind = "fake"

        [repos."github.com/acme/one"]
        embedder = "other"

        [repos."github.com/acme/two"]
        embedder = "other"
        "#,
    )
    .expect("writing the fixture config");

    let config = config::load_config_from(&dir).expect("the config is valid");
    let state = AppState::from_config(config).expect("the fakes wire with no keys");

    assert!(std::sync::Arc::ptr_eq(
        state.embedder_for("github.com/acme/one"),
        state.embedder_for("github.com/acme/two")
    ));

    std::fs::remove_dir_all(&dir).ok();
}
