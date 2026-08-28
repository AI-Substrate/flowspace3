//! The presentation boundary: an envelope in, a human screen out.
//!
//! # The invariant this module exists to protect
//!
//! Rendering happens AFTER the envelope is produced, from the envelope's own
//! serialised bytes, and it can never reach back into how those bytes are made.
//! `crates/cli/src/main.rs::emit` serialises first — in both modes, identically
//! — and only then asks this module whether it has a screen for the result. A
//! renderer that wanted a field the envelope does not carry is reporting a gap
//! in the CONTRACT, and the answer is to add it to the envelope for every
//! consumer, never to fetch it on the side for this one.
//!
//! That is why [`render`] takes `&Envelope<Value>` and returns `Option<String>`
//! rather than writing anything itself: it has no IO, no store, and no way to
//! learn a fact an agent would not also receive.
//!
//! # Why a function and not a trait
//!
//! Workshop 001 rule 3 — a trait earns its existence when a second real
//! implementation exists or is firmly planned. There is one renderer and no
//! second one in view; a `Renderer` trait here would be a seam with nothing on
//! the other side of it. If the daemon ever serves pre-rendered text, that is
//! the moment to introduce one.
//!
//! # Declining is a feature
//!
//! `None` means "no screen for this", and the caller prints the JSON instead.
//! Plan 007 covers the POC's four surfaces plus `status`, `add`, `get` and
//! `tree`; everything else — a new verb, a payload shaped differently from what
//! this build expects — degrades to the JSON an agent would have seen anyway.
//! Honest fallthrough, never a blank screen and never a panic.

use fs3_core::envelope::Envelope;
use serde_json::Value;

/// Render `envelope` for a person, or decline and let the JSON through.
///
/// Dispatch is on `(ok, command)` — `ok` is the only discriminator (workshop
/// 004 D1), so a failure renders through the failure surface whatever verb
/// produced it, and the reader learns the error screen once.
///
/// # Phase 1
///
/// This is the frozen seam, and it declines everything: the CLI behaves exactly
/// as it did before plan 007 until unit u-r ports `pocs/human-render` in behind
/// it. The stub is deliberate — the seam is what three parallel coders build
/// against, and it has to exist before they start, not after.
#[must_use]
pub fn render(envelope: &Envelope<Value>) -> Option<String> {
    let _ = envelope;
    None
}
