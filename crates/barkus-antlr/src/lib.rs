use barkus_core::ir::ProductionId;
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
    Question,
    Star,
    Plus,
    Dot,
    Arrow, // ->
    Comma,
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
            '?' => Some(TokenKind::Question),
            '*' => Some(TokenKind::Star),
            '+' => Some(TokenKind::Plus),
            '.' => Some(TokenKind::Dot),
            ',' => Some(TokenKind::Comma),
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

    fn parse_grammar(&mut self) -> Result<RawGrammar<RawItem>, ParseError> {
        // Optional: `grammar Name ;`
        if self.peek_ident() == Some("grammar") {
            self.advance();
            self.expect(&TokenKind::Ident(String::new()))?;
            self.expect(&TokenKind::Semicolon)?;
        }

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
                _ => {
                    if let Some(item) = self.parse_atom()? {
                        let item = self.maybe_quantified(item);
                        items.push(item);
                    }
                }
            }
        }
        Ok(RawAlternative { items })
    }

    fn skip_action(&mut self) -> Result<(), ParseError> {
        self.advance(); // skip `->`
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
