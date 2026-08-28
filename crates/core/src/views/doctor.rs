//! What `doctor` answers with.

use std::time::Instant;

use serde::{Deserialize, Serialize};

/// One step of the walk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// `engine`, `stack`, `database`, `schema`, `daemon`, `providers`, …
    pub check: String,
    /// What the reader should DO about this row. The vocabulary is closed, and
    /// each word is a promise:
    ///
    /// | outcome | meaning | degrades the verdict? |
    /// |---|---|---|
    /// | `ok` | already fine | no |
    /// | `repaired` | was broken; doctor fixed it | no |
    /// | `info` | reported for awareness; nothing is wrong | **no** |
    /// | `warn` | working, but not as it should be; decide something | yes |
    /// | `down` | not running; start something | yes |
    ///
    /// `info` exists so a row can be *reported* without claiming the stack is
    /// unhealthy. Without it the only way to surface a finding was `warn`,
    /// which degrades — and a purely informational row degrading the whole
    /// verdict is louder than it means to be, which is its own kind of
    /// misleading.
    pub outcome: String,
    /// What doctor found.
    pub found: String,
    /// What doctor did about it, or what you should do — absent when there was
    /// nothing to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// This row's contribution to the envelope's `next_action`, when it is the
    /// most important unmet thing.
    ///
    /// Carried by the ROW rather than computed from a chain of check names, so
    /// a new row supplies its own steer without editing the steering logic —
    /// and so the steer can never drift from the finding that produced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steer: Option<String>,
    /// How long the step took.
    pub elapsed_ms: u128,
}

impl Step {
    // The constructors are public because `Step` is a public struct with
    // public fields — anyone can build one with a literal, so a private
    // constructor bought nothing and only made another module reach for the
    // literal and miss a field default.

    /// Already fine.
    pub fn ok(check: &str, found: impl Into<String>, started: Instant) -> Self {
        Step {
            check: check.to_string(),
            outcome: "ok".to_string(),
            found: found.into(),
            action: None,
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Found working but not as it should be — a finding, not a failure.
    ///
    /// Distinct from `down` because the subject is not absent, it is
    /// misconfigured or running on a stand-in, and the reader's next move is
    /// different: `down` means start something, `warn` means decide something.
    pub fn warn(
        check: &str,
        found: impl Into<String>,
        action: impl Into<String>,
        started: Instant,
    ) -> Self {
        Step {
            check: check.to_string(),
            outcome: "warn".to_string(),
            found: found.into(),
            action: Some(action.into()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Reported for awareness. Nothing is wrong and the verdict is untouched.
    ///
    /// For rows that inform rather than diagnose — a thing the reader may want
    /// to act on, where not having acted is not a fault. Use `warn` when
    /// something is genuinely not as it should be.
    pub fn info(
        check: &str,
        found: impl Into<String>,
        action: impl Into<String>,
        started: Instant,
    ) -> Self {
        Step {
            check: check.to_string(),
            outcome: "info".to_string(),
            found: found.into(),
            action: Some(action.into()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Attach this row's contribution to the envelope's `next_action`.
    #[must_use]
    pub fn with_steer(mut self, steer: impl Into<String>) -> Self {
        self.steer = Some(steer.into());
        self
    }

    /// Whether this row asks anything of the reader.
    ///
    /// `ok` and `repaired` do not: one was already fine and the other doctor
    /// handled. Everything else is a row the reader may need to act on, which
    /// is what makes it eligible to steer.
    #[must_use]
    pub fn asks_something(&self) -> bool {
        !matches!(self.outcome.as_str(), "ok" | "repaired")
    }

    /// Whether this row means the stack is not fully up.
    ///
    /// `info` deliberately does not: it reports, it does not diagnose.
    #[must_use]
    pub fn degrades(&self) -> bool {
        matches!(self.outcome.as_str(), "warn" | "down")
    }

    /// Found not running, and deliberately not started.
    pub fn down(check: &str, found: impl Into<String>, started: Instant) -> Self {
        Step {
            check: check.to_string(),
            outcome: "down".to_string(),
            found: found.into(),
            action: Some("not started — run `flowspace3 daemon &`".to_string()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    /// Found broken, and fixed.
    pub fn repaired(
        check: &str,
        found: impl Into<String>,
        action: impl Into<String>,
        started: Instant,
    ) -> Self {
        Step {
            check: check.to_string(),
            outcome: "repaired".to_string(),
            found: found.into(),
            action: Some(action.into()),
            steer: None,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }
}

/// What `flowspace3 doctor` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Every step, in dependency order.
    pub steps: Vec<Step>,
    /// Whether the STORE is usable now.
    pub healthy: bool,
    /// The whole stack's verdict: `ok`, or `degraded` when something doctor
    /// cannot repair for you is not running.
    ///
    /// Separate from `healthy` because they answer different questions, and
    /// conflating them is what made doctor say a plain "ok" on a machine with
    /// no daemon running (Jordan, live, 2026-08-26). The store really was fine;
    /// the stack was not usable. `ok: true` on the envelope stays either way —
    /// the COMMAND succeeded, and it is the subject it reports on that is
    /// degraded.
    pub verdict: String,
}

impl DoctorReport {
    /// Everything doctor checked is up.
    pub const OK: &'static str = "ok";
    /// Doctor ran fine; something it checked is not up.
    pub const DEGRADED: &'static str = "degraded";
}
