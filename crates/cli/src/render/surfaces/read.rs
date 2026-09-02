use comfy_table::Cell;
use fs3_core::{
    envelope::Envelope,
    views::read::{GetPayload, TreeEntry, TreeResult},
};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::theme;

#[must_use]
pub fn get(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let payload: GetPayload = serde_json::from_value(envelope.data.clone()?).ok()?;
    let mut out = match payload {
        GetPayload::Element(result) => {
            let mut out = theme::title(
                "get",
                &format!(
                    "{} · {}:{}-{}",
                    result.kind, result.path, result.span[0], result.span[1]
                ),
            );
            out.push_str(&format!(
                "\n\n{}{}\n\n",
                theme::GUTTER,
                result.address.bright_black()
            ));
            if let Some(summary) = result
                .smart
                .as_deref()
                .filter(|text| !text.trim().is_empty())
            {
                out.push_str(&format!("{}{}\n\n", theme::GUTTER, summary.bright_white()));
            }
            out.push_str(&result.raw_text);
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out
        }
        GetPayload::Conversation(window) => {
            let mut out = theme::title(
                "get",
                &format!("{} turns · around {}", window.turns, window.around),
            );
            out.push_str(&format!(
                "\n\n{}{}\n",
                theme::GUTTER,
                window
                    .title
                    .as_deref()
                    .unwrap_or(&window.address)
                    .bright_white()
            ));
            for turn in window.window {
                out.push_str(&format!(
                    "\n{}{} #{}  {}\n",
                    theme::GUTTER,
                    turn.role.bright_cyan(),
                    turn.turn_no,
                    turn.at.bright_black()
                ));
                if turn.body.trim().is_empty() {
                    let reason = turn
                        .body_empty_reason
                        .as_deref()
                        .unwrap_or("the stored turn contains no prose");
                    out.push_str(&format!("{}  {}\n", theme::GUTTER, reason.bright_black()));
                } else {
                    for line in theme::wrap(&turn.body, usize::from(width) - 4, 2).lines() {
                        out.push_str(&format!("{}  {line}\n", theme::GUTTER));
                    }
                }
            }
            out
        }
    };
    append_next(&mut out, envelope, width);
    Some(out)
}

#[must_use]
pub fn tree(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let result: TreeResult = serde_json::from_value(envelope.data.clone()?).ok()?;
    let hidden = result.include_hidden.map_or_else(String::new, |enabled| {
        format!(" · hidden {}", if enabled { "yes" } else { "no" })
    });
    let mut out = theme::title(
        "tree",
        &format!(
            "{} · showing {} of {}{}",
            result.kind, result.showing, result.total, hidden
        ),
    );
    out.push_str("\n\n");
    let mut table = theme::table(width);
    table.set_header([
        theme::header("kind"),
        theme::header("name"),
        theme::header("address / path"),
        theme::header("count"),
    ]);
    add_entries(&mut table, &result.entries, 0);
    out.push_str(&theme::block(&table));
    append_next(&mut out, envelope, width);
    Some(out)
}

fn add_entries(table: &mut comfy_table::Table, entries: &[TreeEntry], depth: usize) {
    for entry in entries {
        let location = entry
            .address
            .as_deref()
            .or(entry.path.as_deref())
            .unwrap_or("—");
        let count = entry
            .files
            .map_or_else(String::new, |value| value.to_string());
        table.add_row([
            Cell::new(entry.kind.clone()),
            Cell::new(format!("{}{}", "  ".repeat(depth), entry.name)),
            Cell::new(format!("{}", location.bright_black())),
            theme::right(count),
        ]);
        add_entries(table, &entry.children, depth + 1);
    }
}

fn append_next(out: &mut String, envelope: &Envelope<Value>, width: u16) {
    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, usize::from(width)));
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_less_turn_says_why_it_is_empty() {
        let envelope: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"get","v":1,"data":{"address":"conv:abc","repo":null,
                "worktree":null,"base_sha":null,"title":null,"turns":1,"around":1,
                "window":[{"address":"conv:abc#t1","turn_no":1,"role":"agent","source":"peer",
                "head_sha":null,"at":"2026-08-30T00:00:00Z","body":"",
                "body_empty_reason":"the stored turn contains typed items but no prose","items":[]}]}}"#,
        )
        .unwrap();
        let screen = anstream::adapter::strip_str(&get(&envelope, 100).unwrap()).to_string();
        assert!(screen.contains("typed items but no prose"), "{screen}");
    }

    #[test]
    fn tree_title_shows_the_resolved_root_policy() {
        let envelope: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"tree","v":1,"data":{"target":"git:example/repo","repo":"git:example/repo","include_hidden":true,"kind":"repository","total":0,"showing":0,"entries":[],"inconsistencies":[]}}"#,
        )
        .unwrap();
        let screen = anstream::adapter::strip_str(&tree(&envelope, 100).unwrap()).to_string();
        assert!(screen.contains("hidden yes"), "{screen}");
    }
}
