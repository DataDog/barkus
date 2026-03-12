use rand::Rng;

use crate::ast::{Ast, AstNodeKind};
use crate::error::{BudgetKind, GenerateError};
use crate::ir::grammar::{GrammarIr, Modifier, Symbol, TerminalKind};
use crate::ir::ids::{NodeId, ProductionId, SymbolId};
use crate::profile::Profile;
use crate::tape::{DecisionTape, TapeReader, TapeWriter};

/// Generate an AST from a grammar using random decisions, recording them to a tape.
pub fn generate(
    grammar: &GrammarIr,
    profile: &Profile,
    rng: &mut impl Rng,
) -> Result<(Ast, DecisionTape), GenerateError> {
    let mut ctx = GenCtx::new(profile);
    let mut writer = TapeWriter::new(profile.validity_mode);

    let root = expand_production_gen(grammar, profile, grammar.start, &mut ctx, &mut writer, rng)?;
    ctx.ast.root = root;

    Ok((ctx.ast, writer.finish()))
}

/// Decode an AST from a grammar using a pre-recorded tape.
pub fn decode(
    grammar: &GrammarIr,
    profile: &Profile,
    tape: &[u8],
) -> Result<Ast, GenerateError> {
    let mut ctx = GenCtx::new(profile);
    let mut reader = TapeReader::new(tape);

    let root = expand_production_dec(grammar, profile, grammar.start, &mut ctx, &mut reader)?;
    ctx.ast.root = root;

    Ok(ctx.ast)
}

struct GenCtx {
    ast: Ast,
    depth: u32,
    total_nodes: u32,
    max_depth: u32,
    max_total_nodes: u32,
}

impl GenCtx {
    fn new(profile: &Profile) -> Self {
        Self {
            ast: Ast {
                nodes: Vec::new(),
                root: NodeId(0),
            },
            depth: 0,
            total_nodes: 0,
            max_depth: profile.max_depth,
            max_total_nodes: profile.max_total_nodes,
        }
    }

    fn alloc_node(&mut self) -> Result<(), GenerateError> {
        self.total_nodes += 1;
        if self.total_nodes > self.max_total_nodes {
            return Err(GenerateError::BudgetExhausted {
                kind: BudgetKind::MaxTotalNodes,
            });
        }
        Ok(())
    }
}

// ── Generate path (writer + rng) ──

fn expand_production_gen(
    grammar: &GrammarIr,
    profile: &Profile,
    prod_id: ProductionId,
    ctx: &mut GenCtx,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
) -> Result<NodeId, GenerateError> {
    if ctx.depth >= ctx.max_depth {
        return Err(GenerateError::BudgetExhausted {
            kind: BudgetKind::MaxDepth,
        });
    }
    ctx.alloc_node()?;

    let prod = &grammar.productions[prod_id];
    let remaining = ctx.max_depth - ctx.depth - 1;
    let n_alts = prod.alternatives.len();

    let eligible = eligible_alts(grammar, prod, remaining);

    let alt_idx = if eligible.is_empty() {
        let chosen = rng.gen_range(0..n_alts);
        writer.write_choice(chosen, n_alts, rng);
        chosen
    } else {
        let chosen_eligible = rng.gen_range(0..eligible.len());
        let alt_idx = eligible[chosen_eligible];
        writer.write_choice(alt_idx, n_alts, rng);
        alt_idx
    };

    let node_id = ctx.ast.new_node(AstNodeKind::Production(prod_id));
    let alt_symbols: Vec<_> = prod.alternatives[alt_idx].symbols.clone();

    ctx.depth += 1;
    for sym_ref in &alt_symbols {
        expand_symbol_ref_gen(grammar, profile, sym_ref, ctx, writer, rng, node_id)?;
    }
    ctx.depth -= 1;

    Ok(node_id)
}

fn expand_symbol_ref_gen(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_ref: &crate::ir::grammar::SymbolRef,
    ctx: &mut GenCtx,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
    parent: NodeId,
) -> Result<(), GenerateError> {
    let count = resolve_modifier_gen(&sym_ref.modifier, profile, writer, rng);

    for _ in 0..count {
        let child = expand_symbol_gen(grammar, profile, sym_ref.symbol, ctx, writer, rng)?;
        ctx.ast.add_child(parent, child);
    }
    Ok(())
}

fn resolve_modifier_gen(
    modifier: &Modifier,
    profile: &Profile,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
) -> u32 {
    match modifier {
        Modifier::Once => 1,
        Modifier::Optional => {
            let chosen = rng.gen_range(0..2usize);
            writer.write_choice(chosen, 2, rng);
            chosen as u32
        }
        Modifier::ZeroOrMore { min, max } => {
            let hi = (*max).min(profile.repetition_bounds.1);
            let count = rng.gen_range(*min..=hi);
            writer.write_repetition(count, *min, hi, rng);
            count
        }
        Modifier::OneOrMore { min, max } => {
            let lo = (*min).max(1);
            let hi = (*max).min(profile.repetition_bounds.1);
            let count = rng.gen_range(lo..=hi);
            writer.write_repetition(count, lo, hi, rng);
            count
        }
    }
}

fn expand_symbol_gen(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_id: SymbolId,
    ctx: &mut GenCtx,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
) -> Result<NodeId, GenerateError> {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(tk) => {
            ctx.alloc_node()?;
            let bytes = emit_terminal_gen(tk, writer, rng);
            Ok(ctx.ast.new_node(AstNodeKind::Terminal(bytes)))
        }
        Symbol::NonTerminal(pid) => {
            let pid = *pid;
            expand_production_gen(grammar, profile, pid, ctx, writer, rng)
        }
    }
}

fn emit_terminal_gen(tk: &TerminalKind, writer: &mut TapeWriter, rng: &mut impl Rng) -> Vec<u8> {
    match tk {
        TerminalKind::Literal(b) => b.clone(),
        TerminalKind::CharClass { ranges, negated } => {
            let byte = rng.gen_range(0u8..=255);
            // No tape byte for char class — deterministic from literal or use rng directly.
            // Actually we need tape determinism, so write the byte.
            writer.write_choice(byte as usize, 256, rng);
            let in_range = ranges.iter().any(|&(lo, hi)| byte >= lo && byte <= hi);
            let valid = if *negated { !in_range } else { in_range };
            if valid {
                vec![byte]
            } else if !ranges.is_empty() {
                vec![ranges[0].0]
            } else {
                vec![0]
            }
        }
        TerminalKind::AnyByte => {
            let byte = rng.gen_range(0u8..=255);
            writer.write_choice(byte as usize, 256, rng);
            vec![byte]
        }
        TerminalKind::ByteRange(lo, hi) => {
            let range = (*hi as usize - *lo as usize) + 1;
            let idx = rng.gen_range(0..range);
            writer.write_choice(idx, range, rng);
            vec![*lo + idx as u8]
        }
        TerminalKind::TokenPool(_) => vec![],
    }
}

// ── Decode path (reader) ──

fn expand_production_dec(
    grammar: &GrammarIr,
    profile: &Profile,
    prod_id: ProductionId,
    ctx: &mut GenCtx,
    reader: &mut TapeReader<'_>,
) -> Result<NodeId, GenerateError> {
    if ctx.depth >= ctx.max_depth {
        return Err(GenerateError::BudgetExhausted {
            kind: BudgetKind::MaxDepth,
        });
    }
    ctx.alloc_node()?;

    let prod = &grammar.productions[prod_id];
    let remaining = ctx.max_depth - ctx.depth - 1;
    let n_alts = prod.alternatives.len();

    let eligible = eligible_alts(grammar, prod, remaining);

    let raw = reader.choose(n_alts);
    let alt_idx = if eligible.is_empty() || eligible.contains(&raw) {
        raw
    } else {
        eligible[raw % eligible.len()]
    };

    let node_id = ctx.ast.new_node(AstNodeKind::Production(prod_id));
    let alt_symbols: Vec<_> = prod.alternatives[alt_idx].symbols.clone();

    ctx.depth += 1;
    for sym_ref in &alt_symbols {
        expand_symbol_ref_dec(grammar, profile, sym_ref, ctx, reader, node_id)?;
    }
    ctx.depth -= 1;

    Ok(node_id)
}

fn expand_symbol_ref_dec(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_ref: &crate::ir::grammar::SymbolRef,
    ctx: &mut GenCtx,
    reader: &mut TapeReader<'_>,
    parent: NodeId,
) -> Result<(), GenerateError> {
    let count = resolve_modifier_dec(&sym_ref.modifier, profile, reader);

    for _ in 0..count {
        let child = expand_symbol_dec(grammar, profile, sym_ref.symbol, ctx, reader)?;
        ctx.ast.add_child(parent, child);
    }
    Ok(())
}

fn resolve_modifier_dec(
    modifier: &Modifier,
    profile: &Profile,
    reader: &mut TapeReader<'_>,
) -> u32 {
    match modifier {
        Modifier::Once => 1,
        Modifier::Optional => reader.choose(2) as u32,
        Modifier::ZeroOrMore { min, max } => {
            let hi = (*max).min(profile.repetition_bounds.1);
            reader.repetition(*min, hi)
        }
        Modifier::OneOrMore { min, max } => {
            let lo = (*min).max(1);
            let hi = (*max).min(profile.repetition_bounds.1);
            reader.repetition(lo, hi)
        }
    }
}

fn expand_symbol_dec(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_id: SymbolId,
    ctx: &mut GenCtx,
    reader: &mut TapeReader<'_>,
) -> Result<NodeId, GenerateError> {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(tk) => {
            ctx.alloc_node()?;
            let bytes = emit_terminal_dec(tk, reader);
            Ok(ctx.ast.new_node(AstNodeKind::Terminal(bytes)))
        }
        Symbol::NonTerminal(pid) => {
            let pid = *pid;
            expand_production_dec(grammar, profile, pid, ctx, reader)
        }
    }
}

fn emit_terminal_dec(tk: &TerminalKind, reader: &mut TapeReader<'_>) -> Vec<u8> {
    match tk {
        TerminalKind::Literal(b) => b.clone(),
        TerminalKind::CharClass { ranges, negated } => {
            let byte = reader.choose(256) as u8;
            let in_range = ranges.iter().any(|&(lo, hi)| byte >= lo && byte <= hi);
            let valid = if *negated { !in_range } else { in_range };
            if valid {
                vec![byte]
            } else if !ranges.is_empty() {
                vec![ranges[0].0]
            } else {
                vec![0]
            }
        }
        TerminalKind::AnyByte => {
            let byte = reader.choose(256) as u8;
            vec![byte]
        }
        TerminalKind::ByteRange(lo, hi) => {
            let range = (*hi as usize - *lo as usize) + 1;
            let idx = reader.choose(range);
            vec![*lo + idx as u8]
        }
        TerminalKind::TokenPool(_) => vec![],
    }
}

// ── Shared helpers ──

fn eligible_alts(
    grammar: &GrammarIr,
    prod: &crate::ir::grammar::Production,
    remaining: u32,
) -> Vec<usize> {
    prod.alternatives
        .iter()
        .enumerate()
        .filter(|(_, alt)| {
            alt.symbols.iter().all(|sr| {
                let required = match &sr.modifier {
                    Modifier::Optional => false,
                    Modifier::ZeroOrMore { min, .. } => *min > 0,
                    _ => true,
                };
                if !required {
                    return true;
                }
                match &grammar.symbols[sr.symbol] {
                    Symbol::Terminal(_) => true,
                    Symbol::NonTerminal(pid) => {
                        grammar.productions[*pid].attrs.min_depth <= remaining
                    }
                }
            })
        })
        .map(|(i, _)| i)
        .collect()
}
