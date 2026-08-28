//! Human presentation over the frozen JSON envelope.
//!
//! The renderer receives one already-produced envelope and performs no IO or
//! side lookup. Returning `None` is an explicit decline: the caller prints the
//! serialized JSON bytes instead.

use fs3_core::envelope::Envelope;
use serde_json::Value;

pub mod progress;
mod surfaces;
mod theme;

/// Fixed canvas for deterministic output and tests. Width is presentation, not
/// domain truth; the frozen renderer seam deliberately carries only an envelope.
pub(crate) const WIDTH: u16 = 100;

/// Render a covered envelope for a person.
#[must_use]
pub fn render(envelope: &Envelope<Value>) -> Option<String> {
    if !envelope.ok {
        return surfaces::failure::render(envelope);
    }

    match envelope.command.as_str() {
        "search" => surfaces::search::render(envelope),
        "status" => surfaces::status::render(envelope),
        "doctor" => surfaces::doctor::render(envelope),
        "add" | "scan" => surfaces::roots::render(envelope),
        "get" => surfaces::read::get(envelope),
        "tree" => surfaces::read::tree(envelope),
        "remove" => surfaces::remove::remove(envelope),
        "gc" => surfaces::remove::gc(envelope),
        "conversation list" | "docs" | "agents-start-here" => surfaces::lists::render(envelope),
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
}
