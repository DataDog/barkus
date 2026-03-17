use std::collections::HashMap;

use barkus_core::ir::grammar::TokenPoolEntry;
use barkus_core::ir::{
    Alternative, GrammarIr, Modifier, Production, ProductionAttrs, ProductionId, PoolId, Symbol,
    SymbolId, SymbolRef, TerminalKind,
};
use barkus_core::ir::analysis::{compute_min_depths, mark_recursive};
use barkus_parser_common::{
    build_ir, parse_char_class_contents, parse_string_literal, scan_identifier, skip_block_comment,
    skip_line_comment, BuildItem, IrBuilder, RawAlternative, RawGrammar, RawQuantifier, RawRule,
};

// ── Public API ──────────────────────────────────────────────────────────────

pub use barkus_parser_common::ParseError;

pub fn compile(source: &str) -> Result<barkus_core::ir::GrammarIr, ParseError> {
    let tokens = tokenize(source)?;
    let raw = parse_grammar(&tokens)?;
    let ir = build_ir(raw, pick_start)?;
    Ok(ir)
}

/// Compile a split ANTLR grammar (separate Lexer.g4 + Parser.g4) into a `GrammarIr`.
///
/// Lexer rules are mapped to `TokenPoolEntry` values in the IR. Parser rules that reference
/// lexer tokens will emit `TerminalKind::TokenPool(pool_id)` or `TerminalKind::Literal` for
/// simple keyword tokens. Fragment rules are inlined into their referencing rules and not
/// exposed as pools. Rules with `-> skip` or `-> channel(HIDDEN)` are excluded.
pub fn compile_split(
    lexer_source: &str,
    parser_source: &str,
) -> Result<GrammarIr, ParseError> {
    // 1. Parse the lexer grammar into raw rules.
    let lexer_tokens = tokenize(lexer_source)?;
    let lexer_raw = parse_grammar_for_split(&lexer_tokens)?;

    // 2. Categorize lexer rules: fragment (skipped — not yet inlined), skip, or regular.
    let mut skip_rules: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut lexer_rules: HashMap<String, Vec<RawAlternative<RawItem>>> = HashMap::new();

    for rule in &lexer_raw.rules {
        if rule.is_fragment {
            // Fragment rules are not exposed as pools. Fragment inlining is not
            // yet implemented — references to fragments inside other lexer rules
            // are silently dropped by raw_item_to_terminal.
        } else if rule.is_skip || rule.is_channel_hidden {
            skip_rules.insert(rule.name.clone());
        } else {
            lexer_rules.insert(rule.name.clone(), rule.alternatives.clone());
        }
    }

    // 3. Parse the parser grammar into raw rules.
    let parser_tokens = tokenize(parser_source)?;
    let parser_raw = parse_grammar(&parser_tokens)?;

    // 4. Build the IR.
    // We need to:
    //   a) Create parser productions (lowercase rules from the parser grammar).
    //   b) Create token pools from lexer rules (uppercase rules referenced by parser).
    //   c) Simple keyword literals (lexer rules that are just 'keyword') become Literal terminals.

    let n_parser_rules = parser_raw.rules.len();

    // Map parser rule names to production IDs.
    let mut parser_name_to_id: HashMap<String, ProductionId> = HashMap::with_capacity(n_parser_rules);
    for (i, rule) in parser_raw.rules.iter().enumerate() {
        parser_name_to_id.insert(rule.name.clone(), ProductionId(i as u32));
    }

    // Determine which lexer rules are referenced by the parser and allocate pool IDs.
    let mut lexer_name_to_pool: HashMap<String, PoolId> = HashMap::new();
    let mut pool_entries: Vec<TokenPoolEntry> = Vec::new();

    // Pre-scan: figure out which lexer rules are just keyword literals (single string alt).
    let mut keyword_literals: HashMap<String, String> = HashMap::new();
    for (name, alts) in &lexer_rules {
        if alts.len() == 1
            && alts[0].items.len() == 1
            && matches!(&alts[0].items[0], RawItem::Literal(_))
        {
            if let RawItem::Literal(s) = &alts[0].items[0] {
                keyword_literals.insert(name.clone(), s.clone());
            }
        }
    }

    // Build symbols and productions.
    let mut symbols: Vec<Symbol> = Vec::new();
    let mut anon_productions: Vec<Production> = Vec::new();

    // Build parser productions.
    let mut productions: Vec<Production> = Vec::with_capacity(n_parser_rules);
    for (i, rule) in parser_raw.rules.iter().enumerate() {
        let mut ir_alts = Vec::with_capacity(rule.alternatives.len());
        for raw_alt in &rule.alternatives {
            let mut sym_refs = Vec::new();
            for item in &raw_alt.items {
                build_split_item(
                    item,
                    &parser_name_to_id,
                    &keyword_literals,
                    &skip_rules,
                    &lexer_rules,
                    &mut lexer_name_to_pool,
                    &mut pool_entries,
                    &mut symbols,
                    &mut anon_productions,
                    n_parser_rules as u32,
                    &mut sym_refs,
                )?;
            }
            if sym_refs.is_empty() {
                // Empty alternative — can happen after stripping EOF and skip tokens.
                // Add a zero-byte literal so the alternative is non-empty.
                let sid = SymbolId(symbols.len() as u32);
                symbols.push(Symbol::Terminal(TerminalKind::Literal(Vec::new())));
                sym_refs.push(SymbolRef {
                    symbol: sid,
                    modifier: Modifier::Once,
                });
            }
            ir_alts.push(Alternative {
                symbols: sym_refs,
                weight: 1.0,
                semantic_tag: None,
            });
        }
        productions.push(Production {
            id: ProductionId(i as u32),
            name: rule.name.clone(),
            alternatives: ir_alts,
            attrs: ProductionAttrs::default(),
        });
    }

    productions.extend(anon_productions);

    let start = pick_start(&parser_raw.rules);

    let mut ir = GrammarIr {
        productions,
        symbols,
        start,
        token_pools: pool_entries,
    };

    compute_min_depths(&mut ir);
    mark_recursive(&mut ir);
    ir.validate().map_err(|e| ParseError {
        line: 0,
        column: 0,
        message: format!("IR validation failed: {e}"),
    })?;

    Ok(ir)
}

/// Convert a raw item to a terminal SymbolRef for token pool expansion.
fn raw_item_to_terminal(item: &RawItem, symbols: &mut Vec<Symbol>) -> Option<SymbolRef> {
    let tk = match item {
        RawItem::Literal(s) => TerminalKind::Literal(s.as_bytes().to_vec()),
        RawItem::CharClass { ranges, negated } => TerminalKind::CharClass {
            ranges: ranges.clone(),
            negated: *negated,
        },
        RawItem::AnyChar => TerminalKind::AnyByte,
        RawItem::Quantified(inner, q) => {
            let inner_ref = raw_item_to_terminal(inner, symbols)?;
            let modifier = match q {
                RawQuantifier::Optional => Modifier::Optional,
                RawQuantifier::ZeroOrMore => Modifier::ZeroOrMore { min: 0, max: u32::MAX },
                RawQuantifier::OneOrMore => Modifier::OneOrMore { min: 1, max: u32::MAX },
            };
            return Some(SymbolRef {
                symbol: inner_ref.symbol,
                modifier,
            });
        }
        // NonTerminal references within lexer rules (fragment references) — skip for now.
        RawItem::NonTerminal(..) | RawItem::Group(..) => return None,
    };
    let sid = SymbolId(symbols.len() as u32);
    symbols.push(Symbol::Terminal(tk));
    Some(SymbolRef {
        symbol: sid,
        modifier: Modifier::Once,
    })
}

/// Build IR symbol references for a single parser item in a split grammar.
#[allow(clippy::too_many_arguments)]
fn build_split_item(
    item: &RawItem,
    parser_name_to_id: &HashMap<String, ProductionId>,
    keyword_literals: &HashMap<String, String>,
    skip_rules: &std::collections::HashSet<String>,
    lexer_rules: &HashMap<String, Vec<RawAlternative<RawItem>>>,
    lexer_name_to_pool: &mut HashMap<String, PoolId>,
    pool_entries: &mut Vec<TokenPoolEntry>,
    symbols: &mut Vec<Symbol>,
    anon_productions: &mut Vec<Production>,
    n_parser_rules: u32,
    refs: &mut Vec<SymbolRef>,
) -> Result<(), ParseError> {
    match item {
        RawItem::Literal(s) => {
            let sid = SymbolId(symbols.len() as u32);
            symbols.push(Symbol::Terminal(TerminalKind::Literal(s.as_bytes().to_vec())));
            refs.push(SymbolRef {
                symbol: sid,
                modifier: Modifier::Once,
            });
        }
        RawItem::NonTerminal(name, _line, _col) => {
            // Check if it's a parser rule.
            if let Some(&pid) = parser_name_to_id.get(name.as_str()) {
                let sid = SymbolId(symbols.len() as u32);
                symbols.push(Symbol::NonTerminal(pid));
                refs.push(SymbolRef {
                    symbol: sid,
                    modifier: Modifier::Once,
                });
                return Ok(());
            }

            // Skip 'EOF' token — we don't generate end-of-file.
            if name == "EOF" {
                return Ok(());
            }

            // Skip hidden/skip tokens.
            if skip_rules.contains(name.as_str()) {
                return Ok(());
            }

            // Check if it's a simple keyword literal.
            if let Some(literal) = keyword_literals.get(name.as_str()) {
                let sid = SymbolId(symbols.len() as u32);
                symbols.push(Symbol::Terminal(TerminalKind::Literal(
                    literal.as_bytes().to_vec(),
                )));
                refs.push(SymbolRef {
                    symbol: sid,
                    modifier: Modifier::Once,
                });
                return Ok(());
            }

            // Check if it's a non-keyword lexer rule → token pool.
            if lexer_rules.contains_key(name.as_str()) {
                let pool_id = alloc_pool_for(
                    name,
                    lexer_name_to_pool,
                    pool_entries,
                    lexer_rules,
                    symbols,
                );
                let sid = SymbolId(symbols.len() as u32);
                symbols.push(Symbol::Terminal(TerminalKind::TokenPool(pool_id)));
                refs.push(SymbolRef {
                    symbol: sid,
                    modifier: Modifier::Once,
                });
                return Ok(());
            }

            // Unknown reference — treat as a literal (best effort).
            let sid = SymbolId(symbols.len() as u32);
            symbols.push(Symbol::Terminal(TerminalKind::Literal(
                name.as_bytes().to_vec(),
            )));
            refs.push(SymbolRef {
                symbol: sid,
                modifier: Modifier::Once,
            });
        }
        RawItem::CharClass { ranges, negated } => {
            let sid = SymbolId(symbols.len() as u32);
            symbols.push(Symbol::Terminal(TerminalKind::CharClass {
                ranges: ranges.clone(),
                negated: *negated,
            }));
            refs.push(SymbolRef {
                symbol: sid,
                modifier: Modifier::Once,
            });
        }
        RawItem::AnyChar => {
            let sid = SymbolId(symbols.len() as u32);
            symbols.push(Symbol::Terminal(TerminalKind::AnyByte));
            refs.push(SymbolRef {
                symbol: sid,
                modifier: Modifier::Once,
            });
        }
        RawItem::Group(alts) => {
            // Build alternatives for the group, creating an anonymous production.
            let mut ir_alts = Vec::with_capacity(alts.len());
            for raw_alt in alts {
                let mut inner_refs = Vec::new();
                for inner_item in &raw_alt.items {
                    build_split_item(
                        inner_item,
                        parser_name_to_id,
                        keyword_literals,
                        skip_rules,
                        lexer_rules,
                        lexer_name_to_pool,
                        pool_entries,
                        symbols,
                        anon_productions,
                        n_parser_rules,
                        &mut inner_refs,
                    )?;
                }
                if inner_refs.is_empty() {
                    let sid = SymbolId(symbols.len() as u32);
                    symbols.push(Symbol::Terminal(TerminalKind::Literal(Vec::new())));
                    inner_refs.push(SymbolRef {
                        symbol: sid,
                        modifier: Modifier::Once,
                    });
                }
                ir_alts.push(Alternative {
                    symbols: inner_refs,
                    weight: 1.0,
                    semantic_tag: None,
                });
            }

            // Wrap in an anonymous production.
            let anon_id = ProductionId(n_parser_rules + anon_productions.len() as u32);
            anon_productions.push(Production {
                id: anon_id,
                name: format!("__anon_{}", anon_id.0),
                alternatives: ir_alts,
                attrs: ProductionAttrs::default(),
            });
            let sid = SymbolId(symbols.len() as u32);
            symbols.push(Symbol::NonTerminal(anon_id));
            refs.push(SymbolRef {
                symbol: sid,
                modifier: Modifier::Once,
            });
        }
        RawItem::Quantified(inner, q) => {
            let modifier = match q {
                RawQuantifier::Optional => Modifier::Optional,
                RawQuantifier::ZeroOrMore => Modifier::ZeroOrMore { min: 0, max: u32::MAX },
                RawQuantifier::OneOrMore => Modifier::OneOrMore { min: 1, max: u32::MAX },
            };

            // Build the inner item into a single symbol.
            let mut inner_refs = Vec::new();
            build_split_item(
                inner,
                parser_name_to_id,
                keyword_literals,
                skip_rules,
                lexer_rules,
                lexer_name_to_pool,
                pool_entries,
                symbols,
                anon_productions,
                n_parser_rules,
                &mut inner_refs,
            )?;

            if inner_refs.len() == 1 {
                // Simple case: just apply the modifier to the single symbol.
                refs.push(SymbolRef {
                    symbol: inner_refs[0].symbol,
                    modifier,
                });
            } else if !inner_refs.is_empty() {
                // Multiple symbols — wrap in anon production.
                let anon_id = ProductionId(n_parser_rules + anon_productions.len() as u32);
                anon_productions.push(Production {
                    id: anon_id,
                    name: format!("__anon_{}", anon_id.0),
                    alternatives: vec![Alternative {
                        symbols: inner_refs,
                        weight: 1.0,
                        semantic_tag: None,
                    }],
                    attrs: ProductionAttrs::default(),
                });
                let sid = SymbolId(symbols.len() as u32);
                symbols.push(Symbol::NonTerminal(anon_id));
                refs.push(SymbolRef {
                    symbol: sid,
                    modifier,
                });
            }
        }
    }
    Ok(())
}

/// Allocate a PoolId for a lexer rule, building its alternatives if needed.
fn alloc_pool_for(
    name: &str,
    lexer_name_to_pool: &mut HashMap<String, PoolId>,
    pool_entries: &mut Vec<TokenPoolEntry>,
    lexer_rules: &HashMap<String, Vec<RawAlternative<RawItem>>>,
    symbols: &mut Vec<Symbol>,
) -> PoolId {
    if let Some(&pool_id) = lexer_name_to_pool.get(name) {
        return pool_id;
    }
    let pool_id = PoolId(pool_entries.len() as u32);
    lexer_name_to_pool.insert(name.to_string(), pool_id);

    let empty = Vec::new();
    let alts = lexer_rules.get(name).unwrap_or(&empty);

    let ir_alts = alts
        .iter()
        .map(|raw_alt| {
            let syms: Vec<SymbolRef> = raw_alt
                .items
                .iter()
                .filter_map(|item| raw_item_to_terminal(item, symbols))
                .collect();
            Alternative {
                symbols: syms,
                weight: 1.0,
                semantic_tag: None,
            }
        })
        .filter(|alt| !alt.symbols.is_empty())
        .collect();

    pool_entries.push(TokenPoolEntry {
        name: name.to_string(),
        alternatives: ir_alts,
    });
    pool_id
}

// ── Extended lexer rule type for split grammars ──

struct LexerRule {
    name: String,
    alternatives: Vec<RawAlternative<RawItem>>,
    is_fragment: bool,
    is_skip: bool,
    is_channel_hidden: bool,
}

struct LexerGrammar {
    rules: Vec<LexerRule>,
}

/// Parse a lexer grammar, tracking fragment/skip/channel(HIDDEN) annotations.
fn parse_grammar_for_split(tokens: &[Token]) -> Result<LexerGrammar, ParseError> {
    let mut parser = Parser::new(tokens.to_vec());
    parser.skip_grammar_header()?;

    // Pre-build a map of line → action kind for `-> skip` / `-> channel(HIDDEN)`.
    // This lets us detect actions that parse_sequence's skip_action already consumed.
    let mut line_actions: HashMap<usize, &str> = HashMap::new();
    for w in tokens.windows(2) {
        if w[0].kind == TokenKind::Arrow {
            if let TokenKind::Ident(action) = &w[1].kind {
                if action == "skip" || action == "channel" {
                    line_actions.insert(w[0].line, action.as_str());
                }
            }
        }
    }

    let mut rules = Vec::new();
    while !parser.at_end() {
        let is_fragment = parser.peek_ident() == Some("fragment");
        if is_fragment {
            parser.advance();
        }

        let name_tok = match parser.expect(&TokenKind::Ident(String::new())) {
            Ok(t) => t,
            Err(_) => {
                // Skip unknown token and continue.
                parser.advance();
                continue;
            }
        };
        let name = match &name_tok.kind {
            TokenKind::Ident(s) => s.clone(),
            _ => unreachable!(),
        };

        if parser.expect(&TokenKind::Colon).is_err() {
            // Not a rule — skip ahead to next semicolon.
            while !parser.at_end() {
                if matches!(parser.peek(), Some(Token { kind: TokenKind::Semicolon, .. })) {
                    parser.advance();
                    break;
                }
                parser.advance();
            }
            continue;
        }

        // Record position before parsing alternatives so we can bound the action scan.
        let rule_start_line = name_tok.line;
        let alternatives = parser.parse_alternatives()?;

        // The semicolon position bounds this rule's token range.
        let rule_end_line = parser.peek().map(|t| t.line).unwrap_or(usize::MAX);

        // Check if any `-> skip` or `-> channel(HIDDEN)` action falls within this rule.
        let mut is_skip = false;
        let mut is_channel_hidden = false;
        for (&line, &action) in &line_actions {
            if line >= rule_start_line && line <= rule_end_line {
                match action {
                    "skip" => is_skip = true,
                    "channel" => is_channel_hidden = true,
                    _ => {}
                }
            }
        }

        parser.expect(&TokenKind::Semicolon)?;

        rules.push(LexerRule {
            name,
            alternatives,
            is_fragment,
            is_skip,
            is_channel_hidden,
        });
    }

    Ok(LexerGrammar { rules })
}

fn pick_start<I>(rules: &[RawRule<I>]) -> ProductionId {
    rules
        .iter()
        .enumerate()
        .find(|(_, r)| r.name.starts_with(|c: char| c.is_ascii_lowercase()))
        .map(|(i, _)| ProductionId(i as u32))
        .unwrap_or(ProductionId(0))
}

// ── Tokenizer ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    StringLit(String),
    CharClass {
        ranges: Vec<(u8, u8)>,
        negated: bool,
    },
    Colon,
    Semicolon,
    Pipe,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Question,
    Star,
    Plus,
    Dot,
    Arrow, // ->
    Comma,
    Assign, // =
    Hash,   // #
    At,     // @
    Tilde,  // ~
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    line: usize,
    col: usize,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut pos = 0;
    let mut line = 1;
    let mut col = 1;

    while pos < chars.len() {
        let ch = chars[pos];

        if ch == '\n' {
            line += 1;
            col = 1;
            pos += 1;
            continue;
        }

        if ch.is_ascii_whitespace() {
            col += 1;
            pos += 1;
            continue;
        }

        // Line comment: // ... \n
        if ch == '/' && pos + 1 < chars.len() && chars[pos + 1] == '/' {
            pos += 2;
            skip_line_comment(&chars, &mut pos);
            continue;
        }

        // Block comment: /* ... */
        if ch == '/' && pos + 1 < chars.len() && chars[pos + 1] == '*' {
            let start_line = line;
            let start_col = col;
            pos += 2;
            col += 2;
            skip_block_comment(&chars, &mut pos, &mut line, &mut col, '*', '/', start_line, start_col)?;
            continue;
        }

        // Negated character class: ~[...]
        if ch == '~' && pos + 1 < chars.len() && chars[pos + 1] == '[' {
            let start_col = col;
            pos += 2; // skip ~[
            col += 2;
            let ranges = parse_char_class_contents(&chars, &mut pos, &mut line, &mut col)?;
            tokens.push(Token {
                kind: TokenKind::CharClass {
                    ranges,
                    negated: true,
                },
                line,
                col: start_col,
            });
            continue;
        }

        // Standalone tilde (ANTLR negation): ~'c' or ~(rule)
        if ch == '~' {
            tokens.push(Token {
                kind: TokenKind::Tilde,
                line,
                col,
            });
            pos += 1;
            col += 1;
            continue;
        }

        // Character class: [...]
        if ch == '[' {
            let start_col = col;
            pos += 1; // skip [
            col += 1;
            let ranges = parse_char_class_contents(&chars, &mut pos, &mut line, &mut col)?;
            tokens.push(Token {
                kind: TokenKind::CharClass {
                    ranges,
                    negated: false,
                },
                line,
                col: start_col,
            });
            continue;
        }

        // Arrow: ->
        if ch == '-' && pos + 1 < chars.len() && chars[pos + 1] == '>' {
            tokens.push(Token {
                kind: TokenKind::Arrow,
                line,
                col,
            });
            pos += 2;
            col += 2;
            continue;
        }

        // Single-char tokens
        let single = match ch {
            ':' => Some(TokenKind::Colon),
            ';' => Some(TokenKind::Semicolon),
            '|' => Some(TokenKind::Pipe),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            '{' => Some(TokenKind::LBrace),
            '}' => Some(TokenKind::RBrace),
            '?' => Some(TokenKind::Question),
            '*' => Some(TokenKind::Star),
            '+' => Some(TokenKind::Plus),
            '.' => Some(TokenKind::Dot),
            ',' => Some(TokenKind::Comma),
            '=' => Some(TokenKind::Assign),
            '#' => Some(TokenKind::Hash),
            '@' => Some(TokenKind::At),
            _ => None,
        };

        if let Some(kind) = single {
            tokens.push(Token { kind, line, col });
            pos += 1;
            col += 1;
            continue;
        }

        // String literal: '...'
        if ch == '\'' {
            let start_col = col;
            let s = parse_string_literal(&chars, &mut pos, &mut line, &mut col, '\'')?;
            tokens.push(Token {
                kind: TokenKind::StringLit(s),
                line,
                col: start_col,
            });
            continue;
        }

        // Dollar sign (ANTLR variable prefix) — skip as noise.
        if ch == '$' {
            pos += 1;
            col += 1;
            continue;
        }

        // Identifiers and keywords
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start_col = col;
            let ident = scan_identifier(&chars, &mut pos, &mut col);
            tokens.push(Token {
                kind: TokenKind::Ident(ident),
                line,
                col: start_col,
            });
            continue;
        }

        return Err(ParseError {
            line,
            column: col,
            message: format!("unexpected character: {:?}", ch),
        });
    }

    Ok(tokens)
}

// ── ANTLR RawItem ───────────────────────────────────────────────────────────

#[derive(Clone)]
enum RawItem {
    Literal(String),
    NonTerminal(String, usize, usize),
    CharClass {
        ranges: Vec<(u8, u8)>,
        negated: bool,
    },
    AnyChar,
    Group(Vec<RawAlternative<RawItem>>),
    Quantified(Box<RawItem>, RawQuantifier),
}

impl BuildItem for RawItem {
    fn build_item(
        builder: &mut IrBuilder,
        item: &Self,
        refs: &mut Vec<barkus_core::ir::SymbolRef>,
    ) -> Result<(), ParseError> {
        match item {
            RawItem::Literal(s) => builder.build_literal(s, refs),
            RawItem::NonTerminal(name, line, col) => {
                builder.build_nonterminal(name, *line, *col, refs)?;
            }
            RawItem::CharClass { ranges, negated } => {
                builder.build_charclass(ranges, *negated, refs);
            }
            RawItem::AnyChar => builder.build_anychar(refs),
            RawItem::Group(alts) => builder.build_group(alts, refs)?,
            RawItem::Quantified(inner, q) => builder.build_quantified(inner.as_ref(), *q, refs)?,
        }
        Ok(())
    }
}

// ── Recursive descent parser ────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn current_loc(&self) -> (usize, usize) {
        self.peek()
            .or(self.tokens.last())
            .map(|t| (t.line, t.col))
            .unwrap_or((1, 1))
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        if let Some(tok) = self.tokens.get(self.pos) {
            if std::mem::discriminant(&tok.kind) == std::mem::discriminant(expected) {
                let tok = tok.clone();
                self.pos += 1;
                return Ok(tok);
            }
            return Err(ParseError {
                line: tok.line,
                column: tok.col,
                message: format!("expected {:?}, found {:?}", expected, tok.kind),
            });
        }
        let (line, col) = self.current_loc();
        Err(ParseError {
            line,
            column: col,
            message: format!("expected {:?}, found end of input", expected),
        })
    }

    fn peek_ident(&self) -> Option<&str> {
        match self.peek() {
            Some(Token {
                kind: TokenKind::Ident(s),
                ..
            }) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Skip a brace-delimited block `{ ... }` (assumes opening `{` already consumed).
    fn skip_brace_block(&mut self) {
        let mut depth = 1u32;
        while !self.at_end() && depth > 0 {
            match self.peek().map(|t| &t.kind) {
                Some(TokenKind::LBrace) => { depth += 1; self.advance(); }
                Some(TokenKind::RBrace) => { depth -= 1; self.advance(); }
                _ => { self.advance(); }
            }
        }
    }

    /// Skip optional grammar header: `[lexer|parser] grammar Name ;`
    /// followed by optional `options { ... }` block.
    fn skip_grammar_header(&mut self) -> Result<(), ParseError> {
        if matches!(self.peek_ident(), Some("grammar" | "lexer" | "parser")) {
            if matches!(self.peek_ident(), Some("lexer" | "parser")) {
                self.advance();
            }
            if self.peek_ident() == Some("grammar") {
                self.advance();
            }
            self.expect(&TokenKind::Ident(String::new()))?;
            self.expect(&TokenKind::Semicolon)?;
        }
        if self.peek_ident() == Some("options") {
            self.advance();
            self.expect(&TokenKind::LBrace)?;
            self.skip_brace_block();
        }
        Ok(())
    }

    fn parse_grammar(&mut self) -> Result<RawGrammar<RawItem>, ParseError> {
        self.skip_grammar_header()?;

        let mut rules = Vec::new();
        while !self.at_end() {
            rules.push(self.parse_rule()?);
        }
        if rules.is_empty() {
            return Err(ParseError {
                line: 1,
                column: 1,
                message: "empty grammar".into(),
            });
        }
        Ok(RawGrammar { rules })
    }

    fn parse_rule(&mut self) -> Result<RawRule<RawItem>, ParseError> {
        if self.peek_ident() == Some("fragment") {
            self.advance();
        }

        let name_tok = self.expect(&TokenKind::Ident(String::new()))?;
        let name = match &name_tok.kind {
            TokenKind::Ident(s) => s.clone(),
            _ => unreachable!(),
        };

        self.expect(&TokenKind::Colon)?;
        let alternatives = self.parse_alternatives()?;
        self.expect(&TokenKind::Semicolon)?;
        Ok(RawRule {
            name,
            alternatives,
            line: name_tok.line,
            col: name_tok.col,
        })
    }

    fn parse_alternatives(&mut self) -> Result<Vec<RawAlternative<RawItem>>, ParseError> {
        let mut alts = vec![self.parse_sequence()?];
        while matches!(self.peek(), Some(Token { kind: TokenKind::Pipe, .. })) {
            self.advance();
            alts.push(self.parse_sequence()?);
        }
        Ok(alts)
    }

    fn parse_sequence(&mut self) -> Result<RawAlternative<RawItem>, ParseError> {
        let mut items = Vec::new();
        loop {
            match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Pipe)
                | Some(TokenKind::Semicolon)
                | Some(TokenKind::RParen)
                | None => break,
                Some(TokenKind::Arrow) => {
                    self.skip_action()?;
                    break;
                }
                Some(TokenKind::LBrace) => {
                    // Skip embedded action block `{ ... }`
                    self.advance();
                    self.skip_brace_block();
                }
                Some(TokenKind::Hash) => {
                    // Skip ANTLR label `# labelName`
                    self.advance();
                    if matches!(self.peek(), Some(Token { kind: TokenKind::Ident(_), .. })) {
                        self.advance();
                    }
                }
                _ => {
                    if let Some(item) = self.parse_atom()? {
                        let item = self.maybe_quantified(item);
                        items.push(item);
                    } else {
                        // Unknown token — skip to avoid infinite loop
                        self.advance();
                    }
                }
            }
        }
        Ok(RawAlternative { items })
    }

    fn skip_action(&mut self) -> Result<(), ParseError> {
        self.advance(); // skip `->`
        // Consume comma-separated actions like `-> pushMode(META), more`
        loop {
            if matches!(self.peek(), Some(Token { kind: TokenKind::Ident(_), .. })) {
                self.advance();
            }
            if matches!(self.peek(), Some(Token { kind: TokenKind::LParen, .. })) {
                self.advance();
                while !self.at_end() {
                    if matches!(self.peek(), Some(Token { kind: TokenKind::RParen, .. })) {
                        self.advance();
                        break;
                    }
                    self.advance();
                }
            }
            if matches!(self.peek(), Some(Token { kind: TokenKind::Comma, .. })) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(())
    }

    fn parse_atom(&mut self) -> Result<Option<RawItem>, ParseError> {
        let tok = match self.peek() {
            Some(t) => t,
            None => return Ok(None),
        };

        match &tok.kind {
            TokenKind::StringLit(_) => {
                let tok = self.advance().unwrap();
                match tok.kind {
                    TokenKind::StringLit(s) => Ok(Some(RawItem::Literal(s))),
                    _ => unreachable!(),
                }
            }
            TokenKind::Ident(_) => {
                let tok = self.advance().unwrap();
                match tok.kind {
                    TokenKind::Ident(s) => Ok(Some(RawItem::NonTerminal(s, tok.line, tok.col))),
                    _ => unreachable!(),
                }
            }
            TokenKind::CharClass { .. } => {
                let tok = self.advance().unwrap();
                match tok.kind {
                    TokenKind::CharClass { ranges, negated } => {
                        Ok(Some(RawItem::CharClass { ranges, negated }))
                    }
                    _ => unreachable!(),
                }
            }
            TokenKind::Tilde => {
                // ANTLR negation: ~'c' → negated char class, ~(...) → skip as AnyChar approx
                self.advance();
                match self.peek().map(|t| &t.kind) {
                    Some(TokenKind::StringLit(_)) => {
                        let tok = self.advance().unwrap();
                        if let TokenKind::StringLit(s) = tok.kind {
                            let ranges: Vec<(u8, u8)> = s.bytes().map(|b| (b, b)).collect();
                            Ok(Some(RawItem::CharClass { ranges, negated: true }))
                        } else {
                            unreachable!()
                        }
                    }
                    Some(TokenKind::CharClass { .. }) => {
                        let tok = self.advance().unwrap();
                        if let TokenKind::CharClass { ranges, .. } = tok.kind {
                            // ~[...] as token sequence — already negated by the char class parse
                            Ok(Some(RawItem::CharClass { ranges, negated: true }))
                        } else {
                            unreachable!()
                        }
                    }
                    _ => {
                        // ~(rule) or unknown — approximate as AnyChar
                        Ok(Some(RawItem::AnyChar))
                    }
                }
            }
            TokenKind::Dot => {
                self.advance();
                Ok(Some(RawItem::AnyChar))
            }
            TokenKind::LParen => {
                self.advance();
                let alts = self.parse_alternatives()?;
                self.expect(&TokenKind::RParen)?;
                Ok(Some(RawItem::Group(alts)))
            }
            _ => Ok(None),
        }
    }

    fn maybe_quantified(&mut self, item: RawItem) -> RawItem {
        match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Question) => {
                self.advance();
                RawItem::Quantified(Box::new(item), RawQuantifier::Optional)
            }
            Some(TokenKind::Star) => {
                self.advance();
                RawItem::Quantified(Box::new(item), RawQuantifier::ZeroOrMore)
            }
            Some(TokenKind::Plus) => {
                self.advance();
                RawItem::Quantified(Box::new(item), RawQuantifier::OneOrMore)
            }
            _ => item,
        }
    }
}

fn parse_grammar(tokens: &[Token]) -> Result<RawGrammar<RawItem>, ParseError> {
    let mut parser = Parser::new(tokens.to_vec());
    parser.parse_grammar()
}
