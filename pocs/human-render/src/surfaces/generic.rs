//! The fallback: a verb this build has never heard of, or a payload that does
//! not fit the shape it expected.
//!
//! # Why this exists at all
//!
//! Because the envelope's compatibility promise runs in both directions. `v`
//! bumps only when the ENVELOPE breaks, and a verb's `data` grows fields
//! additively — which means an older CLI pointed at a newer daemon is a
//! SUPPORTED configuration, not an error case. This surface is what makes that
//! true on screen: the envelope's own fields (`command`, `ok`, `next_action`)
//! render exactly as they always do, and the payload is shown as it arrived.
//!
//! It is deliberately plain. A fallback that tried to guess at structure would
//! occasionally guess well and permanently remove the incentive to write the
//! real surface.

use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::render::RenderOptions;
use crate::theme::{self, GUTTER};

/// Render any envelope, without understanding it.
#[must_use]
pub fn render(envelope: &Envelope<Value>, options: &RenderOptions) -> String {
    let status = if envelope.ok {
        format!("{}", "ok".green())
    } else {
        format!("{}", "failed".bright_red().bold())
    };
    let mut out = theme::title(
        &envelope.command,
        &format!("{status}{} v{}", " ·".bright_black(), envelope.v),
    );
    out.push_str("\n\n");

    if let Some(failure) = &envelope.error {
        out.push_str(&format!(
            "{GUTTER}{}\n{GUTTER}{}\n",
            failure.code.bright_red(),
            failure.message.bright_white()
        ));
        out.push('\n');
    }

    for (label, value) in [("data", &envelope.data), ("meta", &envelope.meta)] {
        if let Some(value) = value {
            out.push_str(&format!("{GUTTER}{}\n", label.bright_black()));
            out.push_str(&indented_json(value));
            out.push('\n');
        }
    }

    if let Some(next) = &envelope.next_action {
        out.push_str(&theme::next_action(next, options.width as usize));
        out.push('\n');
    }
    out
}

/// Pretty JSON, pushed in under the gutter and dimmed.
fn indented_json(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "<unrenderable payload>".to_string());
    text.lines()
        .map(|line| format!("{GUTTER}{GUTTER}{}\n", line.bright_black()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn a_future_verb_renders_its_envelope_fields_and_its_payload() {
        let future: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"related","v":1,
                "data":{"neighbours":[{"address":"el:fs3/a.rs::b","hops":2}]},
                "meta":{"strategy":"co-change"},
                "next_action":"`flowspace3 get el:fs3/a.rs::b`"}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&future, &RenderOptions::default()));
        assert!(screen.contains("related"), "{screen}");
        assert!(screen.contains("el:fs3/a.rs::b"), "{screen}");
        assert!(screen.contains("co-change"), "{screen}");
        assert!(screen.contains("flowspace3 get"), "{screen}");
    }

    #[test]
    fn an_envelope_with_a_field_this_build_never_heard_of_still_renders() {
        // Forward compatibility, proven: `trace_id` is not in the struct.
        let newer: Envelope<Value> = serde_json::from_str(
            r#"{"ok":true,"command":"ping","v":1,"data":{"pong":true},
                "trace_id":"01J8ZK"}"#,
        )
        .unwrap();
        let screen = plain(&render::render(&newer, &RenderOptions::default()));
        assert!(screen.contains("ping"), "{screen}");
        assert!(screen.contains("pong"), "{screen}");
    }
}
