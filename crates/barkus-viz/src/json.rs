use crate::reachability::ReachabilityReport;
use crate::recommend::Recommendation;
use crate::stats::CorpusStats;
use serde::Serialize;

#[derive(Serialize)]
struct JsonReport<'a> {
    grammar_path: &'a str,
    stats: &'a CorpusStats,
    reachability: &'a ReachabilityReport,
    recommendations: &'a [Recommendation],
}

pub fn render(
    stats: &CorpusStats,
    reachability: &ReachabilityReport,
    recommendations: &[Recommendation],
    grammar_path: &str,
) -> String {
    let report = make_report(stats, reachability, recommendations, grammar_path);
    serde_json::to_string_pretty(&report).expect("failed to serialize JSON report")
}

pub(crate) fn render_compact(
    stats: &CorpusStats,
    reachability: &ReachabilityReport,
    recommendations: &[Recommendation],
    grammar_path: &str,
) -> String {
    let report = make_report(stats, reachability, recommendations, grammar_path);
    serde_json::to_string(&report).expect("failed to serialize JSON report")
}

fn make_report<'a>(
    stats: &'a CorpusStats,
    reachability: &'a ReachabilityReport,
    recommendations: &'a [Recommendation],
    grammar_path: &'a str,
) -> JsonReport<'a> {
    JsonReport {
        grammar_path,
        stats,
        reachability,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reachability::ReachabilityReport;
    use crate::stats::{
        AlternativeStats, CorpusStats, FailureBreakdown, Histogram, ProductionStats,
    };

    #[test]
    fn test_render_valid_json() {
        let stats = CorpusStats {
            total_payloads: 100,
            failures: 2,
            failure_breakdown: FailureBreakdown {
                max_depth: 1,
                max_total_nodes: 1,
            },
            productions: vec![ProductionStats {
                name: "root".into(),
                production_id: 0,
                hit_count: 98,
                payload_hit_count: 98,
                alternatives: vec![AlternativeStats {
                    index: 0,
                    weight: 1.0,
                    hit_count: 98,
                }],
                has_semantic_hook: false,
            }],
            depth_histogram: Histogram {
                buckets: vec![(1, 98)],
                min: 1,
                max: 1,
            },
            node_count_histogram: Histogram {
                buckets: vec![(1, 98)],
                min: 1,
                max: 1,
            },
        };
        let reach = ReachabilityReport::default();

        let output = render(&stats, &reach, &[], "test.ebnf");
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("invalid JSON");
        assert_eq!(parsed["grammar_path"], "test.ebnf");
        assert_eq!(parsed["stats"]["total_payloads"], 100);
        assert_eq!(parsed["stats"]["failures"], 2);
        assert!(parsed["reachability"]["unreached"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
