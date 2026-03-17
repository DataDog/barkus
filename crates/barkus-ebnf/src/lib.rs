use barkus_core::ir::{Modifier, ProductionId, SymbolRef};
use barkus_parser_common::{
    build_ir, parse_string_literal, scan_identifier, skip_block_comment, skip_line_comment,
    BuildItem, IrBuilder, RawAlternative, RawGrammar, RawRule,
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
    IntLit(u32),
    StringLit(String),
    Equals,
    Semicolon,
    Dot,
    Pipe,
    Slash,
    Comma,
    Star,
    Minus,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
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

        // Line comment: // ...
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
            skip_block_comment(
                &chars, &mut pos, &mut line, &mut col, '*', '/', start_line, start_col,
            )?;
            continue;
        }

        // EBNF block comment: (* ... *)
        if ch == '(' && pos + 1 < chars.len() && chars[pos + 1] == '*' {
            let start_line = line;
            let start_col = col;
            pos += 2;
            col += 2;
            skip_block_comment(
                &chars, &mut pos, &mut line, &mut col, '*', ')', start_line, start_col,
            )?;
            continue;
        }

        // Special sequence: ? ... ? — skip entirely
        if ch == '?' {
            let start_col = col;
            pos += 1;
            col += 1;
            loop {
                if pos >= chars.len() {
                    return Err(ParseError {
                        line,
                        column: start_col,
                        message: "unterminated special sequence".into(),
                    });
                }
                if chars[pos] == '?' {
                    pos += 1;
                    col += 1;
                    break;
                }
                if chars[pos] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                pos += 1;
            }
            continue;
        }

        // Single-char tokens
        let single = match ch {
            '=' => Some(TokenKind::Equals),
            ';' => Some(TokenKind::Semicolon),
            '.' => Some(TokenKind::Dot),
            '|' => Some(TokenKind::Pipe),
            '/' => Some(TokenKind::Slash),
            ',' => Some(TokenKind::Comma),
            '*' => Some(TokenKind::Star),
            '-' => Some(TokenKind::Minus),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            '[' => Some(TokenKind::LBracket),
            ']' => Some(TokenKind::RBracket),
            '{' => Some(TokenKind::LBrace),
            '}' => Some(TokenKind::RBrace),
            _ => None,
        };

        if let Some(kind) = single {
            tokens.push(Token { kind, line, col });
            pos += 1;
            col += 1;
            continue;
        }

        // Double-quoted string literal
        if ch == '"' {
            let start_col = col;
            let s = parse_string_literal(&chars, &mut pos, &mut line, &mut col, '"')?;
            tokens.push(Token {
                kind: TokenKind::StringLit(s),
                line,
                col: start_col,
            });
            continue;
        }

        // Single-quoted string literal
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

        // Integer literals (for repetition factor)
        if ch.is_ascii_digit() {
            let start_col = col;
            let mut num = 0u32;
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                num = num.saturating_mul(10).saturating_add(chars[pos] as u32 - '0' as u32);
                pos += 1;
                col += 1;
            }
            tokens.push(Token {
                kind: TokenKind::IntLit(num),
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

// ── EBNF RawItem ────────────────────────────────────────────────────────────

enum RawItem {
    Literal(String),
    NonTerminal(String, usize, usize),
    Optional(Vec<RawAlternative<RawItem>>),
    Repeat(Vec<RawAlternative<RawItem>>),
    Group(Vec<RawAlternative<RawItem>>),
    Factor(u32, Box<RawItem>),
}

impl BuildItem for RawItem {
    fn build_item(
        builder: &mut IrBuilder,
        item: &Self,
        refs: &mut Vec<SymbolRef>,
    ) -> Result<(), ParseError> {
        match item {
            RawItem::Literal(s) => builder.build_literal(s, refs),
            RawItem::NonTerminal(name, line, col) => {
                builder.build_nonterminal(name, *line, *col, refs)?;
            }
            RawItem::Optional(alts) => {
                let inner_sid = builder.build_alts_symbol(alts)?;
                refs.push(SymbolRef {
                    symbol: inner_sid,
                    modifier: Modifier::Optional,
                });
            }
            RawItem::Repeat(alts) => {
                let inner_sid = builder.build_alts_symbol(alts)?;
                refs.push(SymbolRef {
                    symbol: inner_sid,
                    modifier: Modifier::ZeroOrMore {
                        min: 0,
                        max: u32::MAX,
                    },
                });
            }
            RawItem::Group(alts) => builder.build_group(alts, refs)?,
            RawItem::Factor(n, inner) => {
                // Emit as bounded repetition to avoid pathological IR
                // expansion from nested factors (e.g. 8 * 4 * 8 * ...).
                let count = *n;
                let inner_sid = builder.build_single_symbol::<Self>(inner)?;
                refs.push(SymbolRef {
                    symbol: inner_sid,
                    modifier: Modifier::ZeroOrMore {
                        min: count,
                        max: count,
                    },
                });
            }
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

        match self.peek() {
            Some(Token {
                kind: TokenKind::Equals,
                ..
            }) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.current_loc();
                return Err(ParseError {
                    line,
                    column: col,
                    message: "expected '='".into(),
                });
            }
        }

        let alternatives = self.parse_alternatives()?;

        match self.peek() {
            Some(Token {
                kind: TokenKind::Semicolon | TokenKind::Dot,
                ..
            }) => {
                self.advance();
            }
            _ => {
                let (line, col) = self.current_loc();
                return Err(ParseError {
                    line,
                    column: col,
                    message: "expected ';' or '.' to end rule".into(),
                });
            }
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
                kind: TokenKind::Pipe | TokenKind::Slash,
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
                Some(TokenKind::Pipe)
                | Some(TokenKind::Slash)
                | Some(TokenKind::Semicolon)
                | Some(TokenKind::Dot)
                | Some(TokenKind::RParen)
                | Some(TokenKind::RBracket)
                | Some(TokenKind::RBrace)
                | None => break,
                Some(TokenKind::Comma) => {
                    self.advance();
                    continue;
                }
                _ => {
                    let item = self.parse_item()?;
                    if matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Minus,
                            ..
                        })
                    ) {
                        self.advance();
                        let _discarded = self.parse_item()?;
                    }
                    items.push(item);
                }
            }
        }
        Ok(RawAlternative { items })
    }

    fn parse_item(&mut self) -> Result<RawItem, ParseError> {
        if let Some(Token {
            kind: TokenKind::IntLit(n),
            ..
        }) = self.peek()
        {
            let n = *n;
            if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Star) {
                self.advance();
                self.advance();
                let inner = self.parse_item()?;
                return Ok(RawItem::Factor(n, Box::new(inner)));
            }
        }

        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<RawItem, ParseError> {
        let tok = match self.peek() {
            Some(t) => t,
            None => {
                let (line, col) = self.current_loc();
                return Err(ParseError {
                    line,
                    column: col,
                    message: "unexpected end of input".into(),
                });
            }
        };

        match &tok.kind {
            TokenKind::StringLit(_) => {
                let tok = self.advance().unwrap();
                match tok.kind {
                    TokenKind::StringLit(s) => Ok(RawItem::Literal(s)),
                    _ => unreachable!(),
                }
            }
            TokenKind::Ident(_) => {
                let tok = self.advance().unwrap();
                match tok.kind {
                    TokenKind::Ident(s) => Ok(RawItem::NonTerminal(s, tok.line, tok.col)),
                    _ => unreachable!(),
                }
            }
            TokenKind::LBracket => {
                self.advance();
                let alts = self.parse_alternatives()?;
                match self.peek() {
                    Some(Token {
                        kind: TokenKind::RBracket,
                        ..
                    }) => {
                        self.advance();
                    }
                    _ => {
                        let (line, col) = self.current_loc();
                        return Err(ParseError {
                            line,
                            column: col,
                            message: "expected ']'".into(),
                        });
                    }
                }
                Ok(RawItem::Optional(alts))
            }
            TokenKind::LBrace => {
                self.advance();
                let alts = self.parse_alternatives()?;
                match self.peek() {
                    Some(Token {
                        kind: TokenKind::RBrace,
                        ..
                    }) => {
                        self.advance();
                    }
                    _ => {
                        let (line, col) = self.current_loc();
                        return Err(ParseError {
                            line,
                            column: col,
                            message: "expected '}'".into(),
                        });
                    }
                }
                Ok(RawItem::Repeat(alts))
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
                Ok(RawItem::Group(alts))
            }
            _ => {
                let (line, col) = self.current_loc();
                Err(ParseError {
                    line,
                    column: col,
                    message: format!("unexpected token: {:?}", tok.kind),
                })
            }
        }
    }
}

fn parse_grammar(tokens: &[Token]) -> Result<RawGrammar<RawItem>, ParseError> {
    let mut parser = Parser::new(tokens.to_vec());
    parser.parse_grammar()
}
