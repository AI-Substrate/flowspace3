//! Schema skew: what it means when the binary and the database disagree about
//! which migrations exist, and what to say about it.
//!
//! Two directions, and only one of them is what people expect:
//!
//! * **The database is BEHIND the binary** — the ordinary case. It is fixable
//!   by migrating, which is exactly what boot and `flowspace3 doctor` do, so it
//!   is handled where it is found and never reaches this module.
//! * **The binary is OLDER than the database** — this module. Migrating cannot
//!   fix it, because there is nothing to migrate: the database already has
//!   migrations this binary has never heard of. The only fix is a newer binary.
//!
//! # Why this needed its own words
//!
//! Jordan hit the second case live on 2026-08-27 and got, twice over:
//!
//! ```text
//! applying store migrations to … — if the store is not running: docker compose up -d:
//! migrations failed: migration 8 was previously applied but is missing in the
//! resolved migrations
//! ```
//!
//! Three failures in one line. It is sqlx's sentence, not fs3's, so the reader
//! has to know what "resolved migrations" means. It steers at `docker compose
//! up -d`, which is wrong — the store is perfectly healthy. And it never says
//! the one thing that matters: **auto-migration ran and could not help**,
//! because the problem is the binary. His question was "why not just auto
//! migrate", which is precisely the question an error should have pre-empted.
//!
//! So the words live here, once, and boot, doctor and the user messages queue
//! all say the same thing.

use crate::messages::{Severity, UserMessage};

/// The source the schema producer files messages under (PRD req 59).
pub const SCHEMA_SOURCE: &str = "schema";

/// The installer of last resort, for a machine whose binary cannot be upgraded
/// in place.
pub const REINSTALL_COMMAND: &str = crate::update::REINSTALL_COMMAND;

/// A binary that is older than the database it is looking at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaSkew {
    /// The version of the binary doing the looking.
    pub binary_version: String,
    /// The newest migration this binary carries, if it carries any.
    pub bundled_highest: Option<i64>,
    /// Migration versions the DATABASE has applied that this binary does not
    /// know about, ascending. Empty means no skew.
    pub extra: Vec<i64>,
}

impl SchemaSkew {
    /// Whether the database is ahead at all.
    #[must_use]
    pub fn is_skewed(&self) -> bool {
        !self.extra.is_empty()
    }

    /// `0008-0009`-style summary of the migrations the database has and this
    /// binary does not.
    #[must_use]
    pub fn extra_summary(&self) -> String {
        match self.extra.as_slice() {
            [] => "none".to_string(),
            [one] => format!("{one:04}"),
            [first, .., last] => format!("{first:04}-{last:04}"),
        }
    }

    /// What is true, in one sentence, with the numbers on both sides.
    ///
    /// Names the case first. A reader who takes only the first clause away has
    /// still taken away the right thing.
    #[must_use]
    pub fn summary(&self) -> String {
        let bundled = self
            .bundled_highest
            .map_or_else(|| "none".to_string(), |version| format!("{version:04}"));

        // Spell the individual versions out only when the range is hiding
        // something. For one migration the range IS the version, and "0099
        // (0099)" reads like a bug in the message rather than a fact about the
        // database.
        let detail = if self.extra.len() > 1 {
            format!(
                " ({})",
                self.extra
                    .iter()
                    .map(|version| format!("{version:04}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            String::new()
        };

        format!(
            "this flowspace3 binary is OLDER than its database: the binary is {} and carries \
             migrations up to {bundled}, but the database has already applied {}{detail}, which \
             this binary has never heard of",
            self.binary_version,
            self.extra_summary(),
        )
    }

    /// What to do about it.
    ///
    /// It says migrating cannot help BEFORE it says what will, because "why not
    /// just auto migrate" is the question a reader arrives with, and an
    /// instruction that does not answer it gets argued with instead of
    /// followed.
    #[must_use]
    pub fn fix(&self) -> String {
        format!(
            "migrating cannot fix this — auto-migration already ran, and there is nothing to \
             apply: the database is ahead, not behind. The store is healthy; do NOT restart it. \
             Upgrade the binary instead: `flowspace3 doctor upgrade`, or reinstall: \
             `{REINSTALL_COMMAND}`"
        )
    }

    /// The whole thing, for a startup failure.
    #[must_use]
    pub fn explain(&self) -> String {
        format!("{}\n\n{}", self.summary(), self.fix())
    }

    /// What the queue should be saying about this right now (PRD req 59).
    ///
    /// Level-triggered like every other producer: a pass that finds no skew
    /// declares nothing, and the message retracts itself. That is the whole
    /// reason this is a queue producer rather than a one-shot write — the
    /// condition arrives *while a daemon is running* (a newer `doctor` or a
    /// colleague's daemon migrates the store out from under it) and it can
    /// disappear the same way.
    #[must_use]
    pub fn desired_messages(&self) -> Vec<UserMessage> {
        if !self.is_skewed() {
            return Vec::new();
        }

        vec![UserMessage::new(
            format!("schema:ahead:{}", self.extra_summary()),
            SCHEMA_SOURCE,
            // Error, not warning: unlike a pending update, this daemon is
            // writing to a schema it does not fully understand RIGHT NOW.
            Severity::Error,
            self.summary(),
            "upgrade this flowspace3 and restart the daemon — `flowspace3 doctor upgrade`; \
             migrating will not help, the database is ahead rather than behind",
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skewed() -> SchemaSkew {
        SchemaSkew {
            binary_version: "0.1.0".to_string(),
            bundled_highest: Some(7),
            extra: vec![8, 9],
        }
    }

    #[test]
    fn no_extra_migrations_is_no_skew_and_says_nothing() {
        let level = SchemaSkew {
            binary_version: "0.3.0".to_string(),
            bundled_highest: Some(9),
            extra: Vec::new(),
        };

        assert!(!level.is_skewed());
        assert!(level.desired_messages().is_empty());
    }

    /// The summary has to carry both sides. "Schema mismatch" alone sends the
    /// reader to the migration history to work out who is ahead.
    #[test]
    fn the_summary_names_the_case_and_both_sides() {
        let summary = skewed().summary();

        assert!(
            summary.starts_with("this flowspace3 binary is OLDER than its database"),
            "the case has to come first: {summary}"
        );
        assert!(summary.contains("0.1.0"), "the binary version: {summary}");
        assert!(
            summary.contains("0007"),
            "what the binary carries: {summary}"
        );
        assert!(summary.contains("0008"), "what the database has: {summary}");
        assert!(summary.contains("0009"));
    }

    /// The defect this whole module exists for: the old error steered at
    /// `docker compose up -d` when the store was perfectly healthy, and never
    /// said that migrating had already been tried.
    #[test]
    fn the_fix_refuses_the_two_wrong_answers_and_names_the_right_one() {
        let fix = skewed().fix();

        assert!(
            fix.contains("migrating cannot fix this"),
            "'why not just auto migrate' is the question the reader arrives with: {fix}"
        );
        assert!(
            fix.contains("do NOT restart it"),
            "the store is healthy, and the old message said otherwise: {fix}"
        );
        assert!(
            !fix.contains("docker compose"),
            "never steer at the store: {fix}"
        );
        assert!(fix.contains("doctor upgrade"));
        assert!(
            fix.contains("install.sh"),
            "and the way out when upgrade cannot write"
        );
    }

    #[test]
    fn the_queue_message_is_an_error_and_keyed_by_what_is_missing() {
        let messages = skewed().desired_messages();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].key, "schema:ahead:0008-0009");
        assert_eq!(messages[0].source, SCHEMA_SOURCE);
        assert_eq!(
            messages[0].severity,
            Severity::Error,
            "this daemon is writing to a schema it does not understand"
        );
        assert!(messages[0].next_action.contains("doctor upgrade"));
    }

    /// A single extra migration is the common shape (one release ahead) and
    /// must not read as a range.
    #[test]
    fn one_extra_migration_is_summarised_as_itself() {
        let one = SchemaSkew {
            extra: vec![8],
            ..skewed()
        };

        assert_eq!(one.extra_summary(), "0008");
        assert_eq!(one.desired_messages()[0].key, "schema:ahead:0008");
    }
}
