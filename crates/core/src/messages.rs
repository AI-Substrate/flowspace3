//! The user messages queue's shape (PRD req 59).
//!
//! One place for "the daemon has something to tell the person driving". A
//! feature that has learned something a user must act on — a newer binary is
//! installed, the disk is filling, a provider is misconfigured — pushes a
//! [`UserMessage`] and stops there. It does not reach into the envelope, add a
//! field to a verb's payload, or invent a `meta` key that one command carries
//! and the rest do not. The queue is the only channel, so a consumer learns
//! the same news whatever it happened to be asking about.
//!
//! Jordan, 2026-08-27, on the auto-update steering that motivated it: the
//! message must be *actionable*, which is why [`UserMessage::next_action`] is
//! mandatory rather than optional. A message a user cannot act on is a log
//! line, and log lines belong in the log.
//!
//! # Identity, and why messages are level-triggered
//!
//! [`UserMessage::key`] is the message's identity, chosen by the producer and
//! stable across passes — `update:installed:0.3.1`, not a random id. A
//! reconcile loop that pushes the same key on every pass produces exactly one
//! message, which is what makes a producer safe to run every few seconds.
//!
//! There is deliberately no stored "clear condition" predicate. Clearing is
//! the producer's job and falls out of the same level-triggered shape: each
//! pass declares the messages its source *should* have right now, and anything
//! else under that source goes away. An update that succeeds stops declaring
//! "update pending", so the message clears without anything having to evaluate
//! a rule. The alternative — a condition language in the queue — is a rules
//! engine nobody asked for.
//!
//! Ack and expiry exist for the messages a producer cannot retract on its own:
//! a user dismissing a notice, and a notice that stops being true by the
//! calendar rather than by a state change.

use serde::{Deserialize, Serialize};

/// How loudly a message asks to be read.
///
/// Three levels, because the consumer's decision is three-way: mention it,
/// surface it, or treat the situation as broken. A fourth would be a shade of
/// one of these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Something good or neutral happened that the user should know about.
    #[default]
    Info,
    /// Something is degraded and will stay that way until the user acts.
    Warning,
    /// Something is broken; the feature this message is about is not working.
    Error,
}

impl Severity {
    /// The wire spelling, and the value stored in Postgres.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }

    /// Parse the wire spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Severity::Info),
            "warning" => Some(Severity::Warning),
            "error" => Some(Severity::Error),
            _ => None,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One thing the daemon needs to tell the user, carried by every envelope the
/// daemon answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMessage {
    /// The producer's stable identity for this message. Pushing the same key
    /// twice updates one row rather than making two.
    pub key: String,
    /// The feature that raised it — `update`, and whatever comes next. A
    /// producer owns every message under its own source and no others.
    pub source: String,
    /// How loudly it asks to be read.
    pub severity: Severity,
    /// What happened, in the user's terms.
    pub text: String,
    /// What to do about it. Mandatory: see the module note.
    pub next_action: String,
    /// When it was first raised, RFC 3339 in UTC. Set by the store, never by
    /// the producer — two daemons disagreeing about the clock would otherwise
    /// reorder each other's messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

impl UserMessage {
    /// A message from `source`, identified by `key`.
    pub fn new(
        key: impl Into<String>,
        source: impl Into<String>,
        severity: Severity,
        text: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            source: source.into(),
            severity,
            text: text.into(),
            next_action: next_action.into(),
            created: None,
        }
    }

    /// One line, for a terminal: `[warning] text — next_action`.
    #[must_use]
    pub fn render(&self) -> String {
        format!("[{}] {} — {}", self.severity, self.text, self.next_action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_round_trips_through_its_wire_spelling() {
        for severity in [Severity::Info, Severity::Warning, Severity::Error] {
            assert_eq!(Severity::parse(severity.as_str()), Some(severity));
        }
        assert_eq!(Severity::parse("catastrophe"), None);
    }

    #[test]
    fn a_message_serialises_with_snake_case_severity() {
        let message = UserMessage::new(
            "update:installed:0.3.1",
            "update",
            Severity::Warning,
            "flowspace3 0.3.1 is installed; this daemon is still running 0.3.0",
            "restart the daemon to pick it up",
        );

        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(json["severity"], "warning");
        assert_eq!(json["source"], "update");
        // Unset `created` is absent rather than null: a producer has not
        // claimed a time it does not know.
        assert!(json.get("created").is_none());
    }

    #[test]
    fn a_message_renders_the_action_a_user_must_take() {
        let message = UserMessage::new(
            "update:blocked",
            "update",
            Severity::Warning,
            "update not possible at /usr/local/bin/flowspace3: not writable",
            "run `flowspace3 doctor upgrade`",
        );

        assert_eq!(
            message.render(),
            "[warning] update not possible at /usr/local/bin/flowspace3: not writable \
             — run `flowspace3 doctor upgrade`"
        );
    }
}
