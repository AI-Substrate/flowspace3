use comfy_table::{Cell, CellAlignment, ColumnConstraint, Width};
use fs3_core::{
    envelope::Envelope,
    views::search::{SearchChannel, SearchResults},
};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::theme;

const METER_CELLS: usize = 8;

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let results: SearchResults = serde_json::from_value(envelope.data.clone()?).ok()?;
    let count = results.results.len();
    let mut out = theme::title(
        "search",
        &format!("{count} hit{}", if count == 1 { "" } else { "s" }),
    );
    out.push_str("\n\n");
    if results.results.is_empty() {
        out.push_str(&format!(
            "{}{}\n",
            theme::GUTTER,
            "no hits — widen with --limit, drop --min-score, or `--repo all`".bright_black()
        ));
    } else {
        let mut table = theme::table(width);
        table.set_header([
            theme::header("#"),
            theme::header("score"),
            theme::header("kind"),
            theme::header("element"),
            theme::header("tags"),
        ]);
        table
            .column_mut(1)
            .expect("search table has a score column")
            .set_constraint(ColumnConstraint::Absolute(Width::Fixed(15)));
        for (index, hit) in results.results.iter().enumerate() {
            let mut kind = format!("{}", hit.kind.bright_white());
            if !hit.subkind.is_empty() && hit.subkind != hit.kind {
                kind.push_str(&format!("\n{}", hit.subkind.bright_black()));
            }
            let channel = match hit.channel {
                SearchChannel::Lexical => format!("{}", "lexical".green()),
                SearchChannel::Semantic => format!("{}", "semantic".blue()),
                SearchChannel::Both => format!("{}", "both".cyan()),
            };
            kind.push_str(&format!("\n{channel}"));
            if !hit.match_field.is_empty() {
                kind.push_str(&format!(" · {}", hit.match_field.blue()));
            }
            let mut element = format!("{}", hit.name.bold().bright_white());
            element.push_str(&format!(
                "  {}\n{}",
                format!("[{}-{}]", hit.span[0], hit.span[1]).bright_black(),
                hit.address.bright_black()
            ));
            if let Some(blurb) = hit
                .smart
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    hit.snippet
                        .lines()
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                })
            {
                element.push_str(&format!("\n{}", blurb.bright_black()));
            }
            let tags = if hit.tags.is_empty() {
                format!("{}", "—".bright_black())
            } else {
                hit.tags
                    .iter()
                    .map(|tag| format!("{}", tag.cyan()))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            table.add_row([
                Cell::new(index + 1).set_alignment(CellAlignment::Right),
                Cell::new(theme::score_meter(hit.score, METER_CELLS)),
                Cell::new(kind),
                Cell::new(element),
                Cell::new(tags),
            ]);
        }
        out.push_str(&theme::block(&table));
    }
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
    fn channel_and_exact_match_reason_are_visible() {
        let envelope: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"search","v":1,
                "data":{"results":[{"address":"el:a/b.rs::needle","score":1.0,
                    "channel":"both","match_field":"exact_name","kind":"function",
                    "subkind":"function_item","name":"needle","span":[1,1],
                    "snippet":"fn needle() {}","smart":null,"tags":[],"repo":null,
                    "path":null,"worktree":null}]}}"#,
        )
        .unwrap();
        let screen = anstream::adapter::strip_str(&render(&envelope, 120).unwrap()).to_string();
        assert!(screen.contains("both"), "{screen}");
        assert!(screen.contains("exact_name"), "{screen}");
    }
}
