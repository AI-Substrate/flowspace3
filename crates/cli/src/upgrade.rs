//! `flowspace3 doctor upgrade` — the force-it-now path (PRD req 54).
//!
//! The same engine the daemon's reconcile loop runs, driven by a human instead
//! of by a clock. Deliberately the same code and not a second implementation:
//! the manual path is the one people reach for when the automatic one did not
//! work, so a copy that behaved even slightly differently would be the worst
//! possible place for drift.
//!
//! Two things it does that the loop does not:
//!
//! * It **ignores the interval**. The daemon's check is rate-limited against
//!   `update_state.last_checked_at` so a fleet does not hammer GitHub; a person
//!   typing the command has already decided it is time.
//! * It reports its outcome as an ENVELOPE rather than a log line, because the
//!   person who ran it is standing there waiting for the answer.
//!
//! It writes the same state row the loop does, so a manual upgrade clears the
//! queue's "update not possible" message the same way an automatic one would,
//! and the restart steer it raises is the identical message.

use fs3_core::envelope::{Envelope, Failure};
use fs3_core::{Config, catalog};
use fs3_daemon::update::{Outcome, Updater};
use serde::{Deserialize, Serialize};

/// What `flowspace3 doctor upgrade` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeReport {
    /// The version this binary is.
    pub running: String,
    /// The newest version published, when the probe got that far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    /// `current`, `installed`, or `blocked`.
    pub state: String,
    /// Where the swap landed, or would have.
    pub install_path: String,
    /// Why nothing was installed, when nothing was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Probe, download, verify and swap — now, whatever the interval says.
///
/// Never returns a failed envelope for a refusal that is a fact about this
/// machine (nothing newer, an unwritable path, a bad checksum): those are
/// answers, and an answer belongs in `data` with a `next_action`. Only a probe
/// that could not complete is an error.
pub async fn upgrade(config: &Config) -> Envelope<UpgradeReport> {
    const COMMAND: &str = "doctor upgrade";
    let running = env!("CARGO_PKG_VERSION");

    let updater = match Updater::new(running) {
        Ok(updater) => updater,
        Err(error) => {
            return Envelope::failed(
                COMMAND,
                Failure::new(&catalog::UPDATE_NO_INSTALL_PATH, error.to_string()),
            );
        }
    };
    let install_path = updater.install_path().display().to_string();

    let outcome = match updater.run_once().await {
        Ok(outcome) => outcome,
        Err(error) => {
            return Envelope::failed(
                COMMAND,
                Failure::new(&catalog::UPDATE_UNREACHABLE, error.to_string()),
            );
        }
    };

    // Whatever a manual run concludes is what the daemon's next pass would have
    // concluded, so it lands in the same row and clears the same messages. A
    // store that is not up is not a reason to fail a successful swap, which is
    // why this is best-effort and reported rather than propagated.
    let recorded = record(config, &outcome, &install_path).await;

    let (report, next) = match outcome {
        Outcome::Current => (
            UpgradeReport {
                running: running.to_string(),
                latest: Some(running.to_string()),
                state: "current".to_string(),
                install_path,
                reason: None,
            },
            format!("nothing to do — {running} is the newest published release"),
        ),
        Outcome::Installed(version) => (
            UpgradeReport {
                running: running.to_string(),
                latest: Some(version.to_string()),
                state: "installed".to_string(),
                install_path: install_path.clone(),
                reason: None,
            },
            format!(
                "flowspace3 {version} is now at {install_path} — restart the fs3 daemon to run it"
            ),
        ),
        Outcome::Blocked { latest, reason } => (
            UpgradeReport {
                running: running.to_string(),
                latest: Some(latest.to_string()),
                state: "blocked".to_string(),
                install_path,
                reason: Some(reason.clone()),
            },
            format!(
                "{latest} could not be installed: {reason} — reinstall instead: `{}`",
                fs3_core::update::REINSTALL_COMMAND
            ),
        ),
    };

    let envelope = Envelope::ok(COMMAND, report).with_next_action(next);
    match recorded {
        Ok(()) => envelope,
        Err(error) => envelope.with_meta(serde_json::json!({
            "state_not_recorded": error,
        })),
    }
}

/// Write the outcome into THIS INSTALL's update state AND re-declare the
/// update source's messages for it.
///
/// Both halves, not just the first. The daemon's loop does the same two things
/// in one pass, but `doctor upgrade` is precisely the verb you reach for when
/// the daemon is DOWN — leaving the queue to be reconciled later would mean a
/// manual upgrade recorded a blocked install that nothing told the user about
/// until a daemon came back and its interval came round.
///
/// It re-declares rather than pushes, using the same
/// [`UpdateState::desired_messages`] the loop uses, so the two writers cannot
/// disagree and a manual upgrade that SUCCEEDS clears the message a previous
/// failure left behind.
///
/// Everything here is scoped to `install_path`, which is THIS binary's own
/// resolved path. That is the write half of the per-install fix: an unprivileged
/// user running `doctor upgrade` against `~/.local/bin` used to overwrite the
/// row describing root's `/usr/local/bin`, so root's daemon then advertised a
/// blocked update for a path root does not use.
///
/// [`UpdateState::desired_messages`]: fs3_core::update::UpdateState::desired_messages
async fn record(config: &Config, outcome: &Outcome, install_path: &str) -> Result<(), String> {
    let running = env!("CARGO_PKG_VERSION");
    let pool = fs3_store::connect(&config.database.url)
        .await
        .map_err(|error| error.to_string())?;

    let written = async {
        match outcome {
            Outcome::Current => fs3_store::record_clear(&pool, install_path).await?,
            Outcome::Installed(version) => {
                fs3_store::record_swapped(&pool, install_path, &version.to_string()).await?;
            }
            Outcome::Blocked { latest, reason } => {
                fs3_store::record_seen(&pool, install_path, &latest.to_string()).await?;
                fs3_store::record_blocked(&pool, install_path, reason).await?;
            }
        }

        // What is on disk, asked of the disk — the same reconciliation the
        // daemon does on every check. A manual upgrade is exactly when the
        // stored claim is most likely to be stale, and it is also the verb a
        // user reaches for after reinstalling by hand.
        let found = fs3_daemon::update::on_disk_version(std::path::Path::new(install_path));
        fs3_store::record_on_disk(&pool, install_path, found.as_deref()).await?;

        let state = fs3_store::update_state(&pool, install_path).await?;
        fs3_store::sync_messages(
            &pool,
            fs3_core::UPDATE_SOURCE,
            Some(install_path),
            &state.desired_messages(running),
        )
        .await
    }
    .await;

    pool.close().await;
    written.map_err(|error| error.to_string())
}
