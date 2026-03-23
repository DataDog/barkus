use barkus_core::ast::Ast;
use barkus_core::error::{BudgetKind, GenerateError};
use barkus_core::ir::{GrammarIr, NodeId};
use barkus_core::tape::map::TapeMap;
use barkus_core::tape::DecisionTape;
use serde::Serialize;

/// Histogram with fixed-size buckets.
#[derive(Debug, Clone, Serialize)]
pub struct Histogram {
    /// Each entry: (bucket_lower_bound, count).
    pub buckets: Vec<(u32, u64)>,
    pub min: u32,
    pub max: u32,
}

impl Histogram {
    fn new() -> Self {
        Self {
            buckets: Vec::new(),
            min: u32::MAX,
            max: 0,
        }
    }

    /// Build histogram buckets from recorded raw values.
    /// Called after all values have been collected via `record_into_raw`.
    fn build_from_raw(values: &[u32], n_buckets: usize) -> Self {
        if values.is_empty() {
            return Self::new();
        }
        let min = *values.iter().min().unwrap();
        let max = *values.iter().max().unwrap();
        if min == max {
            return Self {
                buckets: vec![(min, values.len() as u64)],
                min,
                max,
            };
        }
        let range = max - min + 1;
        let bucket_size = (range as usize).div_ceil(n_buckets).max(1);
        let actual_buckets = (range as usize).div_ceil(bucket_size);
        let mut counts = vec![0u64; actual_buckets];
        for &v in values {
            let idx = ((v - min) as usize) / bucket_size;
            counts[idx] += 1;
        }
        let buckets = counts
            .into_iter()
            .enumerate()
            .map(|(i, c)| (min + (i * bucket_size) as u32, c))
            .collect();
        Self { buckets, min, max }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AlternativeStats {
    pub index: usize,
    pub weight: f32,
    pub hit_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionStats {
    pub name: String,
    pub production_id: u32,
    pub hit_count: u64,
    pub payload_hit_count: u64,
    pub alternatives: Vec<AlternativeStats>,
    pub has_semantic_hook: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureBreakdown {
    pub max_depth: u64,
    pub max_total_nodes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorpusStats {
    pub total_payloads: u64,
    pub failures: u64,
    pub failure_breakdown: FailureBreakdown,
    pub productions: Vec<ProductionStats>,
    pub depth_histogram: Histogram,
    pub node_count_histogram: Histogram,
}

/// Streaming collector — does not retain ASTs.
pub struct StatsCollector {
    total_payloads: u64,
    failures: u64,
    failures_max_depth: u64,
    failures_max_nodes: u64,
    /// Per-production hit count.
    prod_hits: Vec<u64>,
    /// Per-production payload presence count.
    prod_payload_hits: Vec<u64>,
    /// Per-production, per-alternative hit count.
    alt_hits: Vec<Vec<u64>>,
    /// Per-payload presence flag per production.
    presence: Vec<bool>,
    /// Indices of productions touched this payload (for fast reset).
    touched: Vec<usize>,
    /// Raw depth values (compacted into histogram at finalize).
    depth_values: Vec<u32>,
    /// Raw node-count values.
    node_count_values: Vec<u32>,
    /// Which productions have semantic hooks (skip alt attribution).
    has_semantic_hook: Vec<bool>,
    /// Number of alternatives per production (for tape decoding).
    n_alts: Vec<usize>,
}

impl StatsCollector {
    pub fn new_with_capacity(grammar: &GrammarIr, expected_payloads: u64) -> Self {
        let n = grammar.productions.len();
        let alt_hits: Vec<Vec<u64>> = grammar
            .productions
            .iter()
            .map(|p| vec![0u64; p.alternatives.len()])
            .collect();
        let has_semantic_hook: Vec<bool> = grammar
            .productions
            .iter()
            .map(|p| p.attrs.semantic_hook.is_some())
            .collect();
        let n_alts: Vec<usize> = grammar
            .productions
            .iter()
            .map(|p| p.alternatives.len())
            .collect();
        Self {
            total_payloads: 0,
            failures: 0,
            failures_max_depth: 0,
            failures_max_nodes: 0,
            prod_hits: vec![0u64; n],
            prod_payload_hits: vec![0u64; n],
            alt_hits,
            presence: vec![false; n],
            touched: Vec::with_capacity(n),
            depth_values: Vec::with_capacity(expected_payloads as usize),
            node_count_values: Vec::with_capacity(expected_payloads as usize),
            has_semantic_hook,
            n_alts,
        }
    }

    pub fn record_failure(&mut self, error: &GenerateError) {
        self.total_payloads += 1;
        self.failures += 1;
        match error {
            GenerateError::BudgetExhausted {
                kind: BudgetKind::MaxDepth,
            } => {
                self.failures_max_depth += 1;
            }
            GenerateError::BudgetExhausted {
                kind: BudgetKind::MaxTotalNodes,
            } => {
                self.failures_max_nodes += 1;
            }
        }
    }

    pub fn record_payload(&mut self, ast: &Ast, tape: &DecisionTape, tape_map: &TapeMap) {
        self.total_payloads += 1;

        // Walk tape map entries to attribute production and alternative hits.
        for entry in &tape_map.entries {
            let pid = entry.production_id.0 as usize;
            self.prod_hits[pid] += 1;
            if !self.presence[pid] {
                self.presence[pid] = true;
                self.touched.push(pid);
            }

            // Recover alternative index from tape byte.
            if !self.has_semantic_hook[pid] {
                let n = self.n_alts[pid];
                if n > 1 {
                    let byte = tape.bytes[entry.tape_offset] as usize;
                    let alt_idx = byte % n;
                    self.alt_hits[pid][alt_idx] += 1;
                } else if n == 1 {
                    self.alt_hits[pid][0] += 1;
                }
            }
        }

        // Increment payload presence counts and reset only touched entries.
        for &pid in &self.touched {
            self.prod_payload_hits[pid] += 1;
            self.presence[pid] = false;
        }
        self.touched.clear();

        // Compute max depth via iterative DFS.
        let max_depth = compute_max_depth(ast);
        self.depth_values.push(max_depth);

        // Record node count.
        self.node_count_values.push(ast.nodes.len() as u32);
    }

    pub fn finalize(self, grammar: &GrammarIr) -> CorpusStats {
        let productions: Vec<ProductionStats> = grammar
            .productions
            .iter()
            .enumerate()
            .map(|(i, prod)| {
                let alternatives = prod
                    .alternatives
                    .iter()
                    .enumerate()
                    .map(|(ai, alt)| AlternativeStats {
                        index: ai,
                        weight: alt.weight,
                        hit_count: self.alt_hits[i][ai],
                    })
                    .collect();
                ProductionStats {
                    name: prod.name.clone(),
                    production_id: prod.id.0,
                    hit_count: self.prod_hits[i],
                    payload_hit_count: self.prod_payload_hits[i],
                    alternatives,
                    has_semantic_hook: self.has_semantic_hook[i],
                }
            })
            .collect();

        let depth_histogram = Histogram::build_from_raw(&self.depth_values, 20);
        let node_count_histogram = Histogram::build_from_raw(&self.node_count_values, 20);

        CorpusStats {
            total_payloads: self.total_payloads,
            failures: self.failures,
            failure_breakdown: FailureBreakdown {
                max_depth: self.failures_max_depth,
                max_total_nodes: self.failures_max_nodes,
            },
            productions,
            depth_histogram,
            node_count_histogram,
        }
    }
}

/// Compute max depth of the AST via iterative DFS.
fn compute_max_depth(ast: &Ast) -> u32 {
    if ast.nodes.is_empty() {
        return 0;
    }
    // Stack: (node_id, depth)
    let mut stack: Vec<(NodeId, u32)> = Vec::with_capacity(64);
    stack.push((ast.root, 1));
    let mut max_depth: u32 = 0;

    while let Some((node_id, depth)) = stack.pop() {
        let node = &ast.nodes[node_id];
        if node.children.is_empty() {
            max_depth = max_depth.max(depth);
        } else {
            for &child in &node.children {
                stack.push((child, depth + 1));
            }
        }
    }
    max_depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use barkus_core::ast::AstNodeKind;
    use barkus_core::ir::ProductionId;

    #[test]
    fn test_histogram_build() {
        let values = vec![1, 2, 3, 4, 5, 10, 20];
        let h = Histogram::build_from_raw(&values, 5);
        assert_eq!(h.min, 1);
        assert_eq!(h.max, 20);
        assert!(!h.buckets.is_empty());
        let total: u64 = h.buckets.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 7);
    }

    #[test]
    fn test_histogram_empty() {
        let h = Histogram::build_from_raw(&[], 10);
        assert!(h.buckets.is_empty());
    }

    #[test]
    fn test_histogram_single_value() {
        let values = vec![42, 42, 42];
        let h = Histogram::build_from_raw(&values, 10);
        assert_eq!(h.buckets.len(), 1);
        assert_eq!(h.buckets[0], (42, 3));
    }

    #[test]
    fn test_compute_max_depth() {
        // Build a simple tree: root -> child -> leaf
        let mut ast = Ast {
            nodes: Vec::new(),
            root: NodeId(0),
        };
        let root = ast.new_node(AstNodeKind::Production(ProductionId(0)));
        let child = ast.new_node(AstNodeKind::Production(ProductionId(1)));
        let leaf = ast.new_node(AstNodeKind::Terminal(vec![b'x']));
        ast.add_child(root, child);
        ast.add_child(child, leaf);
        assert_eq!(compute_max_depth(&ast), 3);
    }
}
