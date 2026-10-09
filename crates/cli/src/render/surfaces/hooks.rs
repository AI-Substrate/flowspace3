//! `hooks install`, `hooks uninstall` and `hooks status` for a person.

use comfy_table::Cell;
use fs3_core::envelope::Envelope;
use serde_json::Value;

use crate::render::theme;

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The files an install or uninstall looked at, and what it did to each.
#[must_use]
pub fn changes(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let data = envelope.data.as_ref()?;
    let rows = data.get("changes")?.as_array()?;
    let changed = rows
        .iter()
        .filter(|r| matches!(text(r, "action"), "created" | "updated" | "removed"))
        .count();
    let mut out = theme::title(&envelope.command, &format!("{changed} file(s) changed"));
    out.push_str("\n\n");
    let mut table = theme::table(width);
    table.set_header(["harness", "action", "file"].map(theme::header));
    for row in rows {
        let mut file = text(row, "path").to_string();
        if let Some(backup) = row.get("backup").and_then(Value::as_str) {
            file.push_str(&format!("\nbackup: {backup}"));
        }
        if let Some(detail) = row.get("detail").and_then(Value::as_str) {
            file.push_str(&format!("\n{detail}"));
        }
        table.add_row([
            Cell::new(text(row, "harness")),
            Cell::new(text(row, "action")),
            Cell::new(file),
        ]);
    }
    out.push_str(&theme::block(&table));
    append_next(&mut out, envelope, width);
    Some(out)
}

/// Each harness's hook, per scope.
#[must_use]
pub fn status(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let data = envelope.data.as_ref()?;
    let rows = data.get("hooks")?.as_array()?;
    let mut out = theme::title("hooks status", text(data, "binary"));
    out.push_str("\n\n");
    let mut table = theme::table(width);
    table.set_header(["harness", "scope", "state", "file"].map(theme::header));
    for row in rows {
        let detected = row
            .get("detected")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !detected && text(row, "state") == "missing" {
            continue; // a harness this user does not have, with nothing installed
        }
        let mut file = text(row, "path").to_string();
        if let Some(detail) = row.get("detail").and_then(Value::as_str) {
            file.push_str(&format!("\n{detail}"));
        }
        table.add_row([
            Cell::new(text(row, "harness")),
            Cell::new(text(row, "scope")),
            Cell::new(text(row, "state")),
            Cell::new(file),
        ]);
    }
    out.push_str(&theme::block(&table));
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
