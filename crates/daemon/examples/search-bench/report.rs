//! Per-stage percentiles by arm and phase, as a table and as JSON.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::run::Sample;

#[derive(Debug, Serialize)]
pub struct StageSummary {
    pub stage: &'static str,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
}

#[derive(Debug, Serialize)]
pub struct GroupSummary {
    pub arm: String,
    pub phase: String,
    pub searches: usize,
    pub errors: usize,
    pub mean_passes: f64,
    pub mean_hits: f64,
    pub stages: Vec<StageSummary>,
}

type Read = fn(&Sample) -> Option<u64>;

const STAGES: [(&str, Read); 10] = [
    ("total", |s| Some(s.wall_ms)),
    ("embed", |s| s.timing.as_ref().map(|t| t.embed_ms)),
    ("store", |s| s.timing.as_ref().map(|t| t.store_ms)),
    ("semantic", |s| s.timing.as_ref().map(|t| t.semantic_ms)),
    ("lexical", |s| s.timing.as_ref().map(|t| t.lexical_ms)),
    ("wide", |s| {
        s.timing
            .as_ref()
            .and_then(|t| t.wide_semantic_ms.max(t.wide_lexical_ms))
    }),
    ("rank", |s| s.timing.as_ref().map(|t| t.rank_ms)),
    ("agents", |s| s.timing.as_ref().map(|t| t.agents_ms)),
    ("read_blocks", |s| {
        s.blocks_read.map(|r| u64::try_from(r).unwrap_or(0))
    }),
    ("hit_blocks", |s| {
        s.blocks_hit.map(|h| u64::try_from(h).unwrap_or(0))
    }),
];

fn percentile(sorted: &[u64], pct: usize) -> u64 {
    let rank = (sorted.len() * pct).div_ceil(100).max(1);
    sorted[rank - 1]
}

#[must_use]
pub fn summarize(samples: &[Sample]) -> Vec<GroupSummary> {
    let mut groups: BTreeMap<(String, String), Vec<&Sample>> = BTreeMap::new();
    for sample in samples {
        groups
            .entry((sample.arm.clone(), sample.phase.to_string()))
            .or_default()
            .push(sample);
    }
    groups
        .into_iter()
        .map(|((arm, phase), group)| {
            let ok: Vec<&Sample> = group
                .iter()
                .copied()
                .filter(|s| s.error.is_none())
                .collect();
            let stages = STAGES
                .iter()
                .filter_map(|(stage, read)| {
                    let mut values: Vec<u64> = ok.iter().filter_map(|s| read(s)).collect();
                    if values.is_empty() {
                        return None;
                    }
                    values.sort_unstable();
                    Some(StageSummary {
                        stage,
                        p50: percentile(&values, 50),
                        p95: percentile(&values, 95),
                        max: *values.last().expect("not empty"),
                    })
                })
                .collect();
            let n = ok.len().max(1) as f64;
            GroupSummary {
                arm,
                phase,
                searches: group.len(),
                errors: group.len() - ok.len(),
                mean_passes: ok.iter().map(|s| s.passes as f64).sum::<f64>() / n,
                mean_hits: ok.iter().map(|s| s.hits as f64).sum::<f64>() / n,
                stages,
            }
        })
        .collect()
}

/// The summary as a fixed-width table, one row per arm and phase.
#[must_use]
pub fn table(summaries: &[GroupSummary]) -> String {
    let mut out = String::new();
    let columns = [
        "total",
        "embed",
        "semantic",
        "lexical",
        "wide",
        "rank",
        "read_blocks",
    ];
    out.push_str(&format!(
        "{:<14} {:<5} {:>4} {:>6}",
        "arm", "phase", "n", "passes"
    ));
    for column in columns {
        out.push_str(&format!(" {:>17}", format!("{column} p50/p95")));
    }
    out.push('\n');
    for summary in summaries {
        out.push_str(&format!(
            "{:<14} {:<5} {:>4} {:>6.2}",
            summary.arm, summary.phase, summary.searches, summary.mean_passes
        ));
        for column in columns {
            let cell = summary
                .stages
                .iter()
                .find(|s| s.stage == column)
                .map_or("-".to_string(), |s| format!("{}/{}", s.p50, s.p95));
            out.push_str(&format!(" {cell:>17}"));
        }
        if summary.errors > 0 {
            out.push_str(&format!("  ({} errors)", summary.errors));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(arm: &str, phase: &'static str, wall_ms: u64) -> Sample {
        Sample {
            arm: arm.to_string(),
            round: 0,
            query: 0,
            kind: "question",
            phase,
            text: String::new(),
            scope: None,
            wall_ms,
            hits: 10,
            passes: 1,
            timing: None,
            blocks_hit: None,
            blocks_read: Some(5),
            error: None,
        }
    }

    #[test]
    fn groups_by_arm_and_phase_with_percentiles() {
        let samples: Vec<Sample> = (1..=20)
            .map(|ms| sample("base", "cold", ms * 10))
            .chain((1..=4).map(|ms| sample("base", "warm", ms)))
            .collect();
        let summaries = summarize(&samples);
        assert_eq!(summaries.len(), 2);
        let cold = &summaries[0];
        assert_eq!((cold.arm.as_str(), cold.phase.as_str()), ("base", "cold"));
        let total = &cold.stages[0];
        assert_eq!((total.p50, total.p95, total.max), (100, 190, 200));
        assert!(table(&summaries).contains("100/190"));
    }
}
