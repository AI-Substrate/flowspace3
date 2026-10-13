//! Correcting conversation turns stored before the 2026-10-13 intake fixes.
//!
//! Turns are append-only, so a reader fix reaches only the turns read after
//! it. Everything the fixes decide is still on the stored row: the whole
//! write-family input sits in the elided path, and a turn's body and items say
//! who it speaks for. So each rule here is a reader's rule restated on what the
//! store kept, written to match only the turns the old readers got wrong.
//!
//! Every rule leaves a turn that is already right unchanged, so restating a
//! turn twice is restating it once, and a turn stored after the fix is never
//! touched.

use fs3_core::{Turn, TurnItem, TurnRole, TurnSource, shape_turn};

use super::omp::{is_peer_injected, opens_with_envelope};

/// How the claude harness notices stored as peer turns begin. Measured on the
/// live index: these three are every claude peer turn that is not a
/// cross-session or coordinator message.
const CLAUDE_NOTICES: [&str; 3] = [
    "<task-notification>",
    "[SYSTEM NOTIFICATION",
    "Your claude.ai usage limit",
];

/// A stored turn as the current intake would have stored it.
///
/// `harness` is the conversation's recorded agent harness; a conversation that
/// names none gets only the write-family path correction, which needs no
/// harness to judge.
#[must_use]
pub fn restate_stored_turn(harness: Option<&str>, turn: Turn) -> Turn {
    let mut turn = shape_turn(turn);
    match harness {
        Some("claude") => {
            if turn.role == TurnRole::Human
                && turn.source == TurnSource::Peer
                && CLAUDE_NOTICES
                    .iter()
                    .any(|notice| turn.body.trim_start().starts_with(notice))
            {
                turn.source = TurnSource::System;
            }
            if turn.role == TurnRole::Human
                && turn.source == TurnSource::System
                && turn
                    .items
                    .iter()
                    .any(|item| matches!(item, TurnItem::ToolResult { .. }))
            {
                turn.role = TurnRole::Agent;
            }
        }
        Some("omp") => {
            // A pij custom message, stored as the harness talking.
            if turn.role == TurnRole::Agent
                && turn.source == TurnSource::System
                && turn.items.is_empty()
                && opens_with_envelope(&turn.body)
            {
                turn.role = TurnRole::Human;
                turn.source = TurnSource::Peer;
            }
            restate_rs_envelope(&mut turn);
        }
        Some("pij") => restate_rs_envelope(&mut turn),
        _ => {}
    }
    turn
}

/// A person's turn carrying the `[pij-rs from` envelope the readers missed.
fn restate_rs_envelope(turn: &mut Turn) {
    if turn.role == TurnRole::Human
        && turn.source == TurnSource::Human
        && is_peer_injected(&turn.body)
    {
        turn.source = TurnSource::Peer;
    }
}

#[cfg(test)]
mod tests {
    use fs3_core::ToolInput;

    use super::*;

    fn turn(role: TurnRole, source: TurnSource, body: &str, items: Vec<TurnItem>) -> Turn {
        Turn {
            turn_no: 1,
            role,
            source,
            head_sha: None,
            at: "2026-09-10T00:00:00Z".to_string(),
            body: body.to_string(),
            items,
        }
    }

    fn result() -> TurnItem {
        TurnItem::ToolResult {
            tool: "Bash".to_string(),
            head: "Cargo.toml".to_string(),
            total_bytes: 10,
            truncated: false,
        }
    }

    fn who(turn: &Turn) -> (TurnRole, TurnSource) {
        (turn.role, turn.source)
    }

    #[test]
    fn a_whole_json_input_stored_as_a_path_is_cut_to_the_path() {
        let input =
            serde_json::json!({"file_path": "/w/src/lib.rs", "content": "body"}).to_string();
        let stored = turn(
            TurnRole::Agent,
            TurnSource::System,
            "",
            vec![TurnItem::ToolCall {
                tool: "Write".to_string(),
                input: ToolInput::Elided {
                    path: input.clone(),
                    bytes: input.len() as u64,
                },
            }],
        );
        for harness in [Some("claude"), Some("omp"), None] {
            let restated = restate_stored_turn(harness, stored.clone());
            assert_eq!(
                restated.items[0],
                TurnItem::ToolCall {
                    tool: "Write".to_string(),
                    input: ToolInput::Elided {
                        path: "/w/src/lib.rs".to_string(),
                        bytes: input.len() as u64,
                    },
                },
                "{harness:?}"
            );
        }
    }

    #[test]
    fn claude_notices_stored_as_peers_become_the_harness() {
        for body in [
            "<task-notification>\n<task-id>b1</task-id>\n</task-notification>",
            "[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated event.",
            "Your claude.ai usage limit has reset. Continue the task.",
        ] {
            let stored = turn(TurnRole::Human, TurnSource::Peer, body, vec![]);
            assert_eq!(
                who(&restate_stored_turn(Some("claude"), stored)),
                (TurnRole::Human, TurnSource::System),
                "{body}"
            );
        }
        for body in [
            "Another Claude session sent a message:\n<cross-session-message from=\"uds:/x\">",
            "The coordinator sent a message while you were working:\nThanks.",
        ] {
            let stored = turn(TurnRole::Human, TurnSource::Peer, body, vec![]);
            assert_eq!(
                who(&restate_stored_turn(Some("claude"), stored)),
                (TurnRole::Human, TurnSource::Peer),
                "a real peer message stays a peer's: {body}"
            );
        }
    }

    #[test]
    fn a_claude_tool_result_becomes_the_agents_turn() {
        let stored = turn(TurnRole::Human, TurnSource::System, "", vec![result()]);
        assert_eq!(
            who(&restate_stored_turn(Some("claude"), stored)),
            (TurnRole::Agent, TurnSource::System)
        );
        let meta = turn(
            TurnRole::Human,
            TurnSource::System,
            "<local-command-caveat>",
            vec![],
        );
        assert_eq!(
            who(&restate_stored_turn(Some("claude"), meta)),
            (TurnRole::Human, TurnSource::System),
            "a meta row carries no result and stays where it was"
        );
    }

    #[test]
    fn an_omp_pij_delivery_stored_as_the_harness_becomes_a_peer() {
        let delivered = turn(
            TurnRole::Agent,
            TurnSource::System,
            "[pij-rs from pij-a]\nImplement the mapper.\n[/pij]",
            vec![],
        );
        assert_eq!(
            who(&restate_stored_turn(Some("omp"), delivered)),
            (TurnRole::Human, TurnSource::Peer)
        );
        for body in [
            "You are pij session pij-b (PEER). This is context only — no action is required.\n\
             Peers can reach you; their messages arrive inline as \"[pij-rs from <id>]\".",
            "<irc>\nIncoming IRC message from agent `Scout`:\n\n[pij from pij-a] relayed",
        ] {
            let quoted = turn(TurnRole::Agent, TurnSource::System, body, vec![]);
            assert_eq!(
                who(&restate_stored_turn(Some("omp"), quoted)),
                (TurnRole::Agent, TurnSource::System),
                "quoting an envelope is not delivering one: {body}"
            );
        }
    }

    #[test]
    fn an_rs_envelope_stored_as_a_person_becomes_a_peer() {
        for harness in ["omp", "pij"] {
            let stored = turn(
                TurnRole::Human,
                TurnSource::Human,
                "[pij-rs from pij-a]\ngo\n[/pij]",
                vec![],
            );
            assert_eq!(
                who(&restate_stored_turn(Some(harness), stored)),
                (TurnRole::Human, TurnSource::Peer),
                "{harness}"
            );
        }
        let typed = turn(
            TurnRole::Human,
            TurnSource::Human,
            "fix the watcher",
            vec![],
        );
        assert_eq!(
            who(&restate_stored_turn(Some("omp"), typed)),
            (TurnRole::Human, TurnSource::Human)
        );
    }

    #[test]
    fn restating_twice_is_restating_once() {
        let stored = [
            (
                Some("claude"),
                turn(
                    TurnRole::Human,
                    TurnSource::Peer,
                    "<task-notification>",
                    vec![],
                ),
            ),
            (
                Some("claude"),
                turn(TurnRole::Human, TurnSource::System, "", vec![result()]),
            ),
            (
                Some("omp"),
                turn(
                    TurnRole::Agent,
                    TurnSource::System,
                    "[pij from a]\nx",
                    vec![],
                ),
            ),
            (
                Some("pij"),
                turn(
                    TurnRole::Human,
                    TurnSource::Human,
                    "[pij-rs from a]\nx",
                    vec![],
                ),
            ),
        ];
        for (harness, turn) in stored {
            let once = restate_stored_turn(harness, turn);
            assert_eq!(restate_stored_turn(harness, once.clone()), once);
        }
    }
}
