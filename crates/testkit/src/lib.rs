//! Fakes over mocks, shipped as infrastructure (workshop 001 rule 5).
//!
//! What lives here is test infrastructure that is load-bearing enough to be
//! shipped, versioned and proven rather than copy-pasted:
//!
//! - **Reusable fakes** ([`FakeEmbedder`], [`FakeSummarizer`]) rich enough to
//!   run the whole stack offline. `provider = "fake"` is a legal runtime config
//!   value, so this crate ships in the daemon binary — it is not dev-only.
//! - **Contract harnesses** ([`embedder_contract`], [`summarizer_contract`])
//!   written once and run over every implementation: the fake in CI, the real
//!   provider on demand. That is what keeps a fake honest.
//! - **The blast-radius seals** ([`database`], [`spawn`]) — which store a test
//!   may open a pool against, and what a test may hand a `flowspace3`
//!   SUBPROCESS. Both exist because the wrong answer was unrecoverable in
//!   production, and both were written the day after it was.
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
pub mod conversation_source;
pub mod database;
pub mod discovery_filter;
pub mod expectations;
pub mod fake_source;
pub mod fakes;
pub mod spawn;

pub use contract::{embedder_contract, sample_element, summarizer_contract};
pub use conversation_source::{SourceFixture, conversation_source_contract};
pub use database::{TEST_DATABASE_ENV, refusal, test_database_url};
pub use expectations::{Expectations, FixtureStore, fixtures_root};
pub use fake_source::{
    FAKE_CHILDREN_DIR, FAKE_MAIN_FILE, FakeConversationSource, FakeDefect, FakeSourceFixture,
};
pub use fakes::{FakeEmbedder, FakeSummarizer};
pub use spawn::{TestDatabase, flowspace3_binary, sealed};
