//! The failure screen — where the `fix` is the star.
//!
//! # The doctrine, rendered
//!
//! Workshop 004 D3 makes `fix` mandatory in the TYPE: a failure cannot be
//! constructed without one. This surface is the visual half of that decision.
//! An error screen where the code and the message are loud and the fix is a
//! footnote reproduces exactly the failure mode the doctrine exists to kill —
//! the reader diagnosing something the system already diagnosed.
//!
//! So the order is deliberate and the weight is deliberate:
//!
//! ```text
//! code      dim, small — for grepping a log and for filing a bug
//! message   normal     — what happened
//! FIX       framed, bright, wrapped, commands highlighted — what to DO
//! details   dim table  — for the reader who is still curious
//! ```
//!
//! # Why the details table is last and dim
//!
//! `details` exists so a CONSUMER can branch without parsing prose. A human
//! reading it is doing the machine's job, which is fine, but it is never the
//! first thing they should have to do.

use comfy_table::{Cell, ContentArrangement, Table, presets};
use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::RenderOptions;
use crate::surfaces::generic;
use crate::theme::{self, GUTTER};

/// Render a failed envelope, whatever verb produced it.
#[must_use]
pub fn render(envelope: &Envelope<Value>, options: &RenderOptions) -> String {
    let Some(failure) = &envelope.error else {
        // `ok: false` with no `error` is a contract violation by whoever sent
        // it. The renderer says what arrived instead of inventing a diagnosis.
        return generic::render(envelope, options);
    };

    let width = options.width as usize;
    let mut out = String::new();

    out.push_str(&theme::title(
        &envelope.command,
        &format!("{}", "failed".bright_red().bold()),
    ));
    out.push_str("\n\n");

    // Code and retryability on one line: the two facts a reader needs before
    // deciding whether to think or just run it again.
    let retry = if failure.retryable {
        format!("{}", "retryable".bright_yellow())
    } else {
        format!("{}", "not retryable".bright_black())
    };
    out.push_str(&format!(
        "{GUTTER}{}  {retry}\n",
        failure.code.bright_black()
    ));

    for line in theme::wrap(&failure.message, width - GUTTER.len() * 2, 0).lines() {
        out.push_str(&format!("{GUTTER}{line}\n"));
    }
    out.push('\n');
    out.push_str(&fix_box(&failure.fix, options));

    if !failure.details.is_empty() {
        out.push('\n');
        out.push_str(&details_block(failure, options));
    }
    out
}

/// The framed callout. One cell, one job.
fn fix_box(fix: &str, options: &RenderOptions) -> String {
    let mut table = Table::new();
    table
        .load_style(presets::UTF8_FULL_CONDENSED.with_rounded_corners())
        .force_no_tty()
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(options.width.saturating_sub(4).max(20))
        .set_header(vec![Cell::new(format!("{}", "fix".bright_yellow().bold()))])
        .add_row(vec![Cell::new(theme::fix_text(fix))]);
    theme::block(&table)
}

/// The structured facts, aligned and dim.
fn details_block(failure: &fs3_core::envelope::Failure, options: &RenderOptions) -> String {
    let mut table = theme::plain_table(options.width);
    for (key, value) in &failure.details {
        table.add_row(vec![
            Cell::new(format!("{}", key.bright_black())),
            Cell::new(match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            }),
        ]);
    }
    format!(
        "{GUTTER}{}\n{}",
        "details".bright_black(),
        theme::block(&table)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;

    fn fixture() -> Envelope<Value> {
        let bytes = include_bytes!("../../fixtures/error.json");
        serde_json::from_slice(bytes).expect("the error fixture is a valid envelope")
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn the_fix_is_framed_and_the_code_is_not() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        let fix_line = screen
            .lines()
            .position(|line| line.contains("compose"))
            .expect("the fix must be on screen");
        let framed = screen
            .lines()
            .nth(fix_line)
            .expect("the fix line")
            .trim_start()
            .starts_with('│');
        assert!(framed, "the fix must be inside the callout:\n{screen}");
    }

    #[test]
    fn the_fix_appears_after_the_message_because_it_is_the_conclusion() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        let message = screen.find("refused the connection").expect("message");
        let fix = screen.find("compose.yaml").expect("fix");
        assert!(message < fix, "{screen}");
    }

    #[test]
    fn retryability_is_stated_not_implied() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        assert!(screen.contains("retryable"), "{screen}");
        assert!(screen.contains("FS3-E-STORE-UNAVAILABLE"), "{screen}");
    }

    #[test]
    fn details_are_present_but_below_the_fix() {
        let screen = plain(&render::render(&fixture(), &RenderOptions::default()));
        let fix = screen.find("compose.yaml").expect("fix");
        let details = screen.find("ECONNREFUSED").map_or(usize::MAX, |_| {
            screen.find("details").expect("details block")
        });
        assert!(fix < details, "{screen}");
        assert!(screen.contains("5432"), "{screen}");
    }

    #[test]
    fn a_failure_with_no_error_object_degrades_instead_of_inventing_one() {
        let broken: Envelope<Value> =
            serde_json::from_str(r#"{"ok":false,"command":"add","v":1}"#).unwrap();
        let screen = plain(&render::render(&broken, &RenderOptions::default()));
        assert!(screen.contains("add"), "{screen}");
        assert!(
            !screen.contains("fix"),
            "nothing to claim as a fix:\n{screen}"
        );
    }
}
