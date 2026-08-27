//! Fakes over mocks, shipped as infrastructure (workshop 001 rule 5).
//!
//! Two things live here, and they are the same thing wearing different hats:
//!
//! - **Reusable fakes** ([`FakeEmbedder`], [`FakeSummarizer`]) rich enough to
//!   run the whole stack offline. `provider = "fake"` is a legal runtime config
//!   value, so this crate ships in the daemon binary — it is not dev-only.
//! - **Contract harnesses** ([`embedder_contract`], [`summarizer_contract`])
//!   written once and run over every implementation: the fake in CI, the real
//!   provider on demand. That is what keeps a fake honest.
//!
//! There is no mocking framework anywhere in fs3, and there never will be.
//!
//! [`arch`] is the odd one out: the mechanical enforcement of the crate graph
//! itself. It lives here because this is the crate that ships proof.
//!
//! [`discovery_filter`] is the same idea one layer down: a table of cases two
//! crates must answer identically, so that "these two filters agree" is a
//! build failure when it stops being true rather than a claim in a doc.

pub mod arch;
pub mod contract;
pub mod database;
pub mod discovery_filter;
pub mod fakes;

pub use contract::{embedder_contract, sample_element, summarizer_contract};
pub use database::{TEST_DATABASE_ENV, refusal, test_database_url};
pub use fakes::{FakeEmbedder, FakeSummarizer};
