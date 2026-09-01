use comfy_table::Cell;
use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::theme;

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    match envelope.command.as_str() {
        "docs" => docs(envelope, width),
        "agents-start-here" => agents(envelope, width),
        "conversation list" => conversations(envelope, width),
        "conversation verify" => conversation_verify(envelope, width),
        _ => None,
    }
}

fn docs(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let topics = envelope.data.as_ref()?.get("topics")?.as_array()?;
    let mut out = theme::title("docs list", &format!("{} topics", topics.len()));
    out.push_str("\n\n");
    let mut table = theme::table(width);
    table.set_header([
        theme::header("topic"),
        theme::header("description"),
        theme::header("bytes"),
    ]);
    for topic in topics {
        table.add_row([
            Cell::new(topic.get("name")?.as_str()?.to_string()),
            Cell::new(topic.get("title")?.as_str()?.to_string()),
            theme::right(topic.get("bytes")?.as_u64()?.to_string()),
        ]);
    }
    out.push_str(&theme::block(&table));
    append_next(&mut out, envelope, width);
    Some(out)
}

fn agents(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let page = envelope.data.as_ref()?;
    let text = page.get("text")?.as_str()?;
    let title = page
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("agent guide");
    let mut out = theme::title("agents-start-here", title);
    out.push_str("\n\n");
    out.push_str(text);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    append_next(&mut out, envelope, width);
    Some(out)
}

fn conversations(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let rows = envelope.data.as_ref()?.get("conversations")?.as_array()?;
    let mut out = theme::title(
        "conversation list",
        &format!("{} conversations", rows.len()),
    );
    out.push_str("\n\n");
    if rows.is_empty() {
        out.push_str(&format!(
            "{}{}\n",
            theme::GUTTER,
            "no indexed conversations".bright_black()
        ));
    } else {
        let mut table = theme::table(width);
        table.set_header([
            theme::header("title"),
            theme::header("address"),
            theme::header("turns"),
            theme::header("started"),
        ]);
        for row in rows {
            table.add_row([
                Cell::new(
                    row.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("untitled"),
                ),
                Cell::new(row.get("address")?.as_str()?),
                theme::right(row.get("turns")?.as_i64()?.to_string()),
                Cell::new(row.get("started_at")?.as_str()?),
            ]);
        }
        out.push_str(&theme::block(&table));
    }
    append_next(&mut out, envelope, width);
    Some(out)
}

fn conversation_verify(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report = envelope.data.as_ref()?;
    let turns = report.get("turns")?.as_i64()?;
    let mut out = theme::title(
        "conversation verify",
        &format!(
            "{} · {turns} turn{}",
            "delivered".bright_green().bold(),
            if turns == 1 { "" } else { "s" }
        ),
    );
    out.push_str("\n\n");
    let mut facts = theme::plain_table(width);
    for (label, value) in [
        ("guid", report.get("guid")?.as_str()?.to_string()),
        ("address", report.get("address")?.as_str()?.to_string()),
        (
            "repo",
            report
                .get("repo")?
                .as_str()
                .unwrap_or("unanchored")
                .to_string(),
        ),
        (
            "worktree",
            report
                .get("worktree")?
                .as_str()
                .unwrap_or("unanchored")
                .to_string(),
        ),
        (
            "last turn",
            report.get("last_turn_at")?.as_str()?.to_string(),
        ),
    ] {
        facts.add_row([Cell::new(label), Cell::new(value)]);
    }
    out.push_str(&theme::block(&facts));
    append_next(&mut out, envelope, width);
    Some(out)
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
    fn conversation_verify_view_carries_the_consumer_contract() {
        let envelope = serde_json::from_str(
            r#"{"ok":true,"command":"conversation verify","v":1,"data":{"guid":"abc","address":"conv:abc","turns":2,"repo":"git:example/repo","worktree":"/srv/repo","last_turn_at":"2026-09-02T00:00:00Z"}}"#,
        )
        .unwrap();
        let screen = render(&envelope, 100).expect("verify has a human view");
        for expected in [
            "delivered",
            "conv:abc",
            "git:example/repo",
            "2026-09-02T00:00:00Z",
        ] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
        }
    }
}
