//! The committed expectations, proven loadable, pinned, and RED.
//!
//! Plan 005 tk-c105. Four coders will stake their unit's done-bar on
//! [`Expectations`]; a helper that has only ever passed is a helper that might
//! assert nothing, so each assertion here is also run against output that is
//! wrong in one named way, and each failure is required.

use std::panic::{AssertUnwindSafe, catch_unwind};

use fs3_testkit::{Expectations, FixtureStore};

/// Run `body` with the panic hook silenced, returning its panic message.
fn violation(body: impl FnOnce()) -> String {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(body));
    std::panic::set_hook(previous);

    let payload = outcome.expect_err("this case must fail the expectation, and it passed");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "<non-string panic>".to_owned())
}

#[test]
fn every_store_has_expectations_that_load() {
    for store in FixtureStore::ALL {
        let expectations = Expectations::load(store);
        assert_eq!(expectations.store, store.dir_name());
        assert!(
            expectations.claims.contains(&"structural".to_owned()),
            "{}: every store must at least make the structural claim",
            store.dir_name()
        );
        assert!(
            !expectations.sessions.is_empty(),
            "{}: an expectation file with no sessions asserts nothing",
            store.dir_name()
        );
        assert!(
            !expectations.grade_of_proof.is_empty(),
            "{}: a file must say what it proves and what it does not",
            store.dir_name()
        );
    }
}

#[test]
fn the_committed_fixtures_are_exactly_what_the_expectations_describe() {
    for store in FixtureStore::ALL {
        Expectations::load(store).verify_fixtures_unchanged();
    }
}

#[test]
fn the_oracle_pinned_in_every_file_is_the_one_sha_pinned_in_the_plan() {
    // One oracle, one sha: a file generated from a drifted oracle would make
    // every claim in it unattributable.
    let pinned = "62b89a89f7035bd8ba077375ffe3e91ae5aa76e650e9cf54b092832302ad82c1";
    for store in FixtureStore::ALL {
        let expectations = Expectations::load(store);
        assert_eq!(
            expectations.oracle.sha256,
            pinned,
            "{}: expectations were generated from an oracle that is not the pinned reconvo.py",
            store.dir_name()
        );
    }
}

#[test]
fn claude_makes_no_oracle_claim_and_says_so() {
    // The pinned oracle has no claude-native reader. The honest shape is an
    // absent claim with the reason recorded, not a claim nobody can cash.
    let expectations = Expectations::load(FixtureStore::Claude);
    assert!(
        !expectations.claims.contains(&"subset".to_owned()),
        "claude cannot make a subset claim: the oracle cannot read this store"
    );
    assert!(
        expectations.oracle.entrypoint.is_none(),
        "claude expectations must name no oracle entrypoint"
    );
    assert!(
        expectations
            .sessions
            .iter()
            .all(|session| session.oracle_turns == 0),
        "claude sessions must claim zero oracle turns"
    );
}

#[test]
fn the_claude_fixtures_carry_the_block_merge_the_reader_owes() {
    // Recipe gotcha 1: claude writes one line per content block, so the gap
    // between assistant RECORDS and distinct message ids is exactly the
    // merging u1a must perform. A fixture where the two are equal would let a
    // reader that never merges pass.
    let expectations = Expectations::load(FixtureStore::Claude);
    let merging = expectations.sessions.iter().any(|session| {
        let records = session.extras["assistant_records"].as_u64();
        let ids = session.extras["distinct_assistant_message_ids"].as_u64();
        matches!((records, ids), (Some(r), Some(i)) if r > i)
    });
    assert!(
        merging,
        "no claude fixture has more assistant records than distinct message ids, so nothing \
         here forces the per-block merge"
    );
}

#[test]
fn a_reader_that_invents_an_ordinal_is_caught() {
    let expectations = Expectations::load(FixtureStore::Omp);
    let key = expectations.sessions[0].key.clone();
    let mut observed: Vec<String> = expectations
        .ordinals(&key)
        .iter()
        .take(3)
        .map(|id| (*id).to_owned())
        .collect();
    observed.push("not-a-record-this-store-ever-wrote".to_owned());

    let message = violation(|| expectations.assert_ordinals_are_a_subsequence(&key, &observed));
    assert!(
        message.contains("the store does not hold") || message.contains("never wrote"),
        "the failure must name what broke: {message}"
    );
}

#[test]
fn a_reader_that_reorders_records_is_caught() {
    let expectations = Expectations::load(FixtureStore::Omp);
    let key = expectations.sessions[0].key.clone();
    let ordinals = expectations.ordinals(&key);
    let observed = vec![ordinals[4].to_owned(), ordinals[1].to_owned()];

    let message = violation(|| expectations.assert_ordinals_are_a_subsequence(&key, &observed));
    assert!(
        message.contains("OUT OF STORE ORDER"),
        "a reordering must be named as one: {message}"
    );
}

#[test]
fn a_reader_that_emits_one_record_twice_is_caught() {
    let expectations = Expectations::load(FixtureStore::Pij);
    let key = expectations.sessions[0].key.clone();
    let ordinals = expectations.ordinals(&key);
    let observed = vec![ordinals[0].to_owned(), ordinals[0].to_owned()];

    let message = violation(|| expectations.assert_ordinals_are_a_subsequence(&key, &observed));
    assert!(
        message.contains("twice"),
        "a duplicate ordinal must be named as one: {message}"
    );
}

#[test]
fn a_reader_that_drops_an_oracle_turn_is_caught() {
    let expectations = Expectations::load(FixtureStore::Omp);
    let key = expectations.sessions[0].key.clone();
    // Everything the oracle read, minus its first prose turn.
    let mut prose: Vec<String> = Vec::new();
    let mut skipped = false;
    for turn in &expectations.turns[&key] {
        if !expectations.prose_kinds.contains(&turn.kind) {
            continue;
        }
        if !skipped {
            skipped = true;
            continue;
        }
        prose.push(format!("placeholder for turn {}", turn.n));
    }
    assert!(
        skipped,
        "the omp fixture must contain at least one prose turn"
    );

    let message = violation(|| expectations.assert_oracle_prose_appears(&key, &prose));
    assert!(
        message.contains("MISSING"),
        "a dropped oracle turn must be named as missing: {message}"
    );
}

#[test]
fn a_reader_that_reproduces_the_oracle_exactly_passes() {
    // The green case cannot be faked from the expectation file alone — the
    // text hashes are of the fixture's own bytes — so it is proven by reading
    // the fixture the way the oracle did and feeding those bodies back.
    let expectations = Expectations::load(FixtureStore::Omp);
    let key = expectations.sessions[0].key.clone();
    let path = fs3_testkit::fixtures_root().join(&expectations.sessions[0].files[0]);
    let raw = std::fs::read_to_string(&path).expect("the omp fixture is committed");

    let mut bodies = Vec::new();
    for line in raw.lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if record["type"] != "message" {
            continue;
        }
        let role = record["message"]["role"].as_str().unwrap_or_default();
        if role != "user" && role != "assistant" {
            continue;
        }
        for block in record["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if block["type"] == "text"
                && let Some(text) = block["text"].as_str()
                && !text.trim().is_empty()
            {
                bodies.push(text.to_owned());
            }
        }
    }

    expectations.assert_oracle_prose_appears(&key, &bodies);
}
