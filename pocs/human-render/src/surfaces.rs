//! The four screens that matter, plus the fallback that keeps the fifth from
//! being blank.
//!
//! Each surface is a pure `&Envelope<Value> -> String`. They share the
//! vocabulary in [`crate::theme`] and nothing else — no state, no io, no
//! ordering between them.

pub mod doctor;
pub mod failure;
pub mod generic;
pub mod search;
pub mod status;
