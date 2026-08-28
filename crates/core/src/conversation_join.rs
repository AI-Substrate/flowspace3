//! Resolving a pij seat id to the native session it is bound to (plan 005,
//! tk-c301).
//!
//! An operator knows a conversation two ways: by the fleet SEAT that had it
//! ("what did pij-appalling-slug do"), and by the harness's own session uuid.
//! The second is directly addressable; the first needs this join, which is the
//! `pij sessions` registry read as a table of
//! `(seat, harness, native session id)`.
//!
//! # Pure, on purpose
//!
//! Nothing here runs `pij`. The composition root shells out and hands the JSON
//! in, so the routing rules — which store a harness means, whether the uuid
//! shape agrees with it — are testable from an ordinary `#[test]` against a
//! recorded registry payload. That is the same split the readers have: the
//! world is somebody else's problem, the decisions are here.
//!
//! # Routing on the field, not on the uuid shape
//!
//! Recipe §2 says the uuid SHAPE routes the store: v4 to claude or copilot, v7
//! to omp. Measured against the live registry (908 rows: pi 800, claude 65,
//! copilot 41, codex 2) that is not sufficient — claude and copilot are BOTH
//! v4, and copilot sessions exist only inside the git-ai metrics database, so
//! shape alone cannot tell the two stores apart. The registry's own `harness`
//! field can, so it routes; the uuid shape is kept as a CONSISTENCY CHECK. A
//! row whose shape contradicts its harness means the registry is lying, and
//! this says so rather than guessing which half to believe.

use std::path::PathBuf;

use serde::Deserialize;

use crate::conversation_source::Harness;
use crate::error::{Error, Result};

/// One row of `pij sessions --json`.
///
/// The registry emits more fields than this (bound model, parent, prime flags);
/// unknown ones are ignored on purpose, so a registry that grows a column does
/// not break the join.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    /// The seat id, e.g. `pij-appalling-slug`.
    pub pij_id: String,
    /// How pij spells the harness. Note `pi`, not `omp` — see [`store_for`].
    pub harness: String,
    /// The harness's own session uuid. Absent for a seat that never bound.
    #[serde(default)]
    pub harness_session_id: Option<String>,
    /// The seat's git directory, when pij recorded one.
    ///
    /// Usable as a FOLDER DEFAULT (strip the trailing `/.git`), but it is not
    /// evidence of where the seat's shell was: pij registers a worktree-resident
    /// seat against its MAIN CLONE, measured across every seat of this plan's
    /// fleet.
    #[serde(default)]
    pub git_common_dir: Option<String>,
}

/// A seat resolved to the conversation store that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatBinding {
    /// The seat this came from.
    pub pij_id: String,
    /// Which store to read.
    pub harness: Harness,
    /// The native session id inside that store.
    pub session_id: String,
    /// The repository the seat was registered against, if pij recorded one.
    pub folder: Option<PathBuf>,
}

/// Parse `pij sessions --json` output.
///
/// # Errors
/// [`Error::InvalidConfig`] when the payload is not the array of rows the
/// registry documents.
pub fn parse_rows(json: &str) -> Result<Vec<SessionRow>> {
    serde_json::from_str(json).map_err(|err| {
        Error::InvalidConfig(format!("pij sessions output is not a row array: {err}"))
    })
}

/// Which store holds conversations for a harness pij names.
///
/// `pi` is omp — pij's spelling, and the one an implementation that trusts
/// `Harness::as_str()` to round-trip will silently miss. `copilot` has no
/// native store of its own: its sessions exist only as git-ai metrics rows,
/// which is why it routes to [`Harness::MetricsDb`].
///
/// # Errors
/// [`Error::InvalidConfig`] for a harness v1 has no reader for, naming it.
/// `codex` is the live example: the registry holds codex seats and this plan
/// ships no codex reader, so a codex seat is refused explicitly rather than
/// resolved to a store that cannot hold it.
pub fn store_for(registry_harness: &str) -> Result<Harness> {
    match registry_harness {
        "claude" => Ok(Harness::Claude),
        "pi" | "omp" => Ok(Harness::Omp),
        "copilot" => Ok(Harness::MetricsDb),
        other => Err(Error::InvalidConfig(format!(
            "pij harness {other:?} has no conversation reader in this version: \
             expected claude, pi or copilot"
        ))),
    }
}

/// The UUID version nibble, when the value is shaped like a UUID at all.
///
/// Read from the version position rather than by matching the `01a0` prefix the
/// recipe describes: the prefix is an artefact of when these v7 ids happened to
/// be minted (v7 leads with a millisecond timestamp), so it ages out, while the
/// version nibble is defined by the format.
#[must_use]
pub fn uuid_version(value: &str) -> Option<u8> {
    let group = value.split('-').nth(2)?;
    if group.len() != 4 {
        return None;
    }
    group
        .chars()
        .next()?
        .to_digit(16)
        .map(|nibble| nibble as u8)
}

/// The uuid version a store's ids are expected to carry, when it is fixed.
fn expected_version(harness: Harness) -> Option<u8> {
    match harness {
        Harness::Omp => Some(7),
        Harness::Claude | Harness::MetricsDb => Some(4),
        Harness::PijLedger => None,
    }
}

/// Resolve one seat against a parsed registry.
///
/// # Errors
/// [`Error::InvalidConfig`] when the seat is absent, when it never bound a
/// session, when its harness has no reader, or when the uuid shape contradicts
/// the harness the registry claims.
pub fn resolve_seat(rows: &[SessionRow], pij_id: &str) -> Result<SeatBinding> {
    let row = rows
        .iter()
        .find(|row| row.pij_id == pij_id)
        .ok_or_else(|| {
            Error::InvalidConfig(format!(
                "no pij session registered for seat {pij_id:?}; \
                 `pij sessions` is the join and it does not know this seat"
            ))
        })?;

    let session_id = row
        .harness_session_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            Error::InvalidConfig(format!(
                "seat {pij_id:?} is registered but never bound a {} session, \
             so there is no conversation to read yet",
                row.harness
            ))
        })?;

    let harness = store_for(&row.harness)?;

    // The consistency check. A disagreement here means the registry's harness
    // and its session id describe different things, and picking either one
    // would silently read the wrong store — or read nothing and report an
    // empty conversation, which is worse.
    if let (Some(expected), Some(actual)) = (expected_version(harness), uuid_version(session_id))
        && expected != actual
    {
        return Err(Error::InvalidConfig(format!(
            "seat {pij_id:?} claims harness {} but its session id {session_id:?} is a v{actual} \
             uuid where {harness} ids are v{expected}: the registry disagrees with itself",
            row.harness
        )));
    }

    Ok(SeatBinding {
        pij_id: row.pij_id.clone(),
        harness,
        session_id: session_id.to_string(),
        folder: row.git_common_dir.as_deref().map(folder_of),
    })
}

/// The working directory a `gitCommonDir` implies.
fn folder_of(git_common_dir: &str) -> PathBuf {
    let trimmed = git_common_dir
        .strip_suffix("/.git")
        .unwrap_or(git_common_dir);
    PathBuf::from(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes copied from a live `pij sessions --json`, one per harness the
    /// registry actually emits, with the fields this join reads.
    const REGISTRY: &str = r#"[
      {"pijId":"pij-pale-silkworm","harness":"pi",
       "harnessSessionId":"01a045f4-edc2-7000-8dc7-47d6d5677147",
       "boundModel":"github-copilot/claude-opus-5",
       "gitCommonDir":"/Users/x/substrate/flowspace/flowspace3/.git","prime":false},
      {"pijId":"pij-able-eel","harness":"claude",
       "harnessSessionId":"beb934d7-71c9-4555-b0ba-247edd7230fe"},
      {"pijId":"pij-able-falcon","harness":"copilot",
       "harnessSessionId":"467210db-27ad-4289-8568-f7f90b98782d"},
      {"pijId":"pij-codex-seat","harness":"codex",
       "harnessSessionId":"c5967bc2-f25c-438e-a23f-a61c15de973e"},
      {"pijId":"pij-never-bound","harness":"claude"}
    ]"#;

    fn registry() -> Vec<SessionRow> {
        parse_rows(REGISTRY).expect("the recorded registry payload parses")
    }

    #[test]
    fn unknown_registry_fields_do_not_break_the_join() {
        let rows = registry();
        assert_eq!(
            rows.len(),
            5,
            "every row parsed despite boundModel and prime"
        );
    }

    #[test]
    fn pij_spells_omp_as_pi() {
        // The whole point: an implementation that trusts `Harness::as_str()` to
        // round-trip looks for "omp", finds nothing, and reports an empty join.
        assert_eq!(store_for("pi").unwrap(), Harness::Omp);
        assert_eq!(Harness::Omp.as_str(), "omp");
    }

    #[test]
    fn copilot_routes_to_the_metrics_database() {
        // Copilot has no native store; it exists only as git-ai metrics rows.
        assert_eq!(store_for("copilot").unwrap(), Harness::MetricsDb);
    }

    #[test]
    fn a_harness_with_no_reader_is_named_not_guessed() {
        let err = resolve_seat(&registry(), "pij-codex-seat")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("codex"),
            "the refusal names the harness: {err}"
        );
    }

    #[test]
    fn a_seat_resolves_to_its_store_and_session() {
        let bound = resolve_seat(&registry(), "pij-pale-silkworm").unwrap();
        assert_eq!(bound.harness, Harness::Omp);
        assert_eq!(bound.session_id, "01a045f4-edc2-7000-8dc7-47d6d5677147");
        assert_eq!(
            bound.folder,
            Some(PathBuf::from("/Users/x/substrate/flowspace/flowspace3")),
            "the git dir becomes a folder, without its /.git"
        );
    }

    #[test]
    fn claude_and_copilot_are_both_v4_so_shape_alone_cannot_route() {
        // This is the measurement that demoted the recipe's shape rule to a
        // check: both are v4, and they live in different stores.
        let claude = resolve_seat(&registry(), "pij-able-eel").unwrap();
        let copilot = resolve_seat(&registry(), "pij-able-falcon").unwrap();
        assert_eq!(uuid_version(&claude.session_id), Some(4));
        assert_eq!(uuid_version(&copilot.session_id), Some(4));
        assert_ne!(claude.harness, copilot.harness);
    }

    #[test]
    fn a_uuid_shape_that_contradicts_its_harness_is_refused() {
        let rows = vec![SessionRow {
            pij_id: "pij-liar".into(),
            harness: "claude".into(),
            // A v7 id under a harness whose ids are v4.
            harness_session_id: Some("01a045f4-edc2-7000-8dc7-47d6d5677147".into()),
            git_common_dir: None,
        }];
        let err = resolve_seat(&rows, "pij-liar").unwrap_err().to_string();
        assert!(
            err.contains("disagrees with itself"),
            "the registry contradiction is reported, not resolved by guessing: {err}"
        );
    }

    #[test]
    fn a_seat_that_never_bound_is_a_different_failure_from_an_unknown_seat() {
        let unbound = resolve_seat(&registry(), "pij-never-bound")
            .unwrap_err()
            .to_string();
        let unknown = resolve_seat(&registry(), "pij-no-such-seat")
            .unwrap_err()
            .to_string();
        assert!(unbound.contains("never bound"), "{unbound}");
        assert!(unknown.contains("does not know this seat"), "{unknown}");
    }

    #[test]
    fn the_version_nibble_is_read_from_the_format_not_from_a_prefix() {
        assert_eq!(
            uuid_version("01a045f4-edc2-7000-8dc7-47d6d5677147"),
            Some(7)
        );
        assert_eq!(
            uuid_version("beb934d7-71c9-4555-b0ba-247edd7230fe"),
            Some(4)
        );
        assert_eq!(uuid_version("not-a-uuid"), None);
        assert_eq!(uuid_version(""), None);
    }
}
