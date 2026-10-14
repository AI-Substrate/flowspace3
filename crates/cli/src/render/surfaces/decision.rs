//! `decision`: each question, then its answer under it.

use fs3_core::envelope::Envelope;
use fs3_daemon::decisions::{Answer, Question};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::decision::DecisionReport;
use crate::render::theme::{self, GUTTER};

/// Cells in each probability bar.
const BAR: usize = 10;

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report: DecisionReport = serde_json::from_value(envelope.data.clone()?).ok()?;
    let mut summary = format!(
        "{} · {} answer{} · {} ms",
        report.model,
        report.answers.len(),
        if report.answers.len() == 1 { "" } else { "s" },
        report.latency_ms
    );
    if let Some(usage) = &report.usage {
        summary.push_str(&format!(" · {} input tokens", usage.input_tokens));
    }
    let mut out = theme::title("decision", &summary);
    out.push_str("\n\n");

    let text_width = usize::from(width).saturating_sub(GUTTER.len() + 4);
    for (index, answer) in report.answers.iter().enumerate() {
        let question = report.questions.get(index);
        let label = answer
            .name()
            .or_else(|| question.and_then(Question::name))
            .map(|name| format!("{} ", name.bold()))
            .unwrap_or_default();
        let asked = question.map_or("", Question::instructions);
        let heading = theme::wrap(&format!("{label}{asked}"), text_width, 4);
        out.push_str(&format!(
            "{GUTTER}{}  {}\n",
            (index + 1).bright_black(),
            heading.replace('\n', &format!("\n{GUTTER}   "))
        ));
        out.push_str(&answer_lines(answer));
        out.push('\n');
    }

    if let Some(next) = &envelope.next_action {
        out.push_str(&theme::next_action(next, usize::from(width)));
        out.push('\n');
    }
    Some(out)
}

fn answer_lines(answer: &Answer) -> String {
    let indent = format!("{GUTTER}   ");
    match answer {
        Answer::Predicate { probability, .. } => {
            let verdict = if *probability >= 0.5 { "yes" } else { "no" };
            format!(
                "{indent}{} {}  {}\n",
                "→".bright_cyan(),
                verdict.bold(),
                theme::score_meter(*probability, BAR)
            )
        }
        Answer::Choice {
            choice,
            confidence,
            probabilities,
            ..
        } => {
            let mut lines = format!(
                "{indent}{} {}  {}\n",
                "→".bright_cyan(),
                choice.to_string().bold(),
                format!("confidence {confidence:.2}").bright_black()
            );
            let pad = probabilities
                .iter()
                .map(|option| option.value.to_string().chars().count())
                .max()
                .unwrap_or(0);
            for option in probabilities {
                lines.push_str(&format!(
                    "{indent}  {:pad$}  {}\n",
                    option.value.to_string(),
                    theme::score_meter(option.probability, BAR)
                ));
            }
            lines
        }
        Answer::Score {
            score,
            confidence,
            probabilities,
            ..
        } => {
            // The score is a weighted mean of level indices; the nearest level
            // is what a person reads it as.
            let level = probabilities
                .iter()
                .min_by(|a, b| {
                    (f64::from(a.value) - score)
                        .abs()
                        .total_cmp(&(f64::from(b.value) - score).abs())
                })
                .map_or("", |level| level.label.as_str());
            let mut lines = format!(
                "{indent}{} {} {}  {}\n",
                "→".bright_cyan(),
                format!("{score:.2}").bold(),
                level.bold(),
                format!("confidence {confidence:.2}").bright_black()
            );
            let pad = probabilities
                .iter()
                .map(|level| level.label.chars().count())
                .max()
                .unwrap_or(0);
            for level in probabilities {
                lines.push_str(&format!(
                    "{indent}  {:pad$}  {}\n",
                    level.label,
                    theme::score_meter(level.probability, BAR)
                ));
            }
            lines
        }
        Answer::Refusal { .. } => format!(
            "{indent}{} {}\n",
            "✗".bright_red(),
            "refused — the model declined this question".bright_red()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(screen: &str) -> String {
        anstream::adapter::strip_str(screen).to_string()
    }

    #[test]
    fn each_answer_sits_under_its_question() {
        let envelope: Envelope<Value> = serde_json::from_value(serde_json::json!({
            "ok": true, "command": "decision", "v": 1,
            "data": {
                "model": "gpt-6-luna",
                "questions": [
                    {"type": "predicate", "name": "complaint", "instructions": "Is this a complaint?"},
                    {"type": "choice", "instructions": "Which department?",
                     "choices": [{"value": "billing"}, {"value": "other"}]},
                    {"type": "score", "name": "severity", "instructions": "How severe?",
                     "levels": [{"label": "low"}, {"label": "high"}]},
                    {"type": "predicate", "instructions": "Whose fault?"}
                ],
                "answers": [
                    {"type": "predicate", "name": "complaint", "probability": 0.97},
                    {"type": "choice", "name": null, "choice": "billing", "confidence": 0.93,
                     "probabilities": [{"value": "billing", "probability": 0.95}, {"value": "other", "probability": 0.05}]},
                    {"type": "score", "name": "severity", "score": 0.9, "confidence": 0.8,
                     "probabilities": [{"value": 0, "label": "low", "probability": 0.1}, {"value": 1, "label": "high", "probability": 0.9}]},
                    {"type": "refusal", "name": null}
                ],
                "usage": {"input_tokens": 120, "output_tokens": 0, "total_tokens": 120},
                "input": {"texts": 1, "images": 0},
                "endpoint": "https://api.openai.com/v1",
                "latency_ms": 111
            },
            "next_action": "branch on type"
        }))
        .unwrap();

        let screen = plain(&render(&envelope, 100).expect("a covered payload renders"));
        assert!(
            screen.contains("gpt-6-luna · 4 answers · 111 ms · 120 input tokens"),
            "{screen}"
        );
        assert!(
            screen.contains("complaint Is this a complaint?"),
            "{screen}"
        );
        assert!(screen.contains("→ yes  0.97"), "{screen}");
        assert!(screen.contains("→ billing  confidence 0.93"), "{screen}");
        assert!(screen.contains("→ 0.90 high  confidence 0.80"), "{screen}");
        assert!(screen.contains("refused"), "{screen}");
        assert!(screen.contains("branch on type"), "{screen}");
    }

    #[test]
    fn an_unrecognised_payload_declines_to_the_json() {
        let envelope: Envelope<Value> = serde_json::from_value(serde_json::json!({
            "ok": true, "command": "decision", "v": 1, "data": {"answers": "soon"}
        }))
        .unwrap();
        assert_eq!(render(&envelope, 100), None);
    }
}
