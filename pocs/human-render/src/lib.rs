//! human-render — the HUMAN skin over the frozen fs3 envelope.
//!
//! v1 of flowspace3 answers in JSON only (workshop 003 D5). This prototype is
//! the fast-follow: the same bytes, rendered for a person at a terminal, in the
//! spirit of Python's `rich`.
//!
//! # The one architectural rule
//!
//! **Two skins, one truth.** The renderer's input is an [`Envelope`] and
//! nothing else — no daemon handle, no store, no second code path that knows
//! something the JSON did not say. Everything on screen is derivable from what
//! an agent piping `--json` already receives, which means the human view can
//! never drift ahead of the machine view: a field that a human needs is a field
//! the CONTRACT is missing, and the fix belongs in workshop 004, not here.
//!
//! That rule is enforced by the dependency graph rather than by discipline.
//! This crate depends on `fs3-core` for [`Envelope`] and
//! [`Failure`](fs3_core::envelope::Failure) — the real ones, by path — and on
//! nothing else from flowspace3. It cannot reach a database if it wants to.
//!
//! # Shape
//!
//! ```text
//! bytes ──serde──▶ Envelope<Value> ──▶ render() ──▶ String (always styled)
//!                                          │
//!                       dispatch on (ok, command)
//!                       ├── search  → ranked table + folder steer
//!                       ├── doctor  → found→did checklist
//!                       ├── status  → roots + queue depth
//!                       ├── ok=false→ the FIX, made primary
//!                       └── unknown → generic, never a panic
//! ```
//!
//! # Colour is not the renderer's decision
//!
//! [`render`] always emits ANSI. Whether those sequences reach a screen is
//! decided once, at the boundary, by [`anstream`] — which already knows about
//! `NO_COLOR`, `CLICOLOR_FORCE`, `TERM=dumb`, pipes and Windows consoles. A
//! renderer that branched on `if colour {}` would be a second, invisible
//! decision-maker; see [`mode`] for the whole strategy.

pub mod mode;
pub mod render;
pub mod surfaces;
pub mod theme;
pub mod views;

pub use mode::{ColorPolicy, Mode, Presentation};
pub use render::{RenderOptions, render, render_bytes};

/// The envelope type this renderer consumes, re-exported so a caller does not
/// have to name `fs3_core` to hold one.
pub use fs3_core::envelope::{Envelope, Failure};
