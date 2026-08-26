//! `flowspace3 config show` — the effective configuration, and where each part
//! of it came from.
//!
//! This is the debuggability anchor for the whole layered scheme: when the
//! daemon is not doing what a config file says, this prints what fs3 actually
//! resolved and which layer won. Secrets are never printed — the database
//! password is masked and a key variable is reported as *set or not*, never by
//! value.

use std::path::Path;

use fs3_core::{Effective, ProviderConfig, SECTIONS};

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

    out.push_str("#\n# secrets are never printed:\n");
    for (section, provider) in [
        ("embedder", &effective.config.embedder),
        ("summarizer", &effective.config.summarizer),
    ] {
        match provider.api_key_env() {
            None => out.push_str(&format!(
                "#   {section}: no key needed (provider = \"fake\")\n"
            )),
            Some(variable) => out.push_str(&format!(
                "#   {section}.api_key_env = {variable} ({})\n",
                if std::env::var_os(variable).is_some() {
                    "set"
                } else {
                    "NOT SET — the daemon will refuse to start"
                }
            )),
        }
    }
    out.push('\n');

    // `redacted()` masks the database password; serializing the redacted copy
    // means a field added later cannot leak by being forgotten here.
    out.push_str(
        &toml::to_string_pretty(&effective.config.redacted())
            .expect("a Config always serializes to TOML"),
    );
    out
}

/// Whether any provider arm needs a key variable that is not set.
///
/// `config show` reports it; a future `doctor` can act on it.
#[must_use]
pub fn missing_key_variables(effective: &Effective) -> Vec<String> {
    [&effective.config.embedder, &effective.config.summarizer]
        .into_iter()
        .filter_map(ProviderConfig::api_key_env)
        .filter(|variable| std::env::var_os(variable).is_none())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::{Config, Layer, REDACTED};

    fn effective() -> Effective {
        let mut layers = std::collections::BTreeMap::new();
        layers.insert("daemon".to_string(), Layer::File);
        layers.insert("database".to_string(), Layer::Env);
        Effective {
            config: Config::default(),
            layers,
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
    fn the_fake_provider_is_reported_as_needing_no_key() {
        let rendered = render(&effective(), Path::new("/tmp/fs3"), true, false);
        assert!(rendered.contains("embedder: no key needed"), "{rendered}");
        assert!(missing_key_variables(&effective()).is_empty());
    }

    #[test]
    fn a_missing_key_variable_is_called_out_by_name() {
        let mut effective = effective();
        effective.config.embedder = ProviderConfig::OpenAi {
            model: "text-embedding-3-small".into(),
            api_base: None,
            api_key_env: "FS3_TEST_DEFINITELY_UNSET_KEY".into(),
        };

        let rendered = render(&effective, Path::new("/tmp/fs3"), true, false);
        assert!(
            rendered.contains("embedder.api_key_env = FS3_TEST_DEFINITELY_UNSET_KEY (NOT SET"),
            "{rendered}"
        );
        assert_eq!(
            missing_key_variables(&effective),
            vec!["FS3_TEST_DEFINITELY_UNSET_KEY".to_string()]
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
