//! How long recent searches took, stage by stage.
//!
//! Every search records its [`SearchTiming`] here, and `status` and `doctor`
//! read percentiles back. Before this, a search under five seconds left no
//! record at all, so "search is slow" could only be measured with a bench
//! (2026-10-14).

use std::collections::VecDeque;
use std::sync::Mutex;

use fs3_core::views::search::SearchTiming;
use fs3_core::views::status::{SearchLatency, StageLatency};

/// How many recent searches the percentiles are taken over.
const WINDOW: usize = 1024;

/// A search slower than this counts as slow, here and in the daemon log.
pub const SLOW_MS: u64 = 5_000;

/// Whole milliseconds in `duration`, saturating.
#[must_use]
pub fn duration_ms(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Whole milliseconds since `started`.
#[must_use]
pub fn elapsed_ms(started: std::time::Instant) -> u64 {
    duration_ms(started.elapsed())
}

/// The most recent [`WINDOW`] search timings.
#[derive(Debug, Default)]
pub struct SearchLatencies(Mutex<VecDeque<SearchTiming>>);

/// Reads one stage from a timing, or `None` when the search did not run it.
type ReadStage = fn(&SearchTiming) -> Option<u64>;

/// Each stage reported, and how to read it from one timing.
///
/// `wide` is the slower of the two unscoped legs, and only searches that ran
/// them count towards it.
const STAGES: [(&str, ReadStage); 11] = [
    ("total", |t| Some(t.total_ms)),
    ("guard", |t| Some(t.guard_ms)),
    ("scope", |t| Some(t.scope_ms)),
    ("embed", |t| Some(t.embed_ms)),
    ("store", |t| Some(t.store_ms)),
    ("semantic", |t| Some(t.semantic_ms)),
    ("lexical", |t| Some(t.lexical_ms)),
    ("wide", |t| t.wide_semantic_ms.max(t.wide_lexical_ms)),
    ("rank", |t| Some(t.rank_ms)),
    ("agents", |t| Some(t.agents_ms)),
    ("diagnose", |t| Some(t.diagnose_ms)),
];

impl SearchLatencies {
    /// Remember one search, forgetting the oldest past [`WINDOW`].
    pub fn record(&self, timing: SearchTiming) {
        let mut recent = self.0.lock().expect("search latencies poisoned");
        if recent.len() == WINDOW {
            recent.pop_front();
        }
        recent.push_back(timing);
    }

    /// Percentiles per stage, or `None` before the first search.
    #[must_use]
    pub fn report(&self) -> Option<SearchLatency> {
        let recent = self.0.lock().expect("search latencies poisoned");
        if recent.is_empty() {
            return None;
        }
        let stages: Vec<StageLatency> = STAGES
            .iter()
            .filter_map(|(stage, read)| {
                let mut values: Vec<u64> = recent.iter().filter_map(read).collect();
                if values.is_empty() {
                    return None;
                }
                values.sort_unstable();
                Some(StageLatency {
                    stage: (*stage).to_string(),
                    p50_ms: percentile(&values, 50),
                    p95_ms: percentile(&values, 95),
                    max_ms: *values.last().expect("not empty"),
                })
            })
            .collect();
        let slowest_stage = stages
            .iter()
            .filter(|row| row.stage != "total" && row.stage != "store")
            .max_by_key(|row| row.p95_ms)
            .map(|row| row.stage.clone())
            .unwrap_or_default();
        Some(SearchLatency {
            searches: recent.len(),
            slow: recent
                .iter()
                .filter(|timing| timing.total_ms > SLOW_MS)
                .count(),
            slow_ms: SLOW_MS,
            slowest_stage,
            stages,
        })
    }
}

/// The nearest-rank percentile of sorted, non-empty `values`.
fn percentile(values: &[u64], pct: usize) -> u64 {
    let rank = (values.len() * pct).div_ceil(100).max(1);
    values[rank - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(total_ms: u64, semantic_ms: u64, wide: Option<u64>) -> SearchTiming {
        SearchTiming {
            total_ms,
            semantic_ms,
            store_ms: semantic_ms,
            wide_semantic_ms: wide,
            ..SearchTiming::default()
        }
    }

    #[test]
    fn nothing_to_report_before_the_first_search() {
        assert_eq!(SearchLatencies::default().report(), None);
    }

    #[test]
    fn percentiles_are_nearest_rank() {
        let values: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&values, 50), 50);
        assert_eq!(percentile(&values, 95), 95);
        assert_eq!(percentile(&[7], 95), 7);
        assert_eq!(percentile(&[1, 2], 50), 1);
    }

    #[test]
    fn the_report_names_the_slowest_stage_and_counts_slow_searches() {
        let latencies = SearchLatencies::default();
        for ms in 1..=19 {
            latencies.record(timing(ms * 100, ms * 90, None));
        }
        latencies.record(timing(9_000, 8_500, Some(400)));
        let report = latencies.report().expect("searches were recorded");
        assert_eq!(report.searches, 20);
        assert_eq!(report.slow, 1);
        assert_eq!(report.slowest_stage, "semantic");
        let total = &report.stages[0];
        assert_eq!(total.stage, "total");
        assert_eq!(total.max_ms, 9_000);
        let wide = report
            .stages
            .iter()
            .find(|row| row.stage == "wide")
            .expect("one search ran the wide legs");
        assert_eq!((wide.p50_ms, wide.max_ms), (400, 400));
    }

    #[test]
    fn the_window_keeps_only_the_most_recent_searches() {
        let latencies = SearchLatencies::default();
        for _ in 0..WINDOW {
            latencies.record(timing(10_000, 0, None));
        }
        for _ in 0..WINDOW {
            latencies.record(timing(10, 0, None));
        }
        let report = latencies.report().expect("searches were recorded");
        assert_eq!(report.searches, WINDOW);
        assert_eq!(report.slow, 0);
    }
}
