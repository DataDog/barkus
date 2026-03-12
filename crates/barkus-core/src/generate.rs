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
    let mut eligible_buf = Vec::new();

    let root = expand_production_gen(grammar, profile, grammar.start, &mut ctx, &mut writer, rng, &mut eligible_buf)?;
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
    let mut eligible_buf = Vec::new();

    let root = expand_production_dec(grammar, profile, grammar.start, &mut ctx, &mut reader, &mut eligible_buf)?;
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
    eligible_buf: &mut Vec<usize>,
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

    eligible_alts(grammar, prod, remaining, eligible_buf);

    let alt_idx = if eligible_buf.is_empty() {
        let chosen = rng.gen_range(0..n_alts);
        writer.write_choice(chosen, n_alts, rng);
        chosen
    } else {
        let chosen_eligible = rng.gen_range(0..eligible_buf.len());
        let alt_idx = eligible_buf[chosen_eligible];
        writer.write_choice(alt_idx, n_alts, rng);
        alt_idx
    };

    let node_id = ctx.ast.new_node(AstNodeKind::Production(prod_id));
    let n_syms = grammar.productions[prod_id].alternatives[alt_idx].symbols.len();

    ctx.depth += 1;
    for i in 0..n_syms {
        let sym_ref = grammar.productions[prod_id].alternatives[alt_idx].symbols[i].clone();
        expand_symbol_ref_gen(grammar, profile, &sym_ref, ctx, writer, rng, node_id, eligible_buf)?;
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
    eligible_buf: &mut Vec<usize>,
) -> Result<(), GenerateError> {
    let count = resolve_modifier_gen(&sym_ref.modifier, profile, writer, rng);

    for _ in 0..count {
        let child = expand_symbol_gen(grammar, profile, sym_ref.symbol, ctx, writer, rng, eligible_buf)?;
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
    eligible_buf: &mut Vec<usize>,
) -> Result<NodeId, GenerateError> {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(tk) => {
            ctx.alloc_node()?;
            let bytes = emit_terminal_gen(tk, writer, rng);
            Ok(ctx.ast.new_node(AstNodeKind::Terminal(bytes)))
        }
        Symbol::NonTerminal(pid) => {
            let pid = *pid;
            expand_production_gen(grammar, profile, pid, ctx, writer, rng, eligible_buf)
        }
    }
}

fn emit_terminal_gen(tk: &TerminalKind, writer: &mut TapeWriter, rng: &mut impl Rng) -> Vec<u8> {
    match tk {
        TerminalKind::Literal(b) => b.clone(),
        TerminalKind::CharClass { ranges, negated } => {
            let valid_bytes = collect_char_class_bytes(ranges, *negated);
            if valid_bytes.is_empty() {
                writer.write_choice(0, 1, rng);
                return vec![0];
            }
            let idx = rng.gen_range(0..valid_bytes.len());
            writer.write_choice(idx, valid_bytes.len(), rng);
            vec![valid_bytes[idx]]
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
    eligible_buf: &mut Vec<usize>,
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

    eligible_alts(grammar, prod, remaining, eligible_buf);

    let raw = reader.choose(n_alts);
    let alt_idx = if eligible_buf.is_empty() || eligible_buf.contains(&raw) {
        raw
    } else {
        eligible_buf[raw % eligible_buf.len()]
    };

    let node_id = ctx.ast.new_node(AstNodeKind::Production(prod_id));
    let n_syms = grammar.productions[prod_id].alternatives[alt_idx].symbols.len();

    ctx.depth += 1;
    for i in 0..n_syms {
        let sym_ref = grammar.productions[prod_id].alternatives[alt_idx].symbols[i].clone();
        expand_symbol_ref_dec(grammar, profile, &sym_ref, ctx, reader, node_id, eligible_buf)?;
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
    eligible_buf: &mut Vec<usize>,
) -> Result<(), GenerateError> {
    let count = resolve_modifier_dec(&sym_ref.modifier, profile, reader);

    for _ in 0..count {
        let child = expand_symbol_dec(grammar, profile, sym_ref.symbol, ctx, reader, eligible_buf)?;
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
    eligible_buf: &mut Vec<usize>,
) -> Result<NodeId, GenerateError> {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(tk) => {
            ctx.alloc_node()?;
            let bytes = emit_terminal_dec(tk, reader);
            Ok(ctx.ast.new_node(AstNodeKind::Terminal(bytes)))
        }
        Symbol::NonTerminal(pid) => {
            let pid = *pid;
            expand_production_dec(grammar, profile, pid, ctx, reader, eligible_buf)
        }
    }
}

fn emit_terminal_dec(tk: &TerminalKind, reader: &mut TapeReader<'_>) -> Vec<u8> {
    match tk {
        TerminalKind::Literal(b) => b.clone(),
        TerminalKind::CharClass { ranges, negated } => {
            let valid_bytes = collect_char_class_bytes(ranges, *negated);
            if valid_bytes.is_empty() {
                reader.choose(1);
                return vec![0];
            }
            let idx = reader.choose(valid_bytes.len());
            vec![valid_bytes[idx]]
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
    buf: &mut Vec<usize>,
) {
    buf.clear();
    for (i, alt) in prod.alternatives.iter().enumerate() {
        let ok = alt.symbols.iter().all(|sr| {
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
        });
        if ok {
            buf.push(i);
        }
    }
}

/// Collect all valid bytes for a character class into a Vec, for uniform sampling.
fn collect_char_class_bytes(ranges: &[(u8, u8)], negated: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for b in 0u8..=255 {
        let in_range = ranges.iter().any(|&(lo, hi)| b >= lo && b <= hi);
        if negated { !in_range } else { in_range }.then(|| bytes.push(b));
    }
    bytes
}
