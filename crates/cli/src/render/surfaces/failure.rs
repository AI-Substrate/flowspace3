use comfy_table::{Cell, ContentArrangement, Table, presets};
use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde::Deserialize;
use serde_json::Value;

use crate::render::theme;

#[derive(Deserialize)]
struct PartialEvidence {
    label: String,
    #[serde(default)]
    citations: Vec<String>,
    #[serde(default)]
    findings: Vec<String>,
}

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let failure = envelope.error.as_ref()?;
    let mut out = theme::title(
        &envelope.command,
        &format!("{}", "failed".bright_red().bold()),
    );
    out.push_str("\n\n");
    let retry = if failure.retryable {
        format!("{}", "retryable".bright_yellow())
    } else {
        format!("{}", "not retryable".bright_black())
    };
    out.push_str(&format!(
        "{}{}  {retry}\n",
        theme::GUTTER,
        failure.code.bright_black()
    ));
    for line in theme::wrap(&failure.message, usize::from(width) - 4, 0).lines() {
        out.push_str(&format!("{}{line}\n", theme::GUTTER));
    }
    out.push('\n');
    let partial = (envelope.command == "ask")
        .then(|| failure.details.get("evidence"))
        .flatten()
        .and_then(|value| serde_json::from_value::<PartialEvidence>(value.clone()).ok());
    if let Some(partial) = &partial {
        out.push_str(&format!(
            "{}{}\n",
            theme::GUTTER,
            partial.label.bright_yellow().bold()
        ));
        for finding in &partial.findings {
            out.push_str(&format!("{}- {finding}\n", theme::GUTTER));
        }
        if !partial.citations.is_empty() {
            out.push_str(&format!(
                "{}{}\n",
                theme::GUTTER,
                "citations".bright_black()
            ));
            for citation in &partial.citations {
                out.push_str(&format!("{}- {citation}\n", theme::GUTTER));
            }
        }
        out.push('\n');
    }

    let mut fix = Table::new();
    fix.load_style(presets::UTF8_FULL_CONDENSED.with_rounded_corners())
        .force_no_tty()
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(width.saturating_sub(4).max(20))
        .set_header([Cell::new(format!("{}", "fix".bright_yellow().bold()))])
        .add_row([Cell::new(theme::fix_text(&failure.fix))]);
    out.push_str(&theme::block(&fix));

    if !failure.details.is_empty() {
        out.push('\n');
        out.push_str(&format!("{}{}\n", theme::GUTTER, "details".bright_black()));
        let mut details = theme::plain_table(width);
        for (key, value) in &failure.details {
            if key == "evidence" && partial.is_some() {
                continue;
            }
            details.add_row([
                Cell::new(format!("{}", key.bright_black())),
                Cell::new(
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned),
                ),
            ]);
        }
        out.push_str(&theme::block(&details));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_is_the_conclusion() {
        let envelope = serde_json::from_str(
            r#"{"ok":false,"command":"add","v":1,"error":{"code":"FS3-E-X","message":"failed","fix":"run `flowspace3 doctor`","retryable":false}}"#,
        )
        .unwrap();
        let screen = anstream::adapter::strip_str(&render(&envelope, 100).unwrap()).to_string();
        assert!(screen.find("failed").unwrap() < screen.find("flowspace3 doctor").unwrap());
    }

    #[test]
    fn ask_failure_leads_with_labelled_partial_evidence() {
        let envelope = serde_json::from_value(serde_json::json!({
            "ok": false,
            "command": "ask",
            "v": 1,
            "error": {
                "code": "FS3-E-QUERY-ASK-TOKEN-BUDGET",
                "message": "token budget exhausted",
                "fix": "ask a narrower question or raise the budget",
                "retryable": false,
                "details": {
                    "stopped": "token_budget",
                    "evidence": {
                        "label": "partial evidence — no answer was synthesized",
                        "citations": ["el:repo/src/lib.rs::answer"],
                        "findings": ["iteration 1: 1 tool call(s), 1 returned evidence, 0 failed"]
                    }
                }
            }
        }))
        .unwrap();

        let screen = anstream::adapter::strip_str(&render(&envelope, 100).unwrap()).to_string();
        let partial = screen.find("partial evidence").unwrap();
        let citation = screen.find("el:repo/src/lib.rs::answer").unwrap();
        let fix = screen.find("ask a narrower question").unwrap();
        assert!(
            partial < citation && citation < fix,
            "screen was:\n{screen}"
        );
        assert!(
            !screen.contains("{\"citations\""),
            "structured evidence must render readably"
        );
    }
}
