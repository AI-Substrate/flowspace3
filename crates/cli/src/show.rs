//! `flowspace3 config show` — the effective configuration, and where each part
//! of it came from.
//!
//! This is the debuggability anchor for the whole layered scheme: when the
//! daemon is not doing what a config file says, this prints what fs3 actually
//! resolved, which layer won, and which registry instance each port ended up
//! with. Secrets are never printed — the database password is masked and a key
//! variable is reported as *set or not*, never by value.

use std::path::Path;

use fs3_core::{Effective, Port, SECTIONS};

/// Render the effective configuration as annotated TOML.
///
/// `file_present` and `secrets_present` say whether each file exists; the
/// secrets file's *contents* are deliberately not an input, because nothing in
/// this function may see a value.
#[must_use]
pub fn render(
    effective: &Effective,
    dir: &Path,
    file_present: bool,
    secrets_present: bool,
) -> String {
    let config = &effective.config;
    let mut out = String::new();

    out.push_str("# effective fs3 configuration\n");
    out.push_str(&format!(
        "# config file: {} ({})\n",
        crate::settings::config_path(dir).display(),
        if file_present {
            "present"
        } else {
            "absent — running on defaults"
        }
    ));
    out.push_str(&format!(
        "# secrets:     {} ({})\n",
        crate::settings::secrets_path(dir).display(),
        if secrets_present { "present" } else { "absent" }
    ));
    out.push_str("# layers: defaults < config.toml < FS3_* environment\n#\n");

    let width = SECTIONS.iter().map(|s| s.len()).max().unwrap_or(0);
    for section in SECTIONS {
        out.push_str(&format!(
            "# [{section}]{:width$} from {}\n",
            "",
            effective.layer(section),
            width = width - section.len()
        ));
    }
    // Unknown sections are absent from the resolved TOML below. Put their
    // warning beside the known-section provenance list — the exact place a
    // reader scans to confirm that a recent edit took effect.
    for warning in effective.warnings() {
        out.push_str(&format!(
            "# {} WARNING — {}\n",
            warning.key, warning.message
        ));
    }

    out.push_str("#\n# resolved providers (secrets are never printed):\n");
    for port in Port::ALL {
        let name = config.selected(port, None);
        out.push_str(&format!(
            "#   {port} -> {name}{}\n",
            key_status(effective, name)
        ));
    }
    for (repo, selection) in &config.repos {
        for port in Port::ALL {
            let Some(name) = selection.get(port) else {
                continue;
            };
            out.push_str(&format!(
                "#   {port} for {repo} -> {name}{}\n",
                key_status(effective, name)
            ));
        }
    }
    out.push('\n');

    // `redacted()` masks the database password; serializing the redacted copy
    // means a field added later cannot leak by being forgotten here.
    out.push_str(
        &toml::to_string_pretty(&config.redacted()).expect("a Config always serializes to TOML"),
    );
    out
}

/// ` (OPENAI_API_KEY: set)` — or nothing, for an instance that needs no key.
fn key_status(effective: &Effective, name: &str) -> String {
    let Ok(instance) = effective.config.provider(name) else {
        return " — NOT CONFIGURED".to_string();
    };
    match instance.api_key_env() {
        None => String::new(),
        Some(variable) => format!(
            " ({variable}: {})",
            if std::env::var_os(variable).is_some() {
                "set"
            } else {
                "NOT SET — the daemon will refuse to start"
            }
        ),
    }
}

/// Key variables that a *referenced* instance needs and the environment does
/// not have.
///
/// `config show` reports them; a future `doctor` can act on them. An instance
/// nobody selects is not reported: declaring a provider you have no key for is
/// legal and costs nothing.
#[must_use]
pub fn missing_key_variables(effective: &Effective) -> Vec<String> {
    let config = &effective.config;
    let mut missing = Vec::new();
    for port in Port::ALL {
        for name in config.referenced_providers(port) {
            let Ok(instance) = config.provider(name) else {
                continue;
            };
            let Some(variable) = instance.api_key_env() else {
                continue;
            };
            if std::env::var_os(variable).is_none() && !missing.iter().any(|m| m == variable) {
                missing.push(variable.to_string());
            }
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::{Config, Layer, ProviderInstance, REDACTED, RepoSelection};

    fn effective() -> Effective {
        let mut layers = std::collections::BTreeMap::new();
        layers.insert("daemon".to_string(), Layer::File);
        layers.insert("database".to_string(), Layer::Env);
        Effective {
            config: Config::default(),
            layers,
            has_file: true,
        }
    }

    #[test]
    fn the_database_password_is_never_printed() {
        let rendered = render(&effective(), Path::new("/tmp/fs3"), true, false);
        assert!(!rendered.contains("flowspace3:flowspace3@"), "{rendered}");
        assert!(rendered.contains(REDACTED), "{rendered}");
        // The parts you actually need to debug survive.
        assert!(rendered.contains("127.0.0.1:5433/flowspace3"), "{rendered}");
    }

    #[test]
    fn every_section_reports_the_layer_it_came_from() {
        let rendered = render(&effective(), Path::new("/tmp/fs3"), true, false);
        assert!(
            rendered.contains("# [daemon]     from config.toml"),
            "{rendered}"
        );
        assert!(
            rendered.contains("# [database]   from FS3_* environment"),
            "{rendered}"
        );
        assert!(
            rendered.contains("# [scan]       from defaults"),
            "{rendered}"
        );
    }

    #[test]
    fn an_ignored_section_is_named_in_the_header() {
        let effective = fs3_core::resolve(fs3_core::Sources {
            file_label: "/tmp/fs3/config.toml",
            file_text: Some("[typo]\nactive = \"nonsense\"\n\n[embedder]\nactive = \"fake\"\n"),
            env: &[],
        })
        .unwrap();

        let rendered = render(&effective, Path::new("/tmp/fs3"), true, false);
        assert!(
            rendered.contains(
                "# [typo] WARNING — unknown top-level section was ignored by this binary; \
                 none of its settings take effect"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn each_port_reports_the_instance_it_resolved_to() {
        let rendered = render(&effective(), Path::new("/tmp/fs3"), true, false);
        assert!(rendered.contains("#   embedder -> fake"), "{rendered}");
        assert!(rendered.contains("#   summarizer -> fake"), "{rendered}");
        assert!(missing_key_variables(&effective()).is_empty());
    }

    #[test]
    fn a_repo_override_is_shown_next_to_the_default() {
        let mut effective = effective();
        effective.config.providers.insert(
            "big".to_string(),
            ProviderInstance::OpenAi {
                model: "gpt-4o".into(),
                api_base: None,
                api_key_env: "FS3_TEST_DEFINITELY_UNSET_KEY".into(),
            },
        );
        effective.config.repos.insert(
            "github.com/acme/thing".to_string(),
            RepoSelection {
                embedder: None,
                summarizer: Some("big".to_string()),
                ..Default::default()
            },
        );

        let rendered = render(&effective, Path::new("/tmp/fs3"), true, false);
        assert!(
            rendered.contains("#   summarizer for github.com/acme/thing -> big"),
            "{rendered}"
        );
        assert!(
            rendered.contains("FS3_TEST_DEFINITELY_UNSET_KEY: NOT SET"),
            "{rendered}"
        );
        assert_eq!(
            missing_key_variables(&effective),
            vec!["FS3_TEST_DEFINITELY_UNSET_KEY".to_string()],
            "a referenced instance's missing key is reportable"
        );
    }

    #[test]
    fn an_unreferenced_instance_never_asks_for_a_key() {
        let mut effective = effective();
        effective.config.providers.insert(
            "spare".to_string(),
            ProviderInstance::OpenAi {
                model: "gpt-4o".into(),
                api_base: None,
                api_key_env: "FS3_TEST_DEFINITELY_UNSET_KEY".into(),
            },
        );

        assert!(
            missing_key_variables(&effective).is_empty(),
            "declaring an instance must not cost an API key"
        );
    }

    #[test]
    fn the_paths_are_named_so_the_user_knows_what_to_edit() {
        let rendered = render(&effective(), Path::new("/tmp/fs3"), false, true);
        assert!(
            rendered.contains("/tmp/fs3/config.toml (absent"),
            "{rendered}"
        );
        assert!(
            rendered.contains("/tmp/fs3/secrets.env (present)"),
            "{rendered}"
        );
    }
}
