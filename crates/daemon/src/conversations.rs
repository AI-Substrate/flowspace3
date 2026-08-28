//! Conversation intake: one endpoint, append-friendly, policy enforced here.
//!
//! `POST /conversations` accepts a conversation header plus a batch of turns
//! and is idempotent on `(conversation_id, turn_no)`, so a conversation GROWS
//! across many posts (req-0027, ruled by Jordan 2026-08-27). Re-posting an
//! overlap stores nothing, enqueues nothing and costs nothing — the primary key
//! decides, not a check somebody has to remember to write.
//!
//! # The policy lives here, not in the client
//!
//! Workshop 005's payload rulings — tool outputs cut to a 512-byte head, write
//! family inputs reduced to path and length — are ENFORCED at intake rather
//! than trusted from whoever posted. An importer that forgets to shape a
//! payload is an ordinary bug; a store that believed it would be a permanent
//! one, because the oversized bytes are already written by the time anyone
//! notices. The importer still shapes (it can be cheaper about it, having the
//! raw transcript in hand), and this is the backstop that makes the shape a
//! contract.
//!
//! # Identity, and why the orphan case needs no code
//!
//! Jobs carry the repo identity whose provider selection applies, and it is not
//! optional. An ANCHORED conversation passes its anchor identity, so it is
//! summarised and embedded by whatever provider that repository selected — the
//! same treatment as its code. An unanchored one passes [`UNANCHORED`].
//!
//! [`AppState::embedder_for`] is a map lookup with a default fallback, so an
//! identity nobody configured resolves to the default provider with no branch
//! anywhere. That is what makes the ORPHANED case — an anchor naming a
//! repository that was never registered, or has since been removed — cost zero
//! special-case code: it misses the map and takes the same fallback. And
//! because `conv:` is a namespace [`fs3_core::RepoIdentity`] can never mint
//! (it only ever produces `git:` or `path:` keys), the reserved identity cannot
//! collide with a real repository — while remaining configurable by anyone who
//! wants a cheaper summariser for conversations.

use fs3_core::conversation::earns_summary;
use fs3_core::envelope::Failure;
use fs3_core::{Conversation, ConversationId, Element, Turn, catalog};
use serde::{Deserialize, Serialize};

use crate::enrich;
use crate::runner::fail;
use crate::wiring::AppState;

/// The identity a conversation with no repository anchor is enriched under.
///
/// Workshop 003's `conv:` scheme, which [`fs3_core::RepoIdentity`] structurally
/// cannot mint, so this can never collide with a real repository.
pub const UNANCHORED: &str = "conv:unanchored";

/// How much of a tool result is kept (workshop 005, C2).
///
/// Re-exported from [`fs3_core`], which owns the payload policy: the importer
/// applies the same rules on the way in, and two copies of a truncation
/// constant are a constant that drifts (plan 005 risk r3).
pub use fs3_core::OUTPUT_HEAD_BYTES;

/// What a caller posts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntakeRequest {
    /// The conversation's guid. Minted by the client (the importer) so a
    /// re-import addresses the same conversation.
    pub guid: String,
    /// Anchor: the repository identity this conversation happened in.
    #[serde(default)]
    pub repo_identity: Option<String>,
    /// Anchor: the checkout path.
    #[serde(default)]
    pub worktree: Option<String>,
    /// Anchor: the commit the conversation started from.
    #[serde(default)]
    pub base_sha: Option<String>,
    /// Optional title.
    #[serde(default)]
    pub title: Option<String>,
    /// When the conversation began, RFC 3339.
    pub started_at: String,
    /// The batch. May overlap what is already stored.
    #[serde(default)]
    pub turns: Vec<Turn>,
}

/// What the intake answers with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct IntakeReport {
    /// The conversation this batch belongs to.
    pub guid: String,
    /// The identity its enrichment is charged to.
    pub identity: String,
    /// How many turns were newly stored.
    pub accepted: usize,
    /// How many were already there, unchanged.
    pub already_stored: usize,
    /// How many of the accepted turns earned their own summary.
    pub summarized: usize,
}

/// Accept a conversation header and a batch of turns.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for a guid that is not a conversation id or a
/// batch that cannot be a sequence; store failures mapped by their own codes.
pub async fn intake(state: &AppState, request: IntakeRequest) -> Result<IntakeReport, Failure> {
    let guid = ConversationId::new(request.guid.clone()).map_err(|error| {
        Failure::new(&catalog::QUERY_INVALID, error.to_string())
            .with_fix("post a canonical lowercase uuid as `guid`, or omit it and let `flowspace3 conversation import` mint one")
            .retryable(false)
    })?;

    for turn in &request.turns {
        if turn.turn_no == 0 {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                "turn_no 0: turns are numbered densely from 1, because the sequence IS the address"
                    .to_string(),
            )
            .with_fix("number the batch from 1 and re-post")
            .retryable(false));
        }
    }

    // Anchored conversations are enriched by their repository's provider;
    // everything else by the default, via a reserved identity nothing can
    // collide with.
    let identity = request
        .repo_identity
        .clone()
        .unwrap_or_else(|| UNANCHORED.to_string());

    let header = Conversation {
        guid: guid.clone(),
        repo_identity: request.repo_identity,
        worktree: request.worktree,
        base_sha: request.base_sha,
        title: request.title,
        started_at: request.started_at,
    };

    fs3_store::upsert_conversation(&state.db, &header)
        .await
        .map_err(fail)?;

    // Shaped BEFORE storage, so what is hashed, stored and enriched is the
    // shaped form and there is no window in which the oversized bytes exist.
    let turns: Vec<Turn> = request.turns.into_iter().map(shape).collect();

    let floor = state.config.indexing.turn_summary_min_bytes;
    let appended = fs3_store::append_turns(&state.db, &guid, &turns, |element: &Element| {
        earns_summary(&element.raw_text, floor)
    })
    .await
    .map_err(fail)?;

    // ONLY the delta. A re-post of an overlap reaches here with an empty
    // `accepted` and therefore pays nobody a second time.
    let summarized = enrich::enqueue_for_turns(state, &identity, &appended.accepted, floor).await?;

    Ok(IntakeReport {
        guid: guid.as_str().to_string(),
        identity,
        accepted: appended.accepted.len(),
        already_stored: appended.already_stored,
        summarized,
    })
}

/// What `GET /conversations` was asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ListRequest {
    /// Only conversations anchored to this repository identity.
    #[serde(default)]
    pub repo: Option<String>,
    /// Only conversations whose anchor checkout starts with this path.
    #[serde(default)]
    pub path: Option<String>,
}

/// One row of `conversation list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConversationRow {
    /// `conv:<guid>` — feed it straight to `tree` or `get`.
    pub address: String,
    /// The guid on its own, for `conversation remove`.
    pub guid: String,
    /// The title, when it has one.
    pub title: Option<String>,
    /// The anchor repository identity.
    pub repo: Option<String>,
    /// The anchor checkout path.
    pub worktree: Option<String>,
    /// How many turns are stored.
    pub turns: i64,
    /// When it began, RFC 3339 in UTC.
    pub started_at: String,
}

/// What `conversation list` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConversationList {
    /// The conversations, newest first.
    pub conversations: Vec<ConversationRow>,
}

/// What `POST /conversations/remove` was asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct RemoveRequest {
    /// The conversation to forget.
    pub guid: String,
}

/// What removing a conversation reclaimed directly.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RemoveReport {
    /// The conversation asked about.
    pub guid: String,
    /// Whether there was one to remove.
    pub existed: bool,
    /// Turn rows deleted.
    pub turns: i64,
    /// Turn element rows deleted.
    pub elements: i64,
}

/// List indexed conversations, newest first.
///
/// # Errors
/// Store failures mapped by their own codes.
pub async fn list(state: &AppState, request: &ListRequest) -> Result<ConversationList, Failure> {
    let rows = fs3_store::list_conversations(
        &state.db,
        fs3_store::AnchorFilter {
            repo: request.repo.as_deref(),
            path_prefix: request.path.as_deref(),
            guid: None,
        },
    )
    .await
    .map_err(fail)?;

    Ok(ConversationList {
        conversations: rows
            .into_iter()
            .map(|row| ConversationRow {
                address: row.guid.address(),
                guid: row.guid.as_str().to_string(),
                title: row.title,
                repo: row.repo_identity,
                worktree: row.worktree,
                turns: row.turns,
                started_at: row.started_at,
            })
            .collect(),
    })
}

/// Forget one conversation, its turns and its turn elements.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for a guid that is not a conversation id; store
/// failures mapped by their own codes.
pub async fn remove(state: &AppState, request: &RemoveRequest) -> Result<RemoveReport, Failure> {
    let guid = ConversationId::new(request.guid.clone()).map_err(|error| {
        Failure::new(&catalog::QUERY_INVALID, error.to_string())
            .with_fix("`flowspace3 conversation list` prints the guid of everything indexed")
            .retryable(false)
    })?;

    let removed = fs3_store::delete_conversation(&state.db, &guid)
        .await
        .map_err(fail)?;

    Ok(RemoveReport {
        guid: guid.as_str().to_string(),
        existed: removed.existed,
        turns: removed.turns,
        elements: removed.elements,
    })
}

/// What a caller typically does after removing a conversation.
#[must_use]
pub fn next_after_remove(report: &RemoveReport) -> String {
    if !report.existed {
        return "no conversation with that guid was indexed — `flowspace3 conversation list` \
                shows what is"
            .to_string();
    }
    format!(
        "{} turn(s) forgotten. Their summaries and vectors are keyed by content and may still be \
         shared, so `flowspace3 gc` decides those — it reclaims whatever nothing else carries.",
        report.turns
    )
}

/// What a caller typically does after posting a batch.
#[must_use]
pub fn next_after_intake(report: &IntakeReport) -> String {
    if report.accepted == 0 {
        format!(
            "nothing new in this batch — all {} turns were already stored. \
             `flowspace3 search \"<question>\" --source conversation` searches what is there.",
            report.already_stored
        )
    } else {
        format!(
            "{} turn(s) stored and queued for enrichment. \
             `flowspace3 status` watches the queue drain; then \
             `flowspace3 search \"<question>\" --source conversation`.",
            report.accepted
        )
    }
}

/// Apply workshop 005's payload rulings to one turn.
///
/// Delegates to [`fs3_core::shape_turn`] so the policy has ONE implementation.
/// Plan 005's importer must apply the same rules the intake enforces, and a
/// second copy of a truncation rule is a rule that drifts — so the policy, the
/// write-family list and the character-boundary cut moved to `fs3-core` and
/// this delegates to them.
///
/// Intake still ENFORCES rather than trusts: a client that posts an unshaped
/// turn is shaped here exactly as before. That backstop is unchanged, and the
/// tests below are its regression oracle — they are byte-identical to the ones
/// that guarded the private implementation.
///
/// Idempotent: shaping an already-shaped turn changes nothing, which is what
/// lets the importer shape cheaply and this enforce without double-cutting.
fn shape(turn: Turn) -> Turn {
    fs3_core::shape_turn(turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::{ToolInput, TurnItem};

    fn call(tool: &str, text: &str) -> TurnItem {
        TurnItem::ToolCall {
            tool: tool.to_string(),
            input: ToolInput::Verbatim {
                text: text.to_string(),
            },
        }
    }

    fn turn_with(items: Vec<TurnItem>) -> Turn {
        Turn {
            turn_no: 1,
            role: fs3_core::TurnRole::Agent,
            source: fs3_core::TurnSource::System,
            head_sha: None,
            at: "2026-08-27T09:00:00Z".to_string(),
            body: String::new(),
            items,
        }
    }

    #[test]
    fn a_write_family_body_is_reduced_to_its_path_and_length() {
        let body = "crates/store/src/lib.rs\n".to_string() + &"x".repeat(50_000);
        let shaped = shape(turn_with(vec![call("write", &body)]));

        let TurnItem::ToolCall { input, .. } = &shaped.items[0] else {
            panic!("still a call");
        };
        assert_eq!(
            *input,
            ToolInput::Elided {
                path: "crates/store/src/lib.rs".to_string(),
                bytes: body.len() as u64,
            },
            "the body is the very next commit; storing it here doubles the bill"
        );
    }

    /// Harnesses spell the same tool many ways. A policy that only catches one
    /// spelling silently stores every other harness's file bodies.
    #[test]
    fn the_write_family_is_matched_however_a_harness_spells_it() {
        for tool in [
            "write",
            "Write",
            "fs.write",
            "str_replace",
            "str_replace_editor",
            "edit",
            "Edit",
            "apply_patch",
        ] {
            let shaped = shape(turn_with(vec![call(tool, "a/path.rs\nbody")]));
            let TurnItem::ToolCall { input, .. } = &shaped.items[0] else {
                panic!("still a call");
            };
            assert!(
                matches!(input, ToolInput::Elided { .. }),
                "{tool} is write-family"
            );
        }
    }

    /// And a read is NOT: tool inputs are verbatim except the write family, so
    /// over-eager matching would throw away the intent search is looking for.
    #[test]
    fn a_reading_tool_keeps_its_input_verbatim() {
        for tool in ["read", "grep", "bash", "rewrite_history", "editor_config"] {
            let shaped = shape(turn_with(vec![call(tool, "AGENTS.md\nwhatever")]));
            let TurnItem::ToolCall { input, .. } = &shaped.items[0] else {
                panic!("still a call");
            };
            assert!(
                matches!(input, ToolInput::Verbatim { .. }),
                "{tool} is not write-family"
            );
        }
    }

    #[test]
    fn an_oversized_tool_result_is_cut_to_its_head_and_says_so() {
        let whole = "e".repeat(5_000);
        let shaped = shape(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: whole.clone(),
            total_bytes: 0,
            truncated: false,
        }]));

        let TurnItem::ToolResult {
            head,
            total_bytes,
            truncated,
            ..
        } = &shaped.items[0]
        else {
            panic!("still a result");
        };
        assert_eq!(head.len(), OUTPUT_HEAD_BYTES);
        assert_eq!(*total_bytes, whole.len() as u64, "the size is not lost");
        assert!(*truncated);
    }

    /// A client that already truncated knows a total we cannot recover, so
    /// enforcement must not overwrite it with the size of the head.
    #[test]
    fn a_clients_own_total_survives_enforcement() {
        let shaped = shape(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: "e".repeat(1_000),
            total_bytes: 9_000_000,
            truncated: true,
        }]));

        let TurnItem::ToolResult {
            total_bytes, head, ..
        } = &shaped.items[0]
        else {
            panic!("still a result");
        };
        assert_eq!(*total_bytes, 9_000_000);
        assert_eq!(head.len(), OUTPUT_HEAD_BYTES, "but the head is still cut");
    }

    /// Shaping is a backstop, so it runs over already-shaped payloads all the
    /// time. It must be a no-op there rather than cutting a second time.
    #[test]
    fn shaping_an_already_shaped_turn_changes_nothing() {
        let once = shape(turn_with(vec![
            call("write", "a.rs\nbody"),
            TurnItem::ToolResult {
                tool: "bash".to_string(),
                head: "e".repeat(2_000),
                total_bytes: 0,
                truncated: false,
            },
        ]));
        assert_eq!(shape(once.clone()), once);
    }

    /// The 512th byte lands mid-character sooner or later, and `truncate`
    /// panics there — which would lose the whole batch, not one result.
    #[test]
    fn a_cut_never_splits_a_character() {
        let shaped = shape(turn_with(vec![TurnItem::ToolResult {
            tool: "bash".to_string(),
            // 3 bytes each, so 512 is not a boundary.
            head: "☃".repeat(1_000),
            total_bytes: 0,
            truncated: false,
        }]));

        let TurnItem::ToolResult { head, .. } = &shaped.items[0] else {
            panic!("still a result");
        };
        assert!(head.len() <= OUTPUT_HEAD_BYTES);
        assert!(
            head.len() > OUTPUT_HEAD_BYTES - 4,
            "and cuts as late as it can"
        );
        assert!(head.chars().all(|c| c == '☃'));
    }

    #[test]
    fn the_reserved_identity_is_a_namespace_repo_identities_cannot_mint() {
        // `RepoIdentity` only ever produces `git:` or `path:` keys, so this
        // cannot collide with a repository — structurally, not by luck.
        assert!(UNANCHORED.starts_with(fs3_core::address::CONVERSATION_SCHEME));
        assert!(
            !fs3_core::RepoIdentity::from_path(std::path::Path::new("/srv/anything"))
                .key()
                .starts_with(fs3_core::address::CONVERSATION_SCHEME)
        );
    }
}
