//! The one response envelope, for every CLI verb and every daemon endpoint
//! (workshop 004).
//!
//! ```json
//! { "ok": true,  "command": "search", "v": 1, "data": { … }, "meta": { … },
//!   "next_action": "…" }
//! { "ok": false, "command": "search", "v": 1,
//!   "error": { "code": "FS3-E-STORE-UNAVAILABLE", "message": "…", "fix": "…",
//!              "details": { … }, "retryable": true } }
//! ```
//!
//! `ok` is the ONLY discriminator (D1): a consumer never sniffs shapes, and a
//! new verb never invents one. The type here is deliberately one struct rather
//! than an enum, because both directions matter — the daemon serialises it and
//! the CLI deserialises whatever the daemon sent, including from a newer
//! version that added fields.
//!
//! # What lives where
//!
//! `command` is the verb, not the route: `search`, `add`, `status`, `doctor`.
//! An agent reading a log of envelopes can tell what was asked without
//! reconstructing the URL.
//!
//! `v` bumps only when the ENVELOPE shape breaks — never when a verb's `data`
//! grows a field. Payload evolution is additive by contract, which is why the
//! CLI's own deserialisation tolerates unknown fields.
//!
//! `next_action` is PRD req 44's agent steer: what a consumer typically does
//! next. It is advice, never a control instruction.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::Code;
use crate::messages::UserMessage;

/// The envelope version. Bumps only on a breaking envelope change (D1).
pub const ENVELOPE_VERSION: u32 = 1;

/// One response, success or failure.
///
/// Generic in its payload so the daemon can serialise a typed `data` while the
/// CLI deserialises an opaque [`Value`] — one shape, two vantage points, no DTO
/// layer between them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T = Value> {
    /// The only discriminator. `true` means `data` is present and `error` is
    /// not; `false` is the reverse.
    pub ok: bool,
    /// The verb this answers — `search`, `add`, `status`, `doctor`.
    pub command: String,
    /// Envelope version ([`ENVELOPE_VERSION`]).
    pub v: u32,
    /// The verb-specific payload, on success.
    #[serde(default = "Option::default", skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Optional out-of-band facts about the answer: counts, timings, what was
    /// filtered. Never load-bearing — a consumer that ignores it still works.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// What a consumer typically does next (PRD req 44).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    /// The daemon's live user messages (PRD req 59) — news that is about the
    /// SYSTEM rather than about this command's answer.
    ///
    /// Every envelope the daemon serves carries the same list, attached in one
    /// place, so a feature with something to say never has to add a field to a
    /// verb nobody would think to run. Empty is the healthy case and is
    /// omitted from the wire entirely; a consumer that ignores the field still
    /// works, exactly like `meta`.
    ///
    /// It is NOT `meta`: `meta` is out-of-band facts about *this answer*, and
    /// these are standing conditions of the installation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<UserMessage>,
    /// The failure, when `ok` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}

impl<T> Envelope<T> {
    /// A successful answer to `command`.
    pub fn ok(command: impl Into<String>, data: T) -> Self {
        Envelope {
            ok: true,
            command: command.into(),
            v: ENVELOPE_VERSION,
            data: Some(data),
            meta: None,
            next_action: None,
            messages: Vec::new(),
            error: None,
        }
    }

    /// Attach out-of-band facts.
    ///
    /// # Errors
    /// Never fails on the shapes fs3 builds; a payload that cannot serialise
    /// loses only the meta, so this returns `Self` and drops on error rather
    /// than turning a good answer into a bad one.
    #[must_use]
    pub fn with_meta(mut self, meta: impl Serialize) -> Self {
        self.meta = serde_json::to_value(meta).ok();
        self
    }

    /// Attach the agent steer.
    #[must_use]
    pub fn with_next_action(mut self, next_action: impl Into<String>) -> Self {
        self.next_action = Some(next_action.into());
        self
    }

    /// Attach the daemon's live user messages (PRD req 59).
    ///
    /// Called in ONE place per surface — the daemon's [`Answer`] on its way
    /// out, and the CLI's local verbs that hold a pool — so that "which
    /// commands carry messages" is a property of the surface rather than a
    /// decision each endpoint author gets to make differently.
    ///
    /// [`Answer`]: https://docs.rs/fs3-daemon
    #[must_use]
    pub fn with_messages(mut self, messages: Vec<UserMessage>) -> Self {
        self.messages = messages;
        self
    }

    /// A failed answer to `command`.
    pub fn failed(command: impl Into<String>, error: Failure) -> Self {
        Envelope {
            ok: false,
            command: command.into(),
            v: ENVELOPE_VERSION,
            data: None,
            meta: None,
            next_action: None,
            messages: Vec::new(),
            error: Some(error),
        }
    }

    /// The HTTP status this envelope should be served with (workshop 004 D4).
    ///
    /// Mechanical: success is 200, and a failure's status comes from its code's
    /// own spelling. An endpoint author never chooses.
    #[must_use]
    pub fn http_status(&self) -> u16 {
        match &self.error {
            None => 200,
            Some(failure) => failure.http_status(),
        }
    }
}

/// The error half of the envelope. Every field except `details` is mandatory —
/// `fix` most of all (D3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// A code from [`crate::catalog`].
    pub code: String,
    /// What happened, naming the concrete thing that failed.
    pub message: String,
    /// What to DO about it. Never empty: [`Failure::new`] falls back to the
    /// catalog's own template rather than allowing a blank.
    pub fix: String,
    /// Structured facts a consumer can branch on without parsing `message`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, Value>,
    /// Whether repeating the request could succeed without a change (D5).
    pub retryable: bool,
}

impl Failure {
    /// Build a failure from a registry code and a concrete message.
    ///
    /// `fix` and `retryable` come from the catalog, so the common case cannot
    /// omit them and the doctrine holds by construction.
    pub fn new(code: &Code, message: impl Into<String>) -> Self {
        Failure {
            code: code.as_str().to_string(),
            message: message.into(),
            fix: code.fix().to_string(),
            details: BTreeMap::new(),
            retryable: code.retryable(),
        }
    }

    /// Replace the catalog's default `fix` with a more specific one.
    ///
    /// Used when the caller knows the actual path, variable or command — "run
    /// `flowspace3 add /srv/code/api`" beats "run `flowspace3 add <path>`".
    /// A blank replacement is ignored: the doctrine is that a `fix` always
    /// exists, and silently accepting an empty one would be the hole.
    #[must_use]
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        let fix = fix.into();
        if !fix.trim().is_empty() {
            self.fix = fix;
        }
        self
    }

    /// Add one structured detail.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.details.insert(key.into(), value);
        }
        self
    }

    /// Override the catalog's retryability for this occurrence.
    ///
    /// A provider failure is retryable in general but not when the cause is a
    /// missing API key, and the caller is the only one who knows which it was.
    #[must_use]
    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// The HTTP status for this failure, from the code's class.
    ///
    /// Falls back to 500 for a code that is not in the registry, which is what
    /// a CLI reading a NEWER daemon's envelope will see.
    #[must_use]
    pub fn http_status(&self) -> u16 {
        crate::catalog::find(&self.code).map_or(500, Code::http_status)
    }

    /// The two lines a human reads: what happened, then what to do.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}: {}\nfix: {}", self.code, self.message, self.fix)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn a_success_envelope_carries_data_and_no_error_key() {
        let envelope = Envelope::ok("status", serde_json::json!({ "roots": 2 }));
        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(json["ok"], serde_json::json!(true));
        assert_eq!(json["command"], serde_json::json!("status"));
        assert_eq!(json["v"], serde_json::json!(1));
        assert_eq!(json["data"]["roots"], serde_json::json!(2));
        // Absent, not null: a consumer branching on `ok` must never find a key
        // belonging to the other shape.
        assert!(json.get("error").is_none());
        assert!(json.get("meta").is_none());
    }

    #[test]
    fn a_failure_envelope_carries_error_and_no_data_key() {
        let envelope = Envelope::<Value>::failed(
            "search",
            Failure::new(
                &catalog::STORE_UNAVAILABLE,
                "cannot reach the store at postgres://127.0.0.1:5433/flowspace3",
            )
            .with_detail("cause", "connection refused")
            .with_detail("elapsed_ms", 5600),
        );
        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(json["ok"], serde_json::json!(false));
        assert!(json.get("data").is_none());
        assert_eq!(
            json["error"]["code"],
            serde_json::json!("FS3-E-STORE-UNAVAILABLE")
        );
        assert_eq!(json["error"]["retryable"], serde_json::json!(true));
        assert_eq!(
            json["error"]["details"]["elapsed_ms"],
            serde_json::json!(5600)
        );
        assert!(
            json["error"]["fix"]
                .as_str()
                .unwrap()
                .contains("docker compose up -d"),
            "the catalog's fix must ride along without the caller repeating it"
        );
    }

    /// The doctrine's load-bearing property: there is no way to produce an
    /// error envelope without a fix, including by trying to blank it.
    #[test]
    fn a_fix_can_be_sharpened_but_never_removed() {
        let default = Failure::new(&catalog::SCAN_ROOT_NOT_FOUND, "no such directory");
        assert!(!default.fix.is_empty());

        let blanked = Failure::new(&catalog::SCAN_ROOT_NOT_FOUND, "no such directory")
            .with_fix("   ")
            .with_fix("");
        assert_eq!(blanked.fix, default.fix, "a blank fix must be refused");

        let sharpened = Failure::new(&catalog::SCAN_ROOT_NOT_FOUND, "no such directory")
            .with_fix("create /srv/code/api, or point `flowspace3 add` at a path that exists");
        assert!(sharpened.fix.contains("/srv/code/api"));
    }

    #[test]
    fn http_status_comes_from_the_code_not_the_endpoint() {
        assert_eq!(Envelope::ok("status", ()).http_status(), 200);
        assert_eq!(
            Envelope::<Value>::failed("add", Failure::new(&catalog::SCAN_ROOT_NOT_FOUND, "x"))
                .http_status(),
            404
        );
        assert_eq!(
            Envelope::<Value>::failed("search", Failure::new(&catalog::STORE_UNAVAILABLE, "x"))
                .http_status(),
            503
        );
        assert_eq!(
            Envelope::<Value>::failed("search", Failure::new(&catalog::QUERY_INVALID, "x"))
                .http_status(),
            400
        );
    }

    /// An older CLI must survive a newer daemon: unknown fields are tolerated
    /// and an unknown code degrades to 500 rather than panicking.
    #[test]
    fn an_envelope_from_a_newer_daemon_still_parses() {
        let wire = serde_json::json!({
            "ok": false,
            "command": "search",
            "v": 1,
            "invented_later": { "anything": true },
            "error": {
                "code": "FS3-E-QUERY-FROM-THE-FUTURE",
                "message": "…",
                "fix": "…",
                "retryable": false,
                "invented_later": 1
            }
        });
        let envelope: Envelope = serde_json::from_value(wire).unwrap();
        assert!(!envelope.ok);
        assert_eq!(envelope.http_status(), 500);
        assert_eq!(envelope.error.unwrap().code, "FS3-E-QUERY-FROM-THE-FUTURE");
    }

    #[test]
    fn a_failure_renders_as_what_happened_then_what_to_do() {
        let rendered = Failure::new(
            &catalog::STORE_SCHEMA_STALE,
            "database is 2 migrations behind",
        )
        .render();
        assert!(rendered.starts_with("FS3-E-STORE-SCHEMA-STALE: database is 2 migrations behind"));
        assert!(rendered.contains("fix: run `flowspace3 doctor`"));
    }
}
