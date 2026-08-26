//! `FS3_CONFIG_DIR` discovery.
//!
//! In its own test binary because it mutates process environment, which in
//! Rust 2024 is `unsafe` precisely because other threads may be reading it.
//! One test per binary means there are no other threads.

use fs3_core::CONFIG_DIR_ENV;
use fs3_daemon::config;

#[test]
fn the_env_override_wins_over_the_home_directory() {
    let expected = std::env::temp_dir().join("fs3-config-discovery");

    // SAFETY: this binary contains exactly one test, so nothing else is reading
    // the environment concurrently.
    unsafe {
        std::env::set_var(CONFIG_DIR_ENV, &expected);
    }
    assert_eq!(config::config_dir().unwrap(), expected);

    // SAFETY: as above.
    unsafe {
        std::env::remove_var(CONFIG_DIR_ENV);
    }
    let fallback = config::config_dir().expect("HOME is set in a normal test environment");
    assert!(
        fallback.ends_with(".config/flowspace3"),
        "PRD req 28 puts config in ~/.config/flowspace3, got {}",
        fallback.display()
    );
}
