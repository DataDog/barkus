use super::grammar::{GrammarIr, Modifier, Symbol};

/// Fixed-point iteration to compute `min_depth` for each production.
///
/// A terminal has depth 0. A production's min_depth is 1 + the minimum over its
/// alternatives of the maximum min_depth of its required symbols.
pub fn compute_min_depths(ir: &mut GrammarIr) {
    let n = ir.productions.len();
    let mut depths = vec![u32::MAX; n];

    let mut changed = true;
    while changed {
        changed = false;
        for (i, prod) in ir.productions.iter().enumerate() {
            let mut best_alt = u32::MAX;

            for alt in &prod.alternatives {
                let mut alt_depth: u32 = 0;

                for sym_ref in &alt.symbols {
                    let required = match &sym_ref.modifier {
                        Modifier::Optional => false,
                        Modifier::ZeroOrMore { min, .. } => *min > 0,
                        _ => true,
                    };

                    if !required {
                        continue;
                    }

                    let sym_depth = match &ir.symbols[sym_ref.symbol] {
                        Symbol::Terminal(_) => 0,
                        Symbol::NonTerminal(pid) => depths[pid.0 as usize],
                    };

                    alt_depth = alt_depth.max(sym_depth);
                }

                best_alt = best_alt.min(alt_depth);
            }

            let new_depth = best_alt.saturating_add(1);
            if new_depth < depths[i] {
                depths[i] = new_depth;
                changed = true;
            }
        }
    }

    for (i, prod) in ir.productions.iter_mut().enumerate() {
        prod.attrs.min_depth = depths[i];
    }
}

/// Mark productions that are (transitively) self-referential.
pub fn mark_recursive(ir: &mut GrammarIr) {
    let n = ir.productions.len();

    // Build adjacency list first (avoids borrow issues).
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, prod) in ir.productions.iter().enumerate() {
        for alt in &prod.alternatives {
            for sym_ref in &alt.symbols {
                if let Symbol::NonTerminal(pid) = &ir.symbols[sym_ref.symbol] {
                    adj[i].push(pid.0 as usize);
                }
            }
        }
    }

    // DFS from each production to check if it can reach itself.
    for start in 0..n {
        let mut visited = vec![false; n];
        let mut stack: Vec<usize> = adj[start].clone();

        while let Some(current) = stack.pop() {
            if current == start {
                ir.productions[start].attrs.is_recursive = true;
                break;
            }
            if visited[current] {
                continue;
            }
            visited[current] = true;
            for &next in &adj[current] {
                if !visited[next] || next == start {
                    stack.push(next);
                }
            }
        }
    }
}
