//! Human presentation over the frozen JSON envelope.
//!
//! The renderer receives one already-produced envelope and performs no data IO
//! or side lookup. Returning `None` is an explicit decline: the caller prints
//! the serialized JSON bytes instead.

use std::io::IsTerminal;

use fs3_core::envelope::Envelope;
use serde_json::Value;

pub mod progress;
mod surfaces;
mod theme;

/// Deterministic fallback when stdout is not a terminal.
const DEFAULT_WIDTH: u16 = 100;
/// Below this, bordered tables collapse into single-character columns.
const MIN_WIDTH: u16 = 40;
/// Wider tables add eye travel without improving scanability.
const MAX_WIDTH: u16 = 160;

/// Render a covered envelope for a person.
#[must_use]
pub fn render(envelope: &Envelope<Value>) -> Option<String> {
    render_at_width(envelope, canvas_width())
}

fn canvas_width() -> u16 {
    if std::io::stdout().is_terminal() {
        normalize_width(Some(textwrap::termwidth()))
    } else {
        normalize_width(None)
    }
}

fn normalize_width(width: Option<usize>) -> u16 {
    width.map_or(DEFAULT_WIDTH, |width| {
        u16::try_from(width.clamp(usize::from(MIN_WIDTH), usize::from(MAX_WIDTH)))
            .unwrap_or(DEFAULT_WIDTH)
    })
}

fn render_at_width(envelope: &Envelope<Value>, width: u16) -> Option<String> {
    if !envelope.ok {
        return surfaces::failure::render(envelope, width);
    }

    match envelope.command.as_str() {
        "ask" => surfaces::ask::render(envelope, width),
        "search" => surfaces::search::render(envelope, width),
        "status" => surfaces::status::render(envelope, width),
        "doctor" => surfaces::doctor::render(envelope, width),
        "add" | "scan" => surfaces::roots::render(envelope, width),
        "get" => surfaces::read::get(envelope, width),
        "tree" => surfaces::read::tree(envelope, width),
        "remove" => surfaces::remove::remove(envelope, width),
        "gc" => surfaces::remove::gc(envelope, width),
        "conversation list" | "conversation verify" | "docs" | "agents-start-here" => {
            surfaces::lists::render(envelope, width)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(screen: &str) -> String {
        anstream::adapter::strip_str(screen).to_string()
    }

    #[test]
    fn unknown_commands_decline_to_the_json_path() {
        let envelope =
            serde_json::from_str(r#"{"ok":true,"command":"future","v":1,"data":{"answer":42}}"#)
                .unwrap();
        assert_eq!(render(&envelope), None);
    }

    #[test]
    fn malformed_covered_payloads_decline_instead_of_guessing() {
        let envelope = serde_json::from_str(
            r#"{"ok":true,"command":"search","v":1,"data":{"results":"soon"}}"#,
        )
        .unwrap();
        assert_eq!(render(&envelope), None);
    }

    #[test]
    fn failure_dispatch_uses_ok_before_command() {
        let envelope = serde_json::from_str(
            r#"{"ok":false,"command":"future","v":1,"error":{"code":"FS3-E-X","message":"no","fix":"do this","retryable":false}}"#,
        )
        .unwrap();
        let screen = plain(&render(&envelope).unwrap());
        assert!(screen.contains("FS3-E-X"));
        assert!(screen.contains("do this"));
    }

    #[test]
    fn canvas_width_is_bounded_and_has_a_deterministic_fallback() {
        assert_eq!(normalize_width(None), DEFAULT_WIDTH);
        assert_eq!(normalize_width(Some(20)), MIN_WIDTH);
        assert_eq!(normalize_width(Some(400)), MAX_WIDTH);
        assert_eq!(normalize_width(Some(72)), 72);
    }

    #[test]
    fn a_sixty_column_failure_stays_inside_its_canvas() {
        let envelope = serde_json::from_str(
            r#"{"ok":false,"command":"add","v":1,"error":{"code":"FS3-E-SCAN-ROOT-NOT-FOUND","message":"the requested repository path does not exist on the daemon host","fix":"pass a path that exists and run `flowspace3 add` again","retryable":false}}"#,
        )
        .unwrap();
        let screen = plain(&render_at_width(&envelope, 60).unwrap());
        assert!(
            screen.lines().all(|line| line.chars().count() <= 60),
            "screen exceeded 60 columns:\n{screen}"
        );
    }
}
