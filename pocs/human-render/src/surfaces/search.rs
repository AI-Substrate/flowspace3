//! `search` — the ranked table, and the folder steer under it.
//!
//! # What the eye should do
//!
//! Top to bottom: how many hits there were and what narrowed them, then the
//! ranked rows, then WHERE the answers cluster, then the one command to run
//! next. A reader who stops after the first row has the answer; a reader who
//! stops after the folder steer knows which part of the codebase owns the
//! subject, which is the question behind the question.
//!
//! # Why the address is printed in full
//!
//! Addresses are the only id surface (workshop 003 D7) and the input to `get`.
//! Truncating one to fit a column would produce a beautiful table you cannot
//! copy out of. It is dimmed instead: present, precise, and skipped by the eye
//! on the way to the name.

use comfy_table::{Cell, CellAlignment, ColumnConstraint, Width};
use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::RenderOptions;
use crate::surfaces::generic;
use crate::theme::{self, GUTTER};
use crate::views::{Hit, SearchMeta, SearchResults};

/// How many cells the score meter gets.
const METER_CELLS: usize = 8;

/// Render a `search` envelope.
#[must_use]
pub fn render(envelope: &Envelope<Value>, options: &RenderOptions) -> String {
    let Some(results) = envelope
        .data
        .clone()
        .and_then(|data| serde_json::from_value::<SearchResults>(data).ok())
    else {
        return generic::render(envelope, options);
    };
    let meta: SearchMeta = envelope
        .meta
        .clone()
        .and_then(|meta| serde_json::from_value(meta).ok())
        .unwrap_or_default();

    let mut out = String::new();
    out.push_str(&theme::title(
        "search",
        &summary(&meta, results.results.len()),
    ));
    out.push('\n');

    if let Some(filters) = filters_line(&meta) {
        out.push_str(&filters);
        out.push('\n');
    }
    out.push('\n');

    if results.results.is_empty() {
        // An empty result set is an ANSWER, not an error: say so, and say what
        // would widen it, rather than printing an empty table.
        out.push_str(&format!(
            "{GUTTER}{}\n",
            "no hits — widen with --limit, drop --min-score, or `--repo all`".bright_black()
        ));
    } else {
        out.push_str(&hits_table(&results.results, options));
        out.push('\n');
    }

    if let Some(folders) = folders_block(&meta, options) {
        out.push('\n');
        out.push_str(&folders);
    }

    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, options.width as usize));
        out.push('\n');
    }
    out
}

/// `143 hits · showing 1-6 · semantic/rrf · 84ms`, from whatever meta carried.
fn summary(meta: &SearchMeta, shown: usize) -> String {
    let mut parts = Vec::new();
    let counted = meta.total.unwrap_or(shown as u64);
    parts.push(format!(
        "{counted} hit{}",
        if counted == 1 { "" } else { "s" }
    ));
    if let Some(showing) = meta.showing {
        let from = showing.from + 1;
        let to = showing.from + showing.count;
        parts.push(format!("showing {from}-{to}"));
    }
    match (&meta.mode, &meta.rank) {
        (Some(mode), Some(rank)) => parts.push(format!("{mode}/{rank}")),
        (Some(mode), None) => parts.push(mode.clone()),
        (None, Some(rank)) => parts.push(rank.clone()),
        (None, None) => {}
    }
    if let Some(took) = meta.took_ms {
        parts.push(format!("{took}ms"));
    }
    parts.join(" · ")
}

/// `filters: repo=flowspace3 source=all limit=6` — why this set and no other.
fn filters_line(meta: &SearchMeta) -> Option<String> {
    if meta.filters_applied.is_empty() {
        return None;
    }
    let rendered = meta
        .filters_applied
        .iter()
        .map(|(key, value)| format!("{key}={}", scalar(value)))
        .collect::<Vec<_>>()
        .join(" ");
    Some(theme::kv(
        "filters",
        &format!("{}", rendered.bright_black()),
    ))
}

/// JSON scalars read better unquoted; anything else keeps its JSON spelling.
fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The ranked rows.
fn hits_table(hits: &[Hit], options: &RenderOptions) -> String {
    let mut table = theme::table(options.width);
    table.set_header(vec![
        theme::header_cell("#"),
        theme::header_cell("score"),
        theme::header_cell("kind"),
        theme::header_cell("element"),
        theme::header_cell("tags"),
    ]);

    // The score column is fixed: a meter that changes width row to row is a
    // chart that lies. Everything else gives way to the element column, which
    // is where the reading actually happens.
    table
        .column_mut(1)
        .expect("the header defined five columns")
        .set_constraint(ColumnConstraint::Absolute(Width::Fixed(
            u16::try_from(METER_CELLS).unwrap_or(8) + 7,
        )));

    for (index, hit) in hits.iter().enumerate() {
        table.add_row(vec![
            Cell::new(format!("{}", (index + 1).bright_black()))
                .set_alignment(CellAlignment::Right),
            Cell::new(theme::score_meter(hit.score, METER_CELLS)),
            Cell::new(kind_cell(hit)),
            Cell::new(element_cell(hit)),
            Cell::new(tags_cell(hit)),
        ]);
    }
    theme::block(&table)
}

/// `function` over a dim `fn`, with the winning vector space beneath it.
fn kind_cell(hit: &Hit) -> String {
    let mut cell = String::new();
    cell.push_str(&format!("{}", hit.kind.bright_white()));
    if !hit.subkind.is_empty() && hit.subkind != hit.kind {
        cell.push_str(&format!("\n{}", hit.subkind.bright_black()));
    }
    if !hit.match_field.is_empty() {
        // Which space won is a real signal: a `smart` win means the SUMMARY
        // matched, so the code itself may not contain the words searched for.
        let field = match hit.match_field.as_str() {
            "smart" => format!("{}", hit.match_field.magenta()),
            _ => format!("{}", hit.match_field.blue()),
        };
        cell.push_str(&format!("\n{field}"));
    }
    cell
}

/// Name, then the copy-pasteable address and span, then the one line that says
/// what this thing is.
fn element_cell(hit: &Hit) -> String {
    // Name and span together: the two facts that identify the thing to a human
    // ("`migrate`, eight lines, in this file"). The address below them is the
    // machine's name for the same thing, kept whole for copy-paste.
    let mut cell = format!("{}", hit.name.bold().bright_white());
    if let Some([start, end]) = hit.span {
        cell.push_str(&format!("{}", format!("  [{start}-{end}]").bright_black()));
    }
    cell.push_str(&format!("\n{}", hit.address.bright_black()));

    // The summary is what a human wants; the snippet is the fallback when the
    // element has not been summarised yet. Never both — this is a ranked list,
    // not a reading view. `get` is the reading view.
    let blurb = match &hit.smart {
        Some(smart) if !smart.trim().is_empty() => Some(smart.trim().to_string()),
        _ => hit
            .snippet
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(ToString::to_string),
    };
    if let Some(blurb) = blurb {
        cell.push_str(&format!("\n{}", blurb.bright_black()));
    }
    cell
}

/// Tags, one per line — a wrapped comma list is unreadable at this width.
fn tags_cell(hit: &Hit) -> String {
    if hit.tags.is_empty() {
        return format!("{}", "—".bright_black());
    }
    hit.tags
        .iter()
        .map(|tag| format!("{}", tag.cyan()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `meta.folders` — the agent steer, which is a human steer too.
fn folders_block(meta: &SearchMeta, options: &RenderOptions) -> Option<String> {
    if meta.folders.is_empty() {
        return None;
    }
    let peak = meta.folders.values().copied().max().unwrap_or(1).max(1);

    // Ranked by count, not alphabetically: the steer IS the ordering.
    let mut folders: Vec<(&String, &u64)> = meta.folders.iter().collect();
    folders.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let mut table = theme::plain_table(options.width);
    for (folder, count) in folders {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "folder counts are display-scale; a page cannot hold i64::MAX rows"
        )]
        let bar = theme::meter(*count as i64, peak as i64, 12);
        table.add_row(vec![
            Cell::new(format!("{}", folder.bright_white())),
            Cell::new(bar),
            theme::right(format!("{}", count.bright_cyan())),
        ]);
    }
    Some(format!(
        "{GUTTER}{}\n{}",
        "where the answers live".bright_black(),
        theme::block(&table)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;

    fn fixture() -> Envelope<Value> {
        let bytes = include_bytes!("../../fixtures/search.json");
        serde_json::from_slice(bytes).expect("the search fixture is a valid envelope")
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn the_ranked_rows_carry_address_score_kind_span_tags_and_a_blurb() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(
            screen.contains("el:flowspace3/crates/store/src/lib.rs::migrate"),
            "{screen}"
        );
        assert!(screen.contains("0.83"), "{screen}");
        assert!(screen.contains("function"), "{screen}");
        assert!(screen.contains("[90-97]"), "{screen}");
        assert!(screen.contains("migrations"), "{screen}");
        assert!(screen.contains("forward-only"), "{screen}");
    }

    #[test]
    fn rank_order_is_the_daemons_order_not_the_renderers() {
        // Wide canvas: at 100 columns the longest addresses wrap mid-token,
        // which is correct rendering but makes a substring search meaningless.
        let screen = plain(&render::render(&fixture(), &RenderOptions::width(160)));
        let first = screen.find("::migrate").expect("row 1");
        let last = screen.find("::claim_batch").expect("row 6");
        assert!(first < last, "the renderer must never re-sort the ranking");
    }

    #[test]
    fn the_folder_steer_is_ordered_by_count() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        let steer = screen
            .split("where the answers live")
            .nth(1)
            .expect("the folder steer block");
        let store = steer.find("crates/store").expect("store folder");
        let cli = steer.find("crates/cli").expect("cli folder");
        assert!(store < cli, "3 must outrank 1:\n{steer}");
    }

    #[test]
    fn an_unsummarised_hit_falls_back_to_its_snippet() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(screen.contains("CREATE TABLE smart_content ("), "{screen}");
    }

    #[test]
    fn no_hits_says_what_would_widen_the_search() {
        let empty: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"search","v":1,"data":{"results":[]},
                "meta":{"total":0}}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&empty, &RenderOptions::default()));
        assert!(screen.contains("no hits"), "{screen}");
        assert!(screen.contains("--min-score"), "{screen}");
    }

    #[test]
    fn a_search_with_no_meta_at_all_still_renders_its_rows() {
        // `meta` is never load-bearing by contract; prove it.
        let bare: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"search","v":1,
                "data":{"results":[{"address":"el:a/b.rs::c","score":0.5,"kind":"function",
                                    "name":"c","snippet":"fn c() {}"}]}}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&bare, &RenderOptions::default()));
        assert!(screen.contains("el:a/b.rs::c"), "{screen}");
        assert!(screen.contains("1 hit"), "{screen}");
        assert!(
            !screen.contains("1 hits"),
            "singular, not `1 hits`:\n{screen}"
        );
        assert!(!screen.contains("where the answers live"), "{screen}");
    }
}
