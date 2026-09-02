//! Per-verb payload DTOs — the `data` half of the envelope, typed.
//!
//! # Why these live in core
//!
//! `Envelope` is generic in its payload precisely so the daemon can serialise a
//! typed `data` while a consumer deserialises an opaque `Value`. That works
//! until something wants to READ a payload with types — a human renderer, a
//! TUI, a future web client — and finds `SearchResults` inside `fs3-daemon`
//! (axum, sqlx, tokio) and `DoctorReport` inside `fs3-cli`. A consumer would
//! then have to depend on a web server to draw a table, so it does the only
//! other thing available: it mirrors the structs by hand, field for field, and
//! the two copies drift. `pocs/human-render/src/views.rs` is that mirror, and
//! `pocs/human-render/LEARNINGS.md` §3.1 calls the move done here "the one real
//! blocker" of the promotion.
//!
//! So the payloads sit beside the envelope they travel in. The producer and
//! every consumer now read the SAME definition, and a field added for one is a
//! field the other cannot miss. The functional-core rule is untouched: these
//! are plain serde structs with no IO, no pool, and no runtime.
//!
//! # What changed on the wire when they moved: nothing
//!
//! Field order is declaration order and every `#[serde]` attribute came across
//! verbatim, so serialisation is byte-identical — asserted by
//! `crates/cli/tests/envelope_goldens.rs`, whose goldens were captured from the
//! commit BEFORE this move. `Deserialize` was added where a producer-only type
//! had `Serialize` alone; deriving it changes no bytes and is what makes these
//! types readable by a consumer at all.
//!
//! # `skip_serializing_if` ALWAYS travels with `default`
//!
//! A producer-only type could get away with omitting a field on the way out and
//! never saying what its absence means on the way in. A SHARED type cannot: the
//! omission a producer makes is the omission a consumer must survive. The
//! renderer found this the honest way, on real data — `get --human` fell back
//! to JSON because `Outline.children` was skipped when empty and had no
//! `default`, so the first parent element without children failed the whole
//! payload (u-r, 2026-08-28).
//!
//! So the rule, held by `every_view_reads_back_what_it_writes` below: every
//! `skip_serializing_if` in this module carries `default`. Adding `default`
//! changes nothing about serialisation — the goldens prove that too — and it is
//! what makes a skipped field a shape rather than a trap.

pub mod doctor;
pub mod read;
pub mod remove;
pub mod roots;
pub mod search;
pub mod status;

#[cfg(test)]
mod tests {
    /// Every payload type must be able to read back exactly what it writes.
    ///
    /// The interesting case is not a full value — it is the SKIPPED one: a
    /// field the producer omitted because it was empty or absent. A consumer
    /// meets those on real data constantly (most elements have no children,
    /// most rows have no span), so a type that cannot parse its own omissions
    /// is a type that fails on the common case, not the rare one.
    ///
    /// Asserted per type rather than by scanning attributes: this test fails
    /// with the name of the type that broke, which is the thing a reader needs.
    #[test]
    fn every_view_reads_back_what_it_writes() {
        use super::{doctor, read, remove, roots, search, status};

        macro_rules! round_trip {
            ($value:expr) => {{
                let value = $value;
                let json = serde_json::to_string(&value).expect("it serialises");
                let back = serde_json::from_str(&json).unwrap_or_else(|error| {
                    panic!(
                        "{} cannot read its own output: {error}\nwrote: {json}",
                        std::any::type_name_of_val(&value)
                    )
                });
                assert_eq!(value, back);
            }};
        }

        // Each of these omits everything that is allowed to be omitted — the
        // shape a real payload takes far more often than the full one.
        round_trip!(read::Outline {
            address: "el:git:example/repo/src/a.rs::f".to_string(),
            kind: "function".to_string(),
            name: "f".to_string(),
            span: [1, 9],
            children: Vec::new(),
        });
        round_trip!(read::TreeEntry {
            kind: "directory".to_string(),
            name: "src".to_string(),
            address: None,
            path: None,
            span: None,
            files: None,
            role: None,
            source: None,
            at: None,
            children: Vec::new(),
        });
        round_trip!(read::TurnView {
            address: "conv:abc#t1".to_string(),
            turn_no: 1,
            role: "human".to_string(),
            source: "human".to_string(),
            head_sha: None,
            at: "2026-08-28T03:00:00Z".to_string(),
            body: "hello".to_string(),
            body_empty_reason: None,
            items: Vec::new(),
        });
        round_trip!(remove::RemoveReport {
            root_path: "/srv/api".to_string(),
            was_registered: false,
            identity: None,
            files: 0,
            jobs_killed: 0,
            repo_removed: false,
            reclaimable: remove::GcCounts::default(),
            registered: Vec::new(),
        });
        round_trip!(doctor::Step {
            check: "database".to_string(),
            outcome: "ok".to_string(),
            found: "reachable".to_string(),
            action: None,
            steer: None,
            elapsed_ms: 3,
        });
        round_trip!(roots::RootReport {
            identity: "git:example/repo".to_string(),
            identity_source: "remote".to_string(),
            root_path: "/srv/api".to_string(),
            worktree_id: 1,
            files: 0,
            skipped: Vec::new(),
            pruned: Vec::new(),
            enqueued: 0,
            unchanged: 0,
            removed: 0,
        });
        round_trip!(search::SearchResults {
            results: Vec::new(),
            composition: search::SearchComposition::default(),
        });
        round_trip!(status::StatusReport {
            roots: Vec::new(),
            queue: Vec::new(),
            retention: None,
            last_error: None,
            inconsistencies: Vec::new(),
            schema_ahead: Vec::new(),
        });
    }
}
