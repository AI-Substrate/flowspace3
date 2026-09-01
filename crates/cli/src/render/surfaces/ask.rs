use fs3_core::envelope::Envelope;
use owo_colors::OwoColorize;
use serde::Deserialize;
use serde_json::Value;

use crate::render::theme;

#[derive(Deserialize)]
struct AskReport {
    answer: Option<String>,
    #[serde(default)]
    citations: Vec<String>,
    grounded: bool,
    #[serde(default)]
    trace: Vec<TraceEntry>,
    #[serde(default)]
    coverage: Coverage,
    iterations: u32,
    tokens_used: Option<u64>,
    stopped: String,
    model: String,
}

#[derive(Default, Deserialize)]
struct Coverage {
    #[serde(default)]
    corpus: CorpusCoverage,
}

#[derive(Default, Deserialize)]
struct CorpusCoverage {
    #[serde(default)]
    source: String,
    conversation: Option<ConversationCoverage>,
    path: Option<PathCoverage>,
}

#[derive(Deserialize)]
struct ConversationCoverage {
    guid: String,
    turns: i64,
}

#[derive(Deserialize)]
struct PathCoverage {
    glob: String,
    elements: i64,
    conversation_exclusion: Option<String>,
}

impl CorpusCoverage {
    fn summary(&self) -> Option<String> {
        if let Some(conversation) = &self.conversation {
            return Some(format!(
                "one conversation of {} turns (conv:{})",
                conversation.turns, conversation.guid
            ));
        }
        if let Some(path) = &self.path {
            let source = match self.source.as_str() {
                "" | "all" => String::new(),
                source => format!("{source} source only · "),
            };
            let suffix = path
                .conversation_exclusion
                .as_ref()
                .map(|reason| format!(" · {reason}"))
                .unwrap_or_default();
            return Some(format!(
                "{source}paths matching {:?} · {} element{}{suffix}",
                path.glob,
                path.elements,
                if path.elements == 1 { "" } else { "s" }
            ));
        }
        match self.source.as_str() {
            "" | "all" => None,
            source => Some(format!("{source} source only")),
        }
    }
}

#[derive(Deserialize)]
struct TraceEntry {
    #[serde(default)]
    tool: String,
    #[serde(default)]
    failed: bool,
    #[serde(default)]
    evidence: bool,
}

#[must_use]
pub fn render(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    let report: AskReport = serde_json::from_value(envelope.data.clone()?).ok()?;
    let tokens = report.tokens_used.map_or_else(
        || "tokens unreported".to_string(),
        |count| format!("{count} tokens"),
    );
    let iterations = format!(
        "{} iteration{}",
        report.iterations,
        if report.iterations == 1 { "" } else { "s" }
    );
    let mut out = theme::title(
        "ask",
        &format!("{} · {iterations} · {tokens}", report.model),
    );
    out.push_str("\n\n");

    if report.stopped != "answered" {
        append_bound_notice(&mut out, &report.stopped);
    } else if let Some(answer) = report.answer.as_deref() {
        if !report.grounded {
            out.push_str(&format!(
                "{}{}\n{}{}\n\n",
                theme::GUTTER,
                "! UNGROUNDED".bright_red().bold(),
                theme::GUTTER,
                "Treat this answer as a guess: the loop read no supporting evidence.".bright_red()
            ));
        }
        append_wrapped(&mut out, answer, width, true);
    } else {
        out.push_str(&format!(
            "{}{}\n",
            theme::GUTTER,
            "no answer was returned".bright_yellow()
        ));
    }

    if !report.citations.is_empty() {
        out.push_str(&format!(
            "\n{}{}\n",
            theme::GUTTER,
            plural(report.citations.len(), "source").bright_black()
        ));
        for citation in &report.citations {
            append_wrapped(&mut out, citation, width, false);
        }
    }

    if let Some(corpus) = report.coverage.corpus.summary() {
        out.push_str(&format!(
            "\n{}{}  {}\n",
            theme::GUTTER,
            "scope".bright_black(),
            corpus.bright_black()
        ));
    }

    out.push_str(&format!(
        "\n{}{}  {}\n",
        theme::GUTTER,
        "work".bright_black(),
        trace_summary(&report.trace).bright_black()
    ));

    if let Some(next) = &envelope.next_action {
        out.push('\n');
        out.push_str(&theme::next_action(next, usize::from(width)));
        out.push('\n');
    }
    Some(out)
}

fn append_bound_notice(out: &mut String, stopped: &str) {
    let (heading, detail) = match stopped {
        "max_iterations" => (
            "! STOPPED — iteration limit reached",
            "No answer was produced before the iteration limit.",
        ),
        "token_budget" => (
            "! STOPPED — token budget exhausted",
            "No answer was produced before the token budget was exhausted.",
        ),
        _ => (
            "! STOPPED — run ended early",
            "No answer was produced before the run ended.",
        ),
    };
    out.push_str(&format!(
        "{}{}\n{}{}\n",
        theme::GUTTER,
        heading.bright_red().bold(),
        theme::GUTTER,
        detail.bright_yellow()
    ));
}

fn append_wrapped(out: &mut String, text: &str, width: u16, hero: bool) {
    let body_width = usize::from(width).saturating_sub(theme::GUTTER.len());
    for source_line in text.lines() {
        if source_line.is_empty() {
            out.push('\n');
            continue;
        }
        for line in theme::wrap(source_line, body_width, 0).lines() {
            out.push_str(theme::GUTTER);
            if hero {
                out.push_str(&format!("{}", line.bright_white()));
            } else {
                out.push_str(&format!("{}", line.bright_black()));
            }
            out.push('\n');
        }
    }
}

fn trace_summary(trace: &[TraceEntry]) -> String {
    if trace.is_empty() {
        return "no tool calls".to_string();
    }

    let searches = trace.iter().filter(|entry| entry.tool == "search").count();
    let reads = trace
        .iter()
        .filter(|entry| matches!(entry.tool.as_str(), "get" | "read" | "tree"))
        .count();
    let other = trace.len().saturating_sub(searches + reads);
    let failed = trace.iter().filter(|entry| entry.failed).count();
    let empty = trace
        .iter()
        .filter(|entry| !entry.failed && !entry.evidence)
        .count();

    let mut parts = Vec::with_capacity(5);
    if searches > 0 {
        parts.push(plural(searches, "search"));
    }
    if reads > 0 {
        parts.push(plural(reads, "read"));
    }
    if other > 0 {
        parts.push(plural(other, "other call"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if empty > 0 {
        parts.push(format!("{empty} found nothing"));
    }
    parts.join(" · ")
}

fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plain(screen: &str) -> String {
        anstream::adapter::strip_str(screen).to_string()
    }

    fn envelope(data: Value) -> Envelope<Value> {
        serde_json::from_value(json!({
            "ok": true,
            "command": "ask",
            "v": 1,
            "data": data
        }))
        .unwrap()
    }

    fn answered(grounded: bool, tokens_used: Value) -> Value {
        json!({
            "answer": "The watcher rescans each changed repository root.",
            "citations": ["el:git:github.com/example/repo/crates/daemon/src/watch.rs::rescan"],
            "grounded": grounded,
            "trace": [
                {"iteration": 1, "tool": "search", "arguments": "{}", "failed": false, "evidence": true, "result_chars": 120},
                {"iteration": 2, "tool": "get", "arguments": "{}", "failed": false, "evidence": true, "result_chars": 900}
            ],
            "iterations": 3,
            "tokens_used": tokens_used,
            "stopped": "answered",
            "model": "stub@1"
        })
    }

    #[test]
    fn grounded_answer_leads_supporting_detail() {
        let screen = plain(&render(&envelope(answered(true, json!(420))), 80).unwrap());
        println!("--- grounded ask ---\n{screen}");
        let answer = screen.find("The watcher rescans").unwrap();
        let sources = screen.find("1 source").unwrap();
        let work = screen.find("work").unwrap();
        assert!(answer < sources && sources < work, "screen was:\n{screen}");
        assert!(screen.contains("1 search · 1 read"));
    }

    #[test]
    fn ungrounded_answer_is_an_unmissable_guess() {
        let screen = plain(&render(&envelope(answered(false, json!(420))), 80).unwrap());
        println!("--- ungrounded ask ---\n{screen}");
        assert!(screen.contains("! UNGROUNDED"));
        assert!(screen.contains("Treat this answer as a guess"));
        assert!(screen.contains("The watcher rescans"));
    }

    #[test]
    fn bounded_runs_name_the_bound_without_inventing_an_answer() {
        for (stopped, expected) in [
            ("max_iterations", "iteration limit reached"),
            ("token_budget", "token budget exhausted"),
        ] {
            let report = json!({
                "answer": null,
                "citations": [],
                "grounded": true,
                "trace": [],
                "iterations": 8,
                "tokens_used": 80000,
                "stopped": stopped,
                "model": "stub@1"
            });
            let screen = plain(&render(&envelope(report), 80).unwrap());
            assert!(screen.contains(expected), "screen was:\n{screen}");
            assert!(screen.contains("No answer was produced"));
            assert!(!screen.contains("The watcher rescans"));
        }
    }

    #[test]
    fn unreported_tokens_are_never_rendered_as_zero() {
        let screen = plain(&render(&envelope(answered(true, Value::Null)), 80).unwrap());
        assert!(screen.contains("tokens unreported"));
        assert!(!screen.contains("0 tokens"));
    }
    #[test]
    fn pinned_conversation_coverage_is_visible() {
        let mut report = answered(true, json!(420));
        report["coverage"] = json!({
            "corpus": {
                "source": "conversation",
                "conversation": {
                    "guid": "11111111-1111-4111-8111-111111111111",
                    "count": 1,
                    "turns": 42
                }
            }
        });
        let screen = plain(&render(&envelope(report), 100).unwrap());
        assert!(screen.contains("one conversation of 42 turns"), "{screen}");
        assert!(screen.contains("conv:11111111-1111-4111-8111-111111111111"));
    }

    #[test]
    fn path_coverage_names_the_boundary_and_only_relevant_exclusion() {
        let mut report = answered(true, json!(420));
        report["coverage"] = json!({
            "corpus": {
                "source": "all",
                "path": {
                    "glob": "crates/store/**",
                    "elements": 42,
                    "conversation_exclusion": "conversations carry no file path, so --path excludes them"
                }
            }
        });
        let screen = plain(&render(&envelope(report), 120).unwrap());
        assert!(
            screen.contains("paths matching \"crates/store/**\" · 42 elements"),
            "{screen}"
        );
        assert!(
            screen.contains("conversations carry no file path"),
            "{screen}"
        );

        let mut code = answered(true, json!(420));
        code["coverage"] = json!({
            "corpus": {
                "source": "code",
                "path": { "glob": "src/**", "elements": 1 }
            }
        });
        let screen = plain(&render(&envelope(code), 100).unwrap());
        assert!(screen.contains("code source only · paths matching \"src/**\" · 1 element"));
        assert!(!screen.contains("conversations carry no file path"));
    }
}
