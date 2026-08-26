//! Merging embed jobs into wide provider calls.
//!
//! A `summarize` job is one call per element and cannot be batched. `embed` is
//! the opposite: the API takes many texts per request, and the difference
//! between one text per call and hundreds is most of the throughput. This is
//! the module that decides WHAT may ride together.
//!
//! Three rules, each of which is a correctness constraint rather than a tuning
//! knob:
//!
//! 1. **Group by `(identity, source_kind)`.** The identity selects the
//!    embedder, so two repos pointed at different models cannot share a call;
//!    the kind is part of the storage key.
//! 2. **Budget in TOKENS, not items.** Azure's real ceiling is ~300k tokens per
//!    request and says nothing about item count. A 200-item batch of one-line
//!    functions and a 200-item batch of long files are the same number and
//!    wildly different requests.
//! 3. **A job on its second attempt rides ALONE.** One poisonous item fails the
//!    whole merged call, and without solo retry the innocent jobs beside it
//!    inherit the failure and burn their attempts on somebody else's bad data.

use fs3_store::Job;

use crate::enrich::EmbedJob;

/// Tokens we allow in one provider request.
///
/// Azure's documented ceiling is 300k. This sits a third under it, because the
/// number we compare against is an ESTIMATE and being wrong here costs a whole
/// merged batch — every job in it fails together, and the retry is the same
/// expensive call again.
pub const TOKEN_BUDGET: usize = 200_000;

/// Bytes per token, for the estimate.
///
/// The usual rule of thumb is four, derived from prose. Code tokenizes worse —
/// punctuation, identifiers and indentation all fragment — so three is the
/// pessimistic direction, and pessimistic is the safe direction: overestimating
/// splits a batch that would have fitted, underestimating gets the request
/// rejected.
const BYTES_PER_TOKEN: usize = 3;

/// Attempts at which a job stops being allowed to travel with others.
///
/// The first attempt merges. A job that has already failed once is a suspect,
/// and a suspect must not be able to take a batch of innocents down with it.
pub const SOLO_FROM_ATTEMPT: i32 = 2;

/// A pessimistic token estimate for one text.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(BYTES_PER_TOKEN)
}

/// One provider call's worth of work: the items, and the jobs they came from.
#[derive(Debug)]
pub struct Batch {
    /// The repo whose embedder and model key apply.
    pub identity: String,
    /// `raw` or `smart`.
    pub source: String,
    /// `(source_hash, text)` pairs, merged in claim order.
    pub items: Vec<(String, String)>,
    /// The job rows this batch settles, all of them, together.
    pub job_ids: Vec<i64>,
}

/// Jobs accumulating under one `(identity, source)` before the budget cuts
/// them into calls.
struct Group {
    identity: String,
    source: String,
    items: Vec<(String, String)>,
    job_ids: Vec<i64>,
}

/// A claimed job whose payload could not be read.
#[derive(Debug)]
pub struct Unreadable {
    /// The row to fail.
    pub job_id: i64,
    /// Why.
    pub reason: String,
}

/// Split claimed jobs into provider calls.
///
/// Returns the batches to run and the jobs whose payloads were unreadable —
/// those are a defect, not a provider problem, and the caller fails them
/// without a retry rather than letting them poison a batch.
///
/// Batches come back in a stable order: solo jobs in claim order, then merged
/// groups. Determinism matters here because a test that asserts on call
/// contents should not be asserting on hash-map iteration order.
#[must_use]
pub fn plan(jobs: &[Job]) -> (Vec<Batch>, Vec<Unreadable>) {
    let mut batches = Vec::new();
    let mut unreadable = Vec::new();
    // Insertion-ordered grouping, so the plan is reproducible.
    let mut groups: Vec<Group> = Vec::new();

    for job in jobs {
        let parsed: EmbedJob = match serde_json::from_value(job.payload.clone()) {
            Ok(parsed) => parsed,
            Err(error) => {
                unreadable.push(Unreadable {
                    job_id: job.id,
                    reason: format!("embed payload is not readable: {error}"),
                });
                continue;
            }
        };

        // A suspect travels alone, even if it would have fitted.
        if job.attempts >= SOLO_FROM_ATTEMPT {
            batches.push(Batch {
                identity: parsed.identity,
                source: parsed.source,
                items: parsed.items,
                job_ids: vec![job.id],
            });
            continue;
        }

        match groups
            .iter_mut()
            .find(|group| group.identity == parsed.identity && group.source == parsed.source)
        {
            Some(group) => {
                group.items.extend(parsed.items);
                group.job_ids.push(job.id);
            }
            None => groups.push(Group {
                identity: parsed.identity,
                source: parsed.source,
                items: parsed.items,
                job_ids: vec![job.id],
            }),
        }
    }

    for group in groups {
        batches.extend(split_to_budget(
            &group.identity,
            &group.source,
            group.items,
            group.job_ids,
        ));
    }

    (batches, unreadable)
}

/// Cut one group into as few calls as the token budget allows.
///
/// A single item larger than the whole budget still goes out ALONE rather than
/// being dropped or split: we cannot divide one text, and refusing it would
/// leave that element permanently unvectorised — a silent hole in the index,
/// which is worse than one oversized request the provider may well accept.
fn split_to_budget(
    identity: &str,
    source: &str,
    items: Vec<(String, String)>,
    job_ids: Vec<i64>,
) -> Vec<Batch> {
    let mut out: Vec<Batch> = Vec::new();
    let mut current: Vec<(String, String)> = Vec::new();
    let mut spent = 0usize;

    for (hash, text) in items {
        let cost = estimate_tokens(&text);
        if !current.is_empty() && spent + cost > TOKEN_BUDGET {
            out.push(Batch {
                identity: identity.to_string(),
                source: source.to_string(),
                items: std::mem::take(&mut current),
                job_ids: job_ids.clone(),
            });
            spent = 0;
        }
        spent += cost;
        current.push((hash, text));
    }

    if !current.is_empty() {
        out.push(Batch {
            identity: identity.to_string(),
            source: source.to_string(),
            items: current,
            job_ids: job_ids.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn job(id: i64, attempts: i32, identity: &str, source: &str, items: usize) -> Job {
        let items: Vec<(String, String)> = (0..items)
            .map(|n| (format!("{id}-{n}"), format!("text {id}-{n}")))
            .collect();
        Job {
            id,
            kind: "embed".to_string(),
            dedupe_key: format!("embed:{id}"),
            payload: json!({ "identity": identity, "source": source, "items": items }),
            attempts,
        }
    }

    #[test]
    fn jobs_sharing_a_repo_and_a_kind_ride_together() {
        let (batches, bad) = plan(&[job(1, 1, "git:a", "raw", 2), job(2, 1, "git:a", "raw", 3)]);

        assert!(bad.is_empty());
        assert_eq!(batches.len(), 1, "one call, not two");
        assert_eq!(batches[0].items.len(), 5);
        assert_eq!(
            batches[0].job_ids,
            vec![1, 2],
            "and it settles both rows when it lands"
        );
    }

    /// The identity picks the EMBEDDER. Merging across identities would send a
    /// repo's text to a model it did not choose and store the vectors under
    /// the wrong key — a silent, permanent mis-index.
    #[test]
    fn different_repos_never_share_a_call() {
        let (batches, _) = plan(&[job(1, 1, "git:a", "raw", 1), job(2, 1, "git:b", "raw", 1)]);
        assert_eq!(batches.len(), 2);
    }

    /// `raw` and `smart` are different text with different meaning and a
    /// different storage key.
    #[test]
    fn raw_and_smart_never_share_a_call() {
        let (batches, _) = plan(&[job(1, 1, "git:a", "raw", 1), job(2, 1, "git:a", "smart", 1)]);
        assert_eq!(batches.len(), 2);
    }

    /// One bad item fails the whole merged call. A job that has already failed
    /// once must not be able to take innocents with it.
    #[test]
    fn a_job_on_its_second_attempt_rides_alone() {
        let (batches, _) = plan(&[
            job(1, 1, "git:a", "raw", 1),
            job(2, SOLO_FROM_ATTEMPT, "git:a", "raw", 1),
            job(3, 1, "git:a", "raw", 1),
        ]);

        let solo = batches
            .iter()
            .find(|batch| batch.job_ids == vec![2])
            .expect("the suspect is its own batch");
        assert_eq!(solo.items.len(), 1);
        assert!(
            batches.iter().any(|b| b.job_ids == vec![1, 3]),
            "and the innocents still merge with each other"
        );
    }

    #[test]
    fn a_batch_is_cut_when_the_token_budget_is_spent() {
        let big = "x".repeat(BYTES_PER_TOKEN * (TOKEN_BUDGET / 2 + 1));
        let payload = json!({
            "identity": "git:a",
            "source": "raw",
            "items": [["h1", big], ["h2", big], ["h3", big]],
        });
        let job = Job {
            id: 1,
            kind: "embed".to_string(),
            dedupe_key: "embed:1".to_string(),
            payload,
            attempts: 1,
        };

        let (batches, _) = plan(&[job]);
        assert!(
            batches.len() >= 2,
            "three half-budget texts cannot be one call"
        );
        for batch in &batches {
            let spent: usize = batch
                .items
                .iter()
                .map(|(_, text)| estimate_tokens(text))
                .sum();
            assert!(
                spent <= TOKEN_BUDGET || batch.items.len() == 1,
                "only a single oversized item may exceed the budget"
            );
        }
    }

    /// An item bigger than the entire budget still has to go out. Dropping it
    /// would leave that element permanently unvectorised — a hole in the index
    /// that no error ever names.
    #[test]
    fn one_oversized_item_still_gets_its_own_call() {
        let huge = "x".repeat(BYTES_PER_TOKEN * TOKEN_BUDGET * 2);
        let payload = json!({
            "identity": "git:a",
            "source": "raw",
            "items": [["h1", huge]],
        });
        let (batches, _) = plan(&[Job {
            id: 1,
            kind: "embed".to_string(),
            dedupe_key: "embed:1".to_string(),
            payload,
            attempts: 1,
        }]);

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].items.len(), 1);
    }

    /// An unreadable payload is a defect, not a provider problem. It must be
    /// reported for terminal failure rather than silently dropped — and it must
    /// not be able to poison a batch of readable jobs.
    #[test]
    fn an_unreadable_payload_is_reported_and_never_batched() {
        let mut broken = job(9, 1, "git:a", "raw", 1);
        broken.payload = json!({ "nonsense": true });

        let (batches, bad) = plan(&[job(1, 1, "git:a", "raw", 1), broken]);

        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].job_id, 9);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].job_ids, vec![1]);
    }
}
