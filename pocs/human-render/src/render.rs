//! The dispatcher: one envelope in, one screen out.
//!
//! # The dispatch rule
//!
//! `ok` first (it is the ONLY discriminator, workshop 004 D1), then `command`.
//! A failure is rendered by the failure surface no matter which verb produced
//! it, which is why the error screen looks the same from `search` and from
//! `add` — the reader learns the shape once.
//!
//! # Unknown verbs must render
//!
//! A CLI built from this renderer will one day be pointed at a NEWER daemon
//! that answers a verb this binary has never heard of. The contract already
//! anticipates that (`v` bumps only on envelope breaks; payloads grow
//! additively), so the renderer honours it: an unrecognised `command`, or a
//! payload that does not fit the shape this build expects, falls through to
//! [`surfaces::generic`] — a titled, indented dump of what actually arrived.
//! Less pretty, never a blank screen, never a panic.

use fs3_core::envelope::Envelope;
use serde_json::Value;

use crate::surfaces;

/// Everything the renderer needs to know about the canvas.
///
/// Deliberately small, and deliberately not self-discovered: this crate never
/// looks at the terminal (see [`crate::mode`]), so a caller that wants
/// terminal-width tables passes the width it measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Total columns available, including the gutter.
    pub width: u16,
}

/// A width that reads well in a default 80-column terminal and in a captured
/// transcript, without forcing either.
pub const DEFAULT_WIDTH: u16 = 100;

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            width: DEFAULT_WIDTH,
        }
    }
}

impl RenderOptions {
    /// A canvas of exactly this many columns.
    #[must_use]
    pub fn width(width: u16) -> Self {
        // Below ~40 columns a bordered table degrades into a column of single
        // characters; clamping is kinder than honouring the request.
        RenderOptions {
            width: width.max(40),
        }
    }
}

/// Render one envelope.
///
/// Always returns styled text; whether the styling survives is the stream's
/// decision ([`crate::mode`]).
#[must_use]
pub fn render(envelope: &Envelope<Value>, options: &RenderOptions) -> String {
    if !envelope.ok {
        return surfaces::failure::render(envelope, options);
    }

    match envelope.command.as_str() {
        "search" => surfaces::search::render(envelope, options),
        "doctor" => surfaces::doctor::render(envelope, options),
        "status" => surfaces::status::render(envelope, options),
        _ => surfaces::generic::render(envelope, options),
    }
}

/// Parse bytes as an envelope and render them.
///
/// # Errors
/// Returns the serde error when the bytes are not an envelope at all — a daemon
/// that died mid-answer, a proxy that injected HTML, a truncated pipe. The
/// renderer refuses to guess what was meant: the caller owns that decision,
/// because only the caller knows what it asked for.
pub fn render_bytes(bytes: &[u8], options: &RenderOptions) -> Result<String, serde_json::Error> {
    let envelope: Envelope<Value> = serde_json::from_slice(bytes)?;
    Ok(render(&envelope, options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn an_unknown_verb_still_renders_its_payload() {
        let json = r#"{"ok":true,"command":"tree","v":1,
                       "data":{"address":"el:fs3/src/lib.rs","children":2}}"#;
        let screen = render_bytes(json.as_bytes(), &RenderOptions::default()).unwrap();
        let screen = plain(&screen);
        assert!(screen.contains("tree"), "{screen}");
        assert!(screen.contains("el:fs3/src/lib.rs"), "{screen}");
    }

    #[test]
    fn a_payload_that_does_not_fit_the_expected_shape_degrades_instead_of_dying() {
        // `results` as a string is not what this build expects from `search`.
        let json = r#"{"ok":true,"command":"search","v":1,"data":{"results":"soon"}}"#;
        let screen = plain(&render_bytes(json.as_bytes(), &RenderOptions::default()).unwrap());
        assert!(screen.contains("search"), "{screen}");
        assert!(screen.contains("soon"), "{screen}");
    }

    #[test]
    fn a_failure_renders_as_a_failure_whatever_the_verb() {
        let json = r#"{"ok":false,"command":"add","v":1,
                       "error":{"code":"FS3-E-SCAN-ROOT-NOT-FOUND",
                                "message":"/srv/nope does not exist",
                                "fix":"pass a path that exists",
                                "retryable":false}}"#;
        let screen = plain(&render_bytes(json.as_bytes(), &RenderOptions::default()).unwrap());
        assert!(screen.contains("FS3-E-SCAN-ROOT-NOT-FOUND"), "{screen}");
        assert!(screen.contains("pass a path that exists"), "{screen}");
    }

    #[test]
    fn bytes_that_are_not_an_envelope_are_an_error_not_a_guess() {
        let err = render_bytes(b"<html>502 Bad Gateway</html>", &RenderOptions::default());
        assert!(err.is_err());
    }

    #[test]
    fn a_narrow_width_is_clamped_rather_than_honoured() {
        assert_eq!(RenderOptions::width(12).width, 40);
    }
}
