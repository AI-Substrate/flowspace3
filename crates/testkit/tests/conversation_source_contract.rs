//! The contract suite, proven both green and RED.
//!
//! A suite that has only ever passed is a suite that might assert nothing.
//! Four coders stake a unit's definition of done on
//! [`conversation_source_contract`], so before any of them started it was run
//! against four readers that are each wrong in one named way, and each failure
//! was required (plan 005, tk-c104).

use std::panic::{AssertUnwindSafe, catch_unwind};

use fs3_testkit::{FakeDefect, FakeSourceFixture, SourceFixture, conversation_source_contract};

/// Run the suite and return the panic message it produced, if any.
fn violation(defect: FakeDefect) -> Option<String> {
    let mut fixture = FakeSourceFixture::new(4, 1, defect);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        conversation_source_contract(&mut fixture as &mut dyn SourceFixture);
    }));
    std::panic::set_hook(previous);

    outcome.err().map(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "<non-string panic>".to_owned())
    })
}

#[test]
fn a_correct_reader_passes_every_case() {
    let mut fixture = FakeSourceFixture::new(4, 1, FakeDefect::None);
    conversation_source_contract(&mut fixture as &mut dyn SourceFixture);
}

#[test]
fn a_reader_that_ignores_its_cursor_is_caught() {
    let message = violation(FakeDefect::IgnoresCursor)
        .expect("a reader that re-reads the whole file on every poll must fail the contract");
    assert!(
        message.contains("polling an unchanged conversation"),
        "the failure must name the claim that broke, not just panic: {message}"
    );
}

#[test]
fn a_reader_that_returns_half_a_record_is_caught() {
    let message = violation(FakeDefect::EmitsTornRecords)
        .expect("a reader that returns a partially written line must fail the contract");
    assert!(
        message.contains("half a record is not a record"),
        "the torn-record case must be the one that fires: {message}"
    );
}

#[test]
fn a_reader_that_accepts_another_stores_cursor_is_caught() {
    let message = violation(FakeDefect::AcceptsForeignCursor)
        .expect("silently reading a foreign cursor as zero must fail the contract");
    assert!(
        message.contains("must be REFUSED"),
        "the foreign-cursor case must be the one that fires: {message}"
    );
}

#[test]
fn a_reader_that_duplicates_ordinals_is_caught() {
    let message = violation(FakeDefect::DuplicateOrdinals)
        .expect("emitting one record twice under one ordinal must fail the contract");
    assert!(
        message.contains("ordinals must be unique") || message.contains("whole conversation"),
        "a duplicate ordinal must be caught by the count or the uniqueness claim: {message}"
    );
}
