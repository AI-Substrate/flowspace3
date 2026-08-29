//! Offline control-plane commands for GitHub Copilot authentication and model
//! discovery. They do not require a running fs3 daemon.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs3_core::envelope::Envelope;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct LoginReport {
    pub provider: &'static str,
    pub state: &'static str,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_file: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelsReport {
    pub provider: String,
    pub models: Vec<fs3_daemon::github_copilot::GitHubCopilotModel>,
    pub filter: &'static str,
    pub omitted_non_chat: usize,
}

pub async fn login(config_dir: &Path) -> Result<Envelope<LoginReport>> {
    if let Ok(existing) = fs3_daemon::github_copilot::GitHubCopilotCredential::discover() {
        return Ok(Envelope::ok(
            "login github-copilot",
            LoginReport {
                provider: "github-copilot",
                state: "logged_in",
                source: existing.source().label(),
                credential_file: None,
            },
        )
        .with_next_action("configure `[providers.<name>] kind = \"github_copilot\"`, then run `flowspace3 models <name>`"));
    }

    let device = fs3_daemon::github_copilot::start_device_login().await?;
    eprintln!(
        "GitHub Copilot login: open {} and enter code {}",
        device.verification_uri(),
        device.user_code()
    );
    let credential = fs3_daemon::github_copilot::finish_device_login(device).await?;
    let path = persist(config_dir, &credential.secret_env_value()?)?;
    Ok(Envelope::ok(
        "login github-copilot",
        LoginReport {
            provider: "github-copilot",
            state: "logged_in",
            source: "flowspace3 login",
            credential_file: Some(path.display().to_string()),
        },
    )
    .with_next_action("configure `[providers.<name>] kind = \"github_copilot\"`, then run `flowspace3 models <name>`"))
}

pub async fn models(provider: &str, config_dir: &Path) -> Result<Envelope<ModelsReport>> {
    let effective = crate::settings::load_effective_from(config_dir)?;
    let instance = effective
        .config
        .provider(provider)
        .with_context(|| format!("`{provider}` is not a configured provider instance"))?;
    if instance.kind() != "github_copilot" {
        anyhow::bail!(
            "provider instance `{provider}` is kind = {:?}, not github_copilot",
            instance.kind()
        );
    }
    let credential = fs3_daemon::github_copilot::GitHubCopilotCredential::discover()
        .context("GitHub Copilot is not logged in; run `flowspace3 login github-copilot`")?;
    let listing = fs3_daemon::github_copilot::list_models(&credential).await?;
    Ok(Envelope::ok(
        "models",
        ModelsReport {
            provider: provider.to_string(),
            models: listing.models,
            filter: listing.filter,
            omitted_non_chat: listing.omitted_non_chat,
        },
    )
    .with_next_action(format!(
        "set `model = \"<id>\"` under `[providers.{provider}]`, then select it in `[agent]`, `[summarizer]`, or `[embedder]`"
    )))
}

fn persist(config_dir: &Path, encoded: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating {}", config_dir.display()))?;
    let path = crate::settings::secrets_path(config_dir);
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    let prefix = format!("{}=", fs3_daemon::github_copilot::TOKEN_ENV);
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|line| !line.trim_start().starts_with(&prefix))
        .map(ToOwned::to_owned)
        .collect();
    lines.push(format!("{prefix}{encoded}"));
    let mut output = lines.join("\n");
    output.push('\n');

    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
    };
    #[cfg(not(unix))]
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path);

    let mut file = file.with_context(|| format!("opening {}", path.display()))?;
    file.write_all(output.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("setting mode 0600 on {}", path.display()))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_replaces_only_the_copilot_secret_and_sets_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::settings::secrets_path(dir.path());
        std::fs::write(&path, "OTHER=value\nCOPILOT_GITHUB_TOKEN=old\n").unwrap();
        persist(dir.path(), r#"{"token":"new"}"#).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("OTHER=value"));
        assert!(!text.contains("=old"));
        assert!(text.contains("COPILOT_GITHUB_TOKEN={\"token\":\"new\"}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
