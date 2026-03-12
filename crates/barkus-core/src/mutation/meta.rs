use crate::ast::{Ast, AstNodeKind};
use crate::ir::grammar::GrammarIr;
use crate::ir::ids::NodeId;
use crate::tape::map::TapeMap;

/// Per-input analysis used by the mutation engine.
///
/// While [`TapeMap`] is a generation-time artifact (recording which tape byte ranges correspond
/// to which productions), `MutationMeta` is a mutation-time structure that combines the
/// `TapeMap` with AST-derived data — production indexes, subtree sizes, and depths — needed
/// by structure-aware operators to make informed mutation decisions.
pub struct MutationMeta {
    pub tape_map: TapeMap,
    /// For each ProductionId, the list of AST NodeIds that are Production nodes with that id.
    pub nodes_by_production: Vec<Vec<NodeId>>,
    /// Subtree size (number of nodes) for each NodeId.
    pub subtree_sizes: Vec<u32>,
    /// Depth of each NodeId (root = 0).
    pub depths: Vec<u32>,
}

impl MutationMeta {
    pub fn compute(ast: &Ast, tape_map: TapeMap, grammar: &GrammarIr) -> Self {
        let n_nodes = ast.nodes.len();
        let n_prods = grammar.productions.len();

        let mut subtree_sizes = vec![0u32; n_nodes];
        let mut depths = vec![0u32; n_nodes];
        let mut nodes_by_production = vec![Vec::new(); n_prods];

        Self::dfs(
            ast,
            ast.root,
            0,
            &mut subtree_sizes,
            &mut depths,
            &mut nodes_by_production,
        );

        Self {
            tape_map,
            nodes_by_production,
            subtree_sizes,
            depths,
        }
    }

    fn dfs(
        ast: &Ast,
        node_id: NodeId,
        depth: u32,
        subtree_sizes: &mut [u32],
        depths: &mut [u32],
        nodes_by_production: &mut [Vec<NodeId>],
    ) {
        let idx = node_id.0 as usize;
        depths[idx] = depth;

        if let AstNodeKind::Production(pid) = &ast.nodes[idx].kind {
            nodes_by_production[pid.0 as usize].push(node_id);
        }

        let mut size = 1u32;
        let n_children = ast.nodes[idx].children.len();
        for ci in 0..n_children {
            let child = ast.nodes[idx].children[ci];
            Self::dfs(ast, child, depth + 1, subtree_sizes, depths, nodes_by_production);
            size += subtree_sizes[child.0 as usize];
        }
        subtree_sizes[idx] = size;
    }
}
