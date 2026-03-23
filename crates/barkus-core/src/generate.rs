use rand::{Rng, RngExt};

use crate::ast::{Ast, AstNodeKind};
use crate::error::{BudgetKind, GenerateError};
use crate::hooks::SemanticHooks;
use crate::ir::grammar::{GrammarIr, Modifier, Symbol, TerminalKind};
use crate::ir::ids::{NodeId, ProductionId, SymbolId};
use crate::profile::Profile;
use crate::tape::map::TapeMap;
use crate::tape::{DecisionTape, TapeReader, TapeWriter};

// ── Public API (no hooks, backwards-compatible) ──

/// Generate an AST from a grammar using random decisions, recording them to a tape.
pub fn generate(
    grammar: &GrammarIr,
    profile: &Profile,
    rng: &mut impl Rng,
) -> Result<(Ast, DecisionTape, TapeMap), GenerateError> {
    generate_with_hooks(grammar, grammar.start, profile, rng, &mut ())
}

/// Generate an AST starting from a specific production.
///
/// This is the entry point used by [`crate::mutation::ops::subtree_regenerate`] to produce a
/// fresh tape fragment for a single production. By starting generation from an arbitrary
/// production (rather than the grammar root), it yields a self-contained subtree + tape that
/// can be spliced into an existing tape to replace the original subtree.
pub fn generate_from(
    grammar: &GrammarIr,
    start: ProductionId,
    profile: &Profile,
    rng: &mut impl Rng,
) -> Result<(Ast, DecisionTape, TapeMap), GenerateError> {
    generate_with_hooks(grammar, start, profile, rng, &mut ())
}

/// Decode an AST from a grammar using a pre-recorded tape.
pub fn decode(
    grammar: &GrammarIr,
    profile: &Profile,
    tape: &[u8],
) -> Result<(Ast, TapeMap), GenerateError> {
    decode_with_hooks(grammar, profile, tape, &mut ())
}

// ── Public API (with hooks) ──

/// Generate an AST with semantic hooks, starting from a specific production.
pub fn generate_with_hooks<H: SemanticHooks>(
    grammar: &GrammarIr,
    start: ProductionId,
    profile: &Profile,
    rng: &mut impl Rng,
    hooks: &mut H,
) -> Result<(Ast, DecisionTape, TapeMap), GenerateError> {
    let mut ctx = GenCtx::new(profile);
    let mut writer = TapeWriter::new(profile.validity_mode);
    let mut eligible_buf = Vec::new();

    let root = expand_production_gen(grammar, profile, start, &mut ctx, &mut writer, rng, &mut eligible_buf, hooks)?;
    ctx.ast.root = root;

    let tape = writer.finish();
    Ok((ctx.ast, tape, ctx.tape_map))
}

/// Decode an AST with semantic hooks from a pre-recorded tape.
pub fn decode_with_hooks<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    tape: &[u8],
    hooks: &mut H,
) -> Result<(Ast, TapeMap), GenerateError> {
    let mut ctx = GenCtx::new(profile);
    let mut reader = TapeReader::new(tape);
    let mut eligible_buf = Vec::new();

    let root = expand_production_dec(grammar, profile, grammar.start, &mut ctx, &mut reader, &mut eligible_buf, hooks)?;
    ctx.ast.root = root;

    Ok((ctx.ast, ctx.tape_map))
}

struct GenCtx {
    ast: Ast,
    tape_map: TapeMap,
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
            tape_map: TapeMap::new(),
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

#[allow(clippy::too_many_arguments)]
fn expand_production_gen<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    prod_id: ProductionId,
    ctx: &mut GenCtx,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
    eligible_buf: &mut Vec<usize>,
    hooks: &mut H,
) -> Result<NodeId, GenerateError> {
    if ctx.depth >= ctx.max_depth {
        return Err(GenerateError::BudgetExhausted {
            kind: BudgetKind::MaxDepth,
        });
    }
    ctx.alloc_node()?;

    hooks.enter_production(prod_id);

    let prod = &grammar.productions[prod_id];

    // If this production has a semantic hook, consume a tape byte and ask the hook.
    if let Some(hook_name) = &prod.attrs.semantic_hook {
        let hook_name = hook_name.clone();
        let tape_byte = rng.random::<u8>();
        writer.write_choice(tape_byte as usize, 256, rng);

        if let Some(bytes) = hooks.on_production(&hook_name, tape_byte, prod_id) {
            let tape_start = writer.offset() - 1; // the byte we just wrote
            let node_id = ctx.ast.new_node(AstNodeKind::Terminal(bytes));
            // Wrap in a production node so the tape map stays consistent.
            let prod_node = ctx.ast.new_node(AstNodeKind::Production(prod_id));
            ctx.ast.add_child(prod_node, node_id);
            ctx.tape_map.push(tape_start, 1, prod_node, prod_id);
            hooks.exit_production(prod_id);
            return Ok(prod_node);
        }
        // Hook returned None — fall through to normal expansion.
    }

    let remaining = ctx.max_depth - ctx.depth - 1;
    let n_alts = prod.alternatives.len();

    eligible_alts(grammar, prod, remaining, eligible_buf);

    let tape_start = writer.offset();

    let alt_idx = if eligible_buf.is_empty() {
        let chosen = rng.random_range(0..n_alts);
        writer.write_choice(chosen, n_alts, rng);
        chosen
    } else {
        let chosen_eligible = rng.random_range(0..eligible_buf.len());
        let alt_idx = eligible_buf[chosen_eligible];
        writer.write_choice(alt_idx, n_alts, rng);
        alt_idx
    };

    let node_id = ctx.ast.new_node(AstNodeKind::Production(prod_id));
    let n_syms = grammar.productions[prod_id].alternatives[alt_idx].symbols.len();

    ctx.depth += 1;
    for i in 0..n_syms {
        let sym_ref = grammar.productions[prod_id].alternatives[alt_idx].symbols[i].clone();
        expand_symbol_ref_gen(grammar, profile, &sym_ref, ctx, writer, rng, node_id, eligible_buf, hooks)?;
    }
    ctx.depth -= 1;

    ctx.tape_map.push(tape_start, writer.offset() - tape_start, node_id, prod_id);

    hooks.exit_production(prod_id);

    Ok(node_id)
}

#[allow(clippy::too_many_arguments)]
fn expand_symbol_ref_gen<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_ref: &crate::ir::grammar::SymbolRef,
    ctx: &mut GenCtx,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
    parent: NodeId,
    eligible_buf: &mut Vec<usize>,
    hooks: &mut H,
) -> Result<(), GenerateError> {
    record_modifier(&sym_ref.modifier, profile, writer.offset(), &mut ctx.tape_map);

    // Check if the inner symbol can fit in the remaining depth budget.
    // For optional/repetition modifiers, force count to the minimum when it can't.
    let sym_fits = symbol_fits_depth(grammar, sym_ref.symbol, ctx);
    let count = if sym_fits {
        resolve_modifier_gen(&sym_ref.modifier, profile, writer, rng)
    } else {
        resolve_modifier_min(&sym_ref.modifier, profile, writer, rng)
    };

    for _ in 0..count {
        let child = expand_symbol_gen(grammar, profile, sym_ref.symbol, ctx, writer, rng, eligible_buf, hooks)?;
        ctx.ast.add_child(parent, child);
    }
    Ok(())
}

/// Record a modifier's tape position in the `TapeMap` for mutation-time lookup.
///
/// Must be called *before* the modifier byte is consumed by `resolve_modifier_gen` /
/// `resolve_modifier_dec`, because the `offset` parameter captures the tape position where
/// the modifier byte will be written/read. The positional coupling between `record_modifier`
/// and `resolve_modifier_*` is intentional: the TapeMap entry must point to the exact byte
/// that encodes the modifier decision.
fn record_modifier(
    modifier: &Modifier,
    profile: &Profile,
    offset: usize,
    tape_map: &mut crate::tape::map::TapeMap,
) {
    match modifier {
        Modifier::Once => {}
        Modifier::Optional => {
            tape_map.push_optional(offset);
        }
        Modifier::ZeroOrMore { min, max } => {
            let hi = (*max).min(profile.repetition_bounds.1);
            if *min < hi {
                tape_map.push_repetition(offset, *min, hi);
            }
        }
        Modifier::OneOrMore { min, max } => {
            let lo = (*min).max(1);
            let hi = (*max).min(profile.repetition_bounds.1);
            if lo < hi {
                tape_map.push_repetition(offset, lo, hi);
            }
        }
    }
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
            let chosen = rng.random_range(0..2usize);
            writer.write_choice(chosen, 2, rng);
            chosen as u32
        }
        Modifier::ZeroOrMore { min, max } => {
            let hi = (*max).min(profile.repetition_bounds.1);
            let count = rng.random_range(*min..=hi);
            writer.write_repetition(count, *min, hi, rng);
            count
        }
        Modifier::OneOrMore { min, max } => {
            let lo = (*min).max(1);
            let hi = (*max).min(profile.repetition_bounds.1);
            let count = rng.random_range(lo..=hi);
            writer.write_repetition(count, lo, hi, rng);
            count
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn expand_symbol_gen<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_id: SymbolId,
    ctx: &mut GenCtx,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
    eligible_buf: &mut Vec<usize>,
    hooks: &mut H,
) -> Result<NodeId, GenerateError> {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(tk) => {
            ctx.alloc_node()?;
            let bytes = emit_terminal_gen(grammar, tk, writer, rng, hooks);
            Ok(ctx.ast.new_node(AstNodeKind::Terminal(bytes)))
        }
        Symbol::NonTerminal(pid) => {
            let pid = *pid;
            expand_production_gen(grammar, profile, pid, ctx, writer, rng, eligible_buf, hooks)
        }
    }
}

fn emit_terminal_gen<H: SemanticHooks>(
    grammar: &GrammarIr,
    tk: &TerminalKind,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
    hooks: &mut H,
) -> Vec<u8> {
    match tk {
        TerminalKind::Literal(b) => b.clone(),
        TerminalKind::CharClass { ranges, negated } => {
            let valid_bytes = collect_char_class_bytes(ranges, *negated);
            if valid_bytes.is_empty() {
                writer.write_choice(0, 1, rng);
                return vec![0];
            }
            let idx = rng.random_range(0..valid_bytes.len());
            writer.write_choice(idx, valid_bytes.len(), rng);
            vec![valid_bytes[idx]]
        }
        TerminalKind::AnyByte => {
            let byte = rng.random_range(0u8..=255);
            writer.write_choice(byte as usize, 256, rng);
            vec![byte]
        }
        TerminalKind::ByteRange(lo, hi) => {
            let range = (*hi as usize - *lo as usize) + 1;
            let idx = rng.random_range(0..range);
            writer.write_choice(idx, range, rng);
            vec![*lo + idx as u8]
        }
        TerminalKind::TokenPool(pool_id) => {
            // Consume a tape byte for the pool decision.
            let tape_byte = rng.random::<u8>();
            writer.write_choice(tape_byte as usize, 256, rng);

            // Ask the hook first.
            if let Some(bytes) = hooks.on_token_pool(*pool_id, tape_byte) {
                return bytes;
            }

            // Mechanical fallback: expand from the pool's alternatives.
            expand_token_pool_gen(grammar, *pool_id, tape_byte, writer, rng)
        }
    }
}

/// Mechanically expand a token pool entry by picking an alternative and expanding its terminals.
fn expand_token_pool_gen(
    grammar: &GrammarIr,
    pool_id: crate::ir::ids::PoolId,
    tape_byte: u8,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
) -> Vec<u8> {
    let pool_idx = pool_id.0 as usize;
    if pool_idx >= grammar.token_pools.len() {
        return Vec::new();
    }
    let pool = &grammar.token_pools[pool_idx];
    if pool.alternatives.is_empty() {
        return Vec::new();
    }

    let alt_idx = tape_byte as usize % pool.alternatives.len();
    let alt = &pool.alternatives[alt_idx];

    let mut result = Vec::new();
    for sym_ref in &alt.symbols {
        match &grammar.symbols[sym_ref.symbol] {
            Symbol::Terminal(tk) => {
                // Recursively expand terminals within the pool (no hooks — pools are mechanical).
                let bytes = emit_terminal_gen_no_hooks(tk, writer, rng);
                result.extend_from_slice(&bytes);
            }
            Symbol::NonTerminal(_) => {
                // Pool entries shouldn't reference non-terminals, but be defensive.
            }
        }
    }
    result
}

/// Terminal expansion without hooks, for mechanical token pool expansion.
fn emit_terminal_gen_no_hooks(
    tk: &TerminalKind,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
) -> Vec<u8> {
    match tk {
        TerminalKind::Literal(b) => b.clone(),
        TerminalKind::CharClass { ranges, negated } => {
            let valid_bytes = collect_char_class_bytes(ranges, *negated);
            if valid_bytes.is_empty() {
                writer.write_choice(0, 1, rng);
                return vec![0];
            }
            let idx = rng.random_range(0..valid_bytes.len());
            writer.write_choice(idx, valid_bytes.len(), rng);
            vec![valid_bytes[idx]]
        }
        TerminalKind::AnyByte => {
            let byte = rng.random_range(0u8..=255);
            writer.write_choice(byte as usize, 256, rng);
            vec![byte]
        }
        TerminalKind::ByteRange(lo, hi) => {
            let range = (*hi as usize - *lo as usize) + 1;
            let idx = rng.random_range(0..range);
            writer.write_choice(idx, range, rng);
            vec![*lo + idx as u8]
        }
        TerminalKind::TokenPool(_) => Vec::new(),
    }
}

// ── Decode path (reader) ──

#[allow(clippy::too_many_arguments)]
fn expand_production_dec<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    prod_id: ProductionId,
    ctx: &mut GenCtx,
    reader: &mut TapeReader<'_>,
    eligible_buf: &mut Vec<usize>,
    hooks: &mut H,
) -> Result<NodeId, GenerateError> {
    if ctx.depth >= ctx.max_depth {
        return Err(GenerateError::BudgetExhausted {
            kind: BudgetKind::MaxDepth,
        });
    }
    ctx.alloc_node()?;

    hooks.enter_production(prod_id);

    let prod = &grammar.productions[prod_id];

    // If this production has a semantic hook, consume a tape byte and ask the hook.
    if let Some(hook_name) = &prod.attrs.semantic_hook {
        let hook_name = hook_name.clone();
        let tape_byte = reader.choose(256) as u8;

        if let Some(bytes) = hooks.on_production(&hook_name, tape_byte, prod_id) {
            let tape_start = reader.offset() - 1;
            let node_id = ctx.ast.new_node(AstNodeKind::Terminal(bytes));
            let prod_node = ctx.ast.new_node(AstNodeKind::Production(prod_id));
            ctx.ast.add_child(prod_node, node_id);
            ctx.tape_map.push(tape_start, 1, prod_node, prod_id);
            hooks.exit_production(prod_id);
            return Ok(prod_node);
        }
    }

    let remaining = ctx.max_depth - ctx.depth - 1;
    let n_alts = prod.alternatives.len();

    eligible_alts(grammar, prod, remaining, eligible_buf);

    let tape_start = reader.offset();

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
        expand_symbol_ref_dec(grammar, profile, &sym_ref, ctx, reader, node_id, eligible_buf, hooks)?;
    }
    ctx.depth -= 1;

    ctx.tape_map.push(tape_start, reader.offset() - tape_start, node_id, prod_id);

    hooks.exit_production(prod_id);

    Ok(node_id)
}

#[allow(clippy::too_many_arguments)]
fn expand_symbol_ref_dec<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_ref: &crate::ir::grammar::SymbolRef,
    ctx: &mut GenCtx,
    reader: &mut TapeReader<'_>,
    parent: NodeId,
    eligible_buf: &mut Vec<usize>,
    hooks: &mut H,
) -> Result<(), GenerateError> {
    record_modifier(&sym_ref.modifier, profile, reader.offset(), &mut ctx.tape_map);

    // Always consume the tape bytes to keep the reader aligned, then clamp
    // the count to 0 when the inner symbol can't fit in the remaining depth.
    // This is essential because fuzzers mutate tapes arbitrarily — a mutated
    // tape can encode a high repetition count at any depth.
    let raw_count = resolve_modifier_dec(&sym_ref.modifier, profile, reader);
    let count = if symbol_fits_depth(grammar, sym_ref.symbol, ctx) {
        raw_count
    } else {
        match &sym_ref.modifier {
            Modifier::Once => 1,
            Modifier::Optional | Modifier::ZeroOrMore { .. } => 0,
            Modifier::OneOrMore { min, .. } => (*min).max(1),
        }
    };

    for _ in 0..count {
        let child = expand_symbol_dec(grammar, profile, sym_ref.symbol, ctx, reader, eligible_buf, hooks)?;
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

fn expand_symbol_dec<H: SemanticHooks>(
    grammar: &GrammarIr,
    profile: &Profile,
    sym_id: SymbolId,
    ctx: &mut GenCtx,
    reader: &mut TapeReader<'_>,
    eligible_buf: &mut Vec<usize>,
    hooks: &mut H,
) -> Result<NodeId, GenerateError> {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(tk) => {
            ctx.alloc_node()?;
            let bytes = emit_terminal_dec(grammar, tk, reader, hooks);
            Ok(ctx.ast.new_node(AstNodeKind::Terminal(bytes)))
        }
        Symbol::NonTerminal(pid) => {
            let pid = *pid;
            expand_production_dec(grammar, profile, pid, ctx, reader, eligible_buf, hooks)
        }
    }
}

fn emit_terminal_dec<H: SemanticHooks>(
    grammar: &GrammarIr,
    tk: &TerminalKind,
    reader: &mut TapeReader<'_>,
    hooks: &mut H,
) -> Vec<u8> {
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
        TerminalKind::TokenPool(pool_id) => {
            let tape_byte = reader.choose(256) as u8;

            if let Some(bytes) = hooks.on_token_pool(*pool_id, tape_byte) {
                return bytes;
            }

            expand_token_pool_dec(grammar, *pool_id, tape_byte, reader)
        }
    }
}

/// Mechanically expand a token pool entry during decode.
fn expand_token_pool_dec(
    grammar: &GrammarIr,
    pool_id: crate::ir::ids::PoolId,
    tape_byte: u8,
    reader: &mut TapeReader<'_>,
) -> Vec<u8> {
    let pool_idx = pool_id.0 as usize;
    if pool_idx >= grammar.token_pools.len() {
        return Vec::new();
    }
    let pool = &grammar.token_pools[pool_idx];
    if pool.alternatives.is_empty() {
        return Vec::new();
    }

    let alt_idx = tape_byte as usize % pool.alternatives.len();
    let alt = &pool.alternatives[alt_idx];

    let mut result = Vec::new();
    for sym_ref in &alt.symbols {
        match &grammar.symbols[sym_ref.symbol] {
            Symbol::Terminal(tk) => {
                let bytes = emit_terminal_dec_no_hooks(tk, reader);
                result.extend_from_slice(&bytes);
            }
            Symbol::NonTerminal(_) => {}
        }
    }
    result
}

/// Terminal expansion without hooks, for mechanical token pool decode.
fn emit_terminal_dec_no_hooks(tk: &TerminalKind, reader: &mut TapeReader<'_>) -> Vec<u8> {
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
        TerminalKind::TokenPool(_) => Vec::new(),
    }
}

/// Check whether a symbol's min_depth fits in the remaining depth budget.
///
/// Returns `true` for terminals (always fit) and for non-terminals whose
/// `min_depth` is within the remaining budget at the current expansion depth.
fn symbol_fits_depth(grammar: &GrammarIr, sym_id: SymbolId, ctx: &GenCtx) -> bool {
    match &grammar.symbols[sym_id] {
        Symbol::Terminal(_) => true,
        Symbol::NonTerminal(pid) => {
            // After entering the current production (depth already incremented by
            // the caller), a child production will increment depth once more,
            // so `remaining` is what the *child* will see.
            let remaining = ctx.max_depth.saturating_sub(ctx.depth + 1);
            grammar.productions[*pid].attrs.min_depth <= remaining
        }
    }
}

/// Resolve a modifier to its minimum count, writing the corresponding tape
/// entry so that the decode path stays consistent.
///
/// Used when the inner symbol cannot fit in the remaining depth budget, so we
/// must emit the fewest possible expansions (0 for Optional/ZeroOrMore).
fn resolve_modifier_min(
    modifier: &Modifier,
    profile: &Profile,
    writer: &mut TapeWriter,
    rng: &mut impl Rng,
) -> u32 {
    match modifier {
        Modifier::Once => {
            // Can't avoid expanding — the caller's eligible_alts should have
            // prevented us from reaching here, but return 1 for correctness.
            1
        }
        Modifier::Optional => {
            writer.write_choice(0, 2, rng); // encode "not present"
            0
        }
        Modifier::ZeroOrMore { min, max } => {
            let hi = (*max).min(profile.repetition_bounds.1);
            let count = *min; // smallest allowed
            writer.write_repetition(count, *min, hi, rng);
            count
        }
        Modifier::OneOrMore { min, max } => {
            let lo = (*min).max(1);
            let hi = (*max).min(profile.repetition_bounds.1);
            writer.write_repetition(lo, lo, hi, rng);
            lo
        }
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
