use barkus_core::ir::ProductionId;
use barkus_parser_common::{
    build_ir, parse_char_class_contents, parse_string_literal, scan_identifier, skip_line_comment,
    BuildItem, IrBuilder, RawAlternative, RawGrammar, RawQuantifier, RawRule,
};

// ── Public API ──────────────────────────────────────────────────────────────

pub use barkus_parser_common::ParseError;

pub fn compile(source: &str) -> Result<barkus_core::ir::GrammarIr, ParseError> {
    let tokens = tokenize(source)?;
    let raw = parse_grammar(&tokens)?;
    let ir = build_ir(raw, |_| ProductionId(0))?;
    Ok(ir)
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
    Arrow,     // <-
    Equals,    // =
    Slash,     // /
    Question,  // ?
    Star,      // *
    Plus,      // +
    Ampersand, // &
    Bang,      // !
    Dot,       // .
    LParen,
    RParen,
    Semicolon,
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

        // Line comment: # ...
        if ch == '#' {
            pos += 1;
            skip_line_comment(&chars, &mut pos);
            continue;
        }

        // Also support // line comments
        if ch == '/' && pos + 1 < chars.len() && chars[pos + 1] == '/' {
            pos += 2;
            skip_line_comment(&chars, &mut pos);
            continue;
        }

        // Arrow: <- or ←
        if ch == '<' && pos + 1 < chars.len() && chars[pos + 1] == '-' {
            tokens.push(Token {
                kind: TokenKind::Arrow,
                line,
                col,
            });
            pos += 2;
            col += 2;
            continue;
        }
        if ch == '\u{2190}' {
            tokens.push(Token {
                kind: TokenKind::Arrow,
                line,
                col,
            });
            pos += 1;
            col += 1;
            continue;
        }

        // Character class: [...]  or [^...]
        if ch == '[' {
            let start_col = col;
            pos += 1;
            col += 1;
            let negated = pos < chars.len() && chars[pos] == '^';
            if negated {
                pos += 1;
                col += 1;
            }
            let ranges = parse_char_class_contents(&chars, &mut pos, &mut line, &mut col)?;
            tokens.push(Token {
                kind: TokenKind::CharClass { ranges, negated },
                line,
                col: start_col,
            });
            continue;
        }

        // Single-char tokens
        let single = match ch {
            '=' => Some(TokenKind::Equals),
            '/' => Some(TokenKind::Slash),
            '?' => Some(TokenKind::Question),
            '*' => Some(TokenKind::Star),
            '+' => Some(TokenKind::Plus),
            '&' => Some(TokenKind::Ampersand),
            '!' => Some(TokenKind::Bang),
            '.' => Some(TokenKind::Dot),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            ';' => Some(TokenKind::Semicolon),
            _ => None,
        };

        if let Some(kind) = single {
            tokens.push(Token { kind, line, col });
            pos += 1;
            col += 1;
            continue;
        }

        // String literals: '...' or "..."
        if ch == '\'' || ch == '"' {
            let start_col = col;
            let s = parse_string_literal(&chars, &mut pos, &mut line, &mut col, ch)?;
            tokens.push(Token {
                kind: TokenKind::StringLit(s),
                line,
                col: start_col,
            });
            continue;
        }

        // Identifiers
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

// ── PEG RawItem ─────────────────────────────────────────────────────────────

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

    fn expect_arrow_or_equals(&mut self) -> Result<Token, ParseError> {
        if let Some(tok) = self.tokens.get(self.pos) {
            if matches!(tok.kind, TokenKind::Arrow | TokenKind::Equals) {
                let tok = tok.clone();
                self.pos += 1;
                return Ok(tok);
            }
            return Err(ParseError {
                line: tok.line,
                column: tok.col,
                message: format!("expected '<-' or '=', found {:?}", tok.kind),
            });
        }
        let (line, col) = self.current_loc();
        Err(ParseError {
            line,
            column: col,
            message: "expected '<-' or '=', found end of input".into(),
        })
    }

    fn at_rule_start(&self) -> bool {
        if let Some(Token {
            kind: TokenKind::Ident(_),
            ..
        }) = self.tokens.get(self.pos)
        {
            if let Some(tok) = self.tokens.get(self.pos + 1) {
                return matches!(tok.kind, TokenKind::Arrow | TokenKind::Equals);
            }
        }
        false
    }

    fn parse_grammar(&mut self) -> Result<RawGrammar<RawItem>, ParseError> {
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
        let name_tok = match self.peek() {
            Some(Token {
                kind: TokenKind::Ident(_),
                ..
            }) => self.advance().unwrap(),
            _ => {
                let (line, col) = self.current_loc();
                return Err(ParseError {
                    line,
                    column: col,
                    message: "expected rule name".into(),
                });
            }
        };
        let name = match &name_tok.kind {
            TokenKind::Ident(s) => s.clone(),
            _ => unreachable!(),
        };

        self.expect_arrow_or_equals()?;
        let alternatives = self.parse_alternatives()?;

        if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Semicolon,
                ..
            })
        ) {
            self.advance();
        }

        Ok(RawRule {
            name,
            alternatives,
            line: name_tok.line,
            col: name_tok.col,
        })
    }

    fn parse_alternatives(&mut self) -> Result<Vec<RawAlternative<RawItem>>, ParseError> {
        let mut alts = vec![self.parse_sequence()?];
        while matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Slash,
                ..
            })
        ) {
            self.advance();
            alts.push(self.parse_sequence()?);
        }
        Ok(alts)
    }

    fn parse_sequence(&mut self) -> Result<RawAlternative<RawItem>, ParseError> {
        let mut items = Vec::new();
        loop {
            match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Slash)
                | Some(TokenKind::Semicolon)
                | Some(TokenKind::RParen)
                | None => break,
                _ => {
                    if self.at_rule_start() {
                        break;
                    }
                    let before = self.pos;
                    match self.parse_atom()? {
                        Some(item) => {
                            let item = self.maybe_quantified(item);
                            items.push(item);
                        }
                        None if self.pos == before => break,
                        None => {} // lookahead consumed tokens but produced no item
                    }
                }
            }
        }
        Ok(RawAlternative { items })
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
                match self.peek() {
                    Some(Token {
                        kind: TokenKind::RParen,
                        ..
                    }) => {
                        self.advance();
                    }
                    _ => {
                        let (line, col) = self.current_loc();
                        return Err(ParseError {
                            line,
                            column: col,
                            message: "expected ')'".into(),
                        });
                    }
                }
                Ok(Some(RawItem::Group(alts)))
            }
            TokenKind::Ampersand | TokenKind::Bang => {
                self.advance();
                if let Some(item) = self.parse_atom()? {
                    let _discarded = self.maybe_quantified(item);
                }
                Ok(None)
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
