use barkus_core::profile::Profile;
use serde::Serialize;

use crate::stats::CorpusStats;

/// Failure rate must exceed this threshold before any recommendations are made.
const FAILURE_RATE_THRESHOLD: f64 = 0.10;
/// A single failure cause must exceed this fraction of total payloads to trigger a suggestion.
const CAUSE_FRACTION_THRESHOLD: f64 = 0.10;

const DEPTH_FLAG: &str = "--max-depth";
const NODES_FLAG: &str = "--max-nodes";
const DEPTH_MULTIPLIER: u32 = 2;
const NODES_MULTIPLIER: u32 = 5;

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub flag: String,
    pub reason: String,
    pub estimated_impact: String,
}

/// Analyze corpus stats and suggest profile flag changes to reduce failure rate below 10%.
pub fn analyze(stats: &CorpusStats, profile: &Profile) -> Vec<Recommendation> {
    if stats.total_payloads == 0 {
        return Vec::new();
    }

    let fail_rate = stats.failures as f64 / stats.total_payloads as f64;
    if fail_rate < FAILURE_RATE_THRESHOLD {
        return Vec::new();
    }

    let bd = &stats.failure_breakdown;
    let total = stats.total_payloads as f64;

    let depth_rec = maybe_recommend(
        bd.max_depth,
        total,
        stats.failures,
        DEPTH_FLAG,
        "max-depth exceeded",
        profile.max_depth,
        DEPTH_MULTIPLIER,
    );
    let nodes_rec = maybe_recommend(
        bd.max_total_nodes,
        total,
        stats.failures,
        NODES_FLAG,
        "max-total-nodes exceeded",
        profile.max_total_nodes,
        NODES_MULTIPLIER,
    );

    // Order by which cause contributes more failures.
    let mut recs = Vec::new();
    if bd.max_depth >= bd.max_total_nodes {
        recs.extend(depth_rec);
        recs.extend(nodes_rec);
    } else {
        recs.extend(nodes_rec);
        recs.extend(depth_rec);
    }
    recs
}

fn maybe_recommend(
    cause_count: u64,
    total: f64,
    total_failures: u64,
    flag: &str,
    cause_label: &str,
    current_val: u32,
    multiplier: u32,
) -> Option<Recommendation> {
    let frac = cause_count as f64 / total;
    if frac <= CAUSE_FRACTION_THRESHOLD {
        return None;
    }
    let new_val = current_val * multiplier;
    let pct_of_failures = (cause_count as f64 / total_failures as f64 * 100.0).min(100.0);
    Some(Recommendation {
        flag: format!("{flag} {new_val}"),
        reason: format!(
            "{pct_of_failures:.0}% of failures are {cause_label} (current: {current_val})",
        ),
        estimated_impact: format!(
            "likely eliminates ~{} of {} failures",
            fmt_approx(cause_count),
            fmt_approx(total_failures),
        ),
    })
}

fn fmt_approx(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{CorpusStats, FailureBreakdown, Histogram};

    fn make_stats(total: u64, failures: u64, max_depth: u64, max_nodes: u64) -> CorpusStats {
        CorpusStats {
            total_payloads: total,
            failures,
            failure_breakdown: FailureBreakdown {
                max_depth,
                max_total_nodes: max_nodes,
            },
            productions: vec![],
            depth_histogram: Histogram {
                buckets: vec![],
                min: u32::MAX,
                max: 0,
            },
            node_count_histogram: Histogram {
                buckets: vec![],
                min: u32::MAX,
                max: 0,
            },
        }
    }

    #[test]
    fn no_recommendations_when_failure_rate_low() {
        let stats = make_stats(1000, 50, 30, 20);
        let profile = Profile::default();
        let recs = analyze(&stats, &profile);
        assert!(recs.is_empty());
    }

    #[test]
    fn recommends_depth_increase() {
        let stats = make_stats(1000, 600, 500, 100);
        let profile = Profile::default();
        let recs = analyze(&stats, &profile);
        assert!(!recs.is_empty());
        assert!(recs[0].flag.contains("--max-depth 60"));
    }

    #[test]
    fn recommends_nodes_increase() {
        let stats = make_stats(1000, 600, 100, 500);
        let profile = Profile::default();
        let recs = analyze(&stats, &profile);
        assert!(!recs.is_empty());
        assert!(recs[0].flag.contains("--max-nodes 100000"));
    }

    #[test]
    fn both_ordered_by_severity() {
        let stats = make_stats(1000, 600, 200, 400);
        let profile = Profile::default();
        let recs = analyze(&stats, &profile);
        assert_eq!(recs.len(), 2);
        // nodes cause more failures, so nodes first
        assert!(recs[0].flag.contains("--max-nodes"));
        assert!(recs[1].flag.contains("--max-depth"));
    }

    #[test]
    fn no_recommendations_when_zero_payloads() {
        let stats = make_stats(0, 0, 0, 0);
        let profile = Profile::default();
        let recs = analyze(&stats, &profile);
        assert!(recs.is_empty());
    }
}
