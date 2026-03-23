use barkus_core::ir::{GrammarIr, ProductionId, Symbol};
use serde::Serialize;

use crate::stats::CorpusStats;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ReachabilityReport {
    /// Productions never hit across the entire corpus.
    pub unreached: Vec<UnreachedProduction>,
    /// Productions hit in less than 1% of payloads.
    pub low_coverage: Vec<LowCoverageProduction>,
    /// Alternatives that were never or rarely chosen.
    pub starved_alternatives: Vec<StarvedAlternative>,
    /// Productions reachable through only a single alternative of a single parent.
    pub choke_points: Vec<ChokePoint>,
    /// Alternatives with significantly lower weight than siblings.
    pub weight_disadvantaged: Vec<WeightDisadvantaged>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnreachedProduction {
    pub name: String,
    pub production_id: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LowCoverageProduction {
    pub name: String,
    pub production_id: u32,
    pub coverage_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StarvedAlternative {
    pub production_name: String,
    pub production_id: u32,
    pub alt_index: usize,
    pub hit_count: u64,
    pub expected_uniform: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChokePoint {
    pub production_name: String,
    pub production_id: u32,
    /// The single parent that reaches this production.
    pub parent_name: String,
    pub parent_id: u32,
    /// Which alternative of the parent reaches this production.
    pub parent_alt_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeightDisadvantaged {
    pub production_name: String,
    pub production_id: u32,
    pub alt_index: usize,
    pub weight: f32,
    pub max_sibling_weight: f32,
}

pub fn analyze(grammar: &GrammarIr, stats: &CorpusStats) -> ReachabilityReport {
    let total = stats.total_payloads;

    // --- Empirical: unreached ---
    let unreached: Vec<UnreachedProduction> = stats
        .productions
        .iter()
        .filter(|p| p.payload_hit_count == 0)
        .map(|p| UnreachedProduction {
            name: p.name.clone(),
            production_id: p.production_id,
        })
        .collect();

    // --- Empirical: low coverage (< 1% of payloads) ---
    let low_coverage: Vec<LowCoverageProduction> = if total > 0 {
        stats
            .productions
            .iter()
            .filter(|p| {
                p.payload_hit_count > 0 && (p.payload_hit_count as f64 / total as f64) < 0.01
            })
            .map(|p| LowCoverageProduction {
                name: p.name.clone(),
                production_id: p.production_id,
                coverage_pct: p.payload_hit_count as f64 / total as f64 * 100.0,
            })
            .collect()
    } else {
        Vec::new()
    };

    // --- Empirical: starved alternatives ---
    let mut starved_alternatives = Vec::new();
    for ps in &stats.productions {
        if ps.has_semantic_hook || ps.alternatives.len() <= 1 {
            continue;
        }
        let total_hits: u64 = ps.alternatives.iter().map(|a| a.hit_count).sum();
        if total_hits == 0 {
            continue;
        }
        let n = ps.alternatives.len() as f64;
        let expected_uniform = total_hits as f64 / n;

        for alt in &ps.alternatives {
            let reason = if alt.hit_count == 0 {
                "never chosen".to_string()
            } else if (alt.hit_count as f64) < expected_uniform * 0.5 {
                format!(
                    "hit {} times, expected ~{:.0} (< 50% of uniform)",
                    alt.hit_count, expected_uniform
                )
            } else {
                continue;
            };

            starved_alternatives.push(StarvedAlternative {
                production_name: ps.name.clone(),
                production_id: ps.production_id,
                alt_index: alt.index,
                hit_count: alt.hit_count,
                expected_uniform,
                reason,
            });
        }
    }

    // --- Static: choke points ---
    // Build reverse map: for each production, which (parent, alt_index) pairs reference it?
    let n_prods = grammar.productions.len();
    let mut referencing_alts: Vec<Vec<(ProductionId, usize)>> = vec![Vec::new(); n_prods];

    for prod in &grammar.productions {
        for (alt_idx, alt) in prod.alternatives.iter().enumerate() {
            for sym_ref in &alt.symbols {
                if let Symbol::NonTerminal(target_id) = &grammar.symbols[sym_ref.symbol] {
                    referencing_alts[target_id.0 as usize].push((prod.id, alt_idx));
                }
            }
        }
    }

    // Deduplicate: a production can reference the same child multiple times in one alt
    for refs in &mut referencing_alts {
        refs.sort_unstable_by_key(|&(pid, ai)| (pid.0, ai));
        refs.dedup();
    }

    let choke_points: Vec<ChokePoint> = grammar
        .productions
        .iter()
        .filter(|prod| prod.id != grammar.start) // start is always reachable
        .filter_map(|prod| {
            let refs = &referencing_alts[prod.id.0 as usize];
            if refs.len() == 1 {
                let (parent_id, parent_alt_index) = refs[0];
                let parent = &grammar.productions[parent_id];
                Some(ChokePoint {
                    production_name: prod.name.clone(),
                    production_id: prod.id.0,
                    parent_name: parent.name.clone(),
                    parent_id: parent_id.0,
                    parent_alt_index,
                })
            } else {
                None
            }
        })
        .collect();

    // --- Static: weight-disadvantaged alternatives ---
    let mut weight_disadvantaged = Vec::new();
    for prod in &grammar.productions {
        if prod.alternatives.len() <= 1 {
            continue;
        }
        let max_weight = prod
            .alternatives
            .iter()
            .map(|a| a.weight)
            .fold(0.0f32, f32::max);
        if max_weight <= 0.0 {
            continue;
        }
        for (ai, alt) in prod.alternatives.iter().enumerate() {
            // Flag if weight is less than 25% of max sibling
            if alt.weight < max_weight * 0.25 {
                weight_disadvantaged.push(WeightDisadvantaged {
                    production_name: prod.name.clone(),
                    production_id: prod.id.0,
                    alt_index: ai,
                    weight: alt.weight,
                    max_sibling_weight: max_weight,
                });
            }
        }
    }

    ReachabilityReport {
        unreached,
        low_coverage,
        starved_alternatives,
        choke_points,
        weight_disadvantaged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{AlternativeStats, FailureBreakdown, Histogram, ProductionStats};

    fn empty_histogram() -> Histogram {
        Histogram {
            buckets: Vec::new(),
            min: u32::MAX,
            max: 0,
        }
    }

    #[test]
    fn test_unreached_detection() {
        // Minimal grammar with 2 productions
        let grammar = GrammarIr {
            productions: vec![
                barkus_core::ir::Production {
                    id: ProductionId(0),
                    name: "root".into(),
                    alternatives: vec![barkus_core::ir::Alternative {
                        symbols: vec![],
                        weight: 1.0,
                        semantic_tag: None,
                    }],
                    attrs: Default::default(),
                },
                barkus_core::ir::Production {
                    id: ProductionId(1),
                    name: "unused".into(),
                    alternatives: vec![barkus_core::ir::Alternative {
                        symbols: vec![],
                        weight: 1.0,
                        semantic_tag: None,
                    }],
                    attrs: Default::default(),
                },
            ],
            symbols: vec![],
            start: ProductionId(0),
            token_pools: vec![],
        };

        let stats = CorpusStats {
            total_payloads: 100,
            failures: 0,
            failure_breakdown: FailureBreakdown {
                max_depth: 0,
                max_total_nodes: 0,
            },
            productions: vec![
                ProductionStats {
                    name: "root".into(),
                    production_id: 0,
                    hit_count: 100,
                    payload_hit_count: 100,
                    alternatives: vec![AlternativeStats {
                        index: 0,
                        weight: 1.0,
                        hit_count: 100,
                    }],
                    has_semantic_hook: false,
                },
                ProductionStats {
                    name: "unused".into(),
                    production_id: 1,
                    hit_count: 0,
                    payload_hit_count: 0,
                    alternatives: vec![AlternativeStats {
                        index: 0,
                        weight: 1.0,
                        hit_count: 0,
                    }],
                    has_semantic_hook: false,
                },
            ],
            depth_histogram: empty_histogram(),
            node_count_histogram: empty_histogram(),
        };

        let report = analyze(&grammar, &stats);
        assert_eq!(report.unreached.len(), 1);
        assert_eq!(report.unreached[0].name, "unused");
    }
}
