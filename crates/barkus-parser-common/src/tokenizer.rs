use crate::ParseError;

/// Parse a string literal starting at the opening quote character.
/// `pos` must point at the opening quote. Advances `pos` past the closing quote.
pub fn parse_string_literal(
    chars: &[char],
    pos: &mut usize,
    line: &mut usize,
    col: &mut usize,
    quote_char: char,
) -> Result<String, ParseError> {
    let start_col = *col;
    *pos += 1; // skip opening quote
    *col += 1;
    let mut s = String::new();
    loop {
        if *pos >= chars.len() {
            return Err(ParseError {
                line: *line,
                column: start_col,
                message: "unterminated string literal".into(),
            });
        }
        let c = chars[*pos];
        if c == quote_char {
            *pos += 1;
            *col += 1;
            break;
        }
        if c == '\\' && *pos + 1 < chars.len() {
            *pos += 1;
            *col += 1;
            let escaped = chars[*pos];
            match escaped {
                'n' => s.push('\n'),
                'r' => s.push('\r'),
                't' => s.push('\t'),
                '\\' => s.push('\\'),
                c if c == quote_char => s.push(c),
                _ => {
                    s.push('\\');
                    s.push(escaped);
                }
            }
            *pos += 1;
            *col += 1;
            continue;
        }
        if c == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
        s.push(c);
        *pos += 1;
    }
    Ok(s)
}

/// Parse the contents of a character class (after the opening `[` and optional negation)
/// up to and including the closing `]`.
pub fn parse_char_class_contents(
    chars: &[char],
    pos: &mut usize,
    line: &mut usize,
    col: &mut usize,
) -> Result<Vec<(u8, u8)>, ParseError> {
    let mut ranges = Vec::new();

    while *pos < chars.len() && chars[*pos] != ']' {
        let c = read_char_class_char(chars, pos, line, col)?;
        if *pos + 1 < chars.len() && chars[*pos] == '-' && chars[*pos + 1] != ']' {
            *pos += 1; // skip -
            *col += 1;
            let end = read_char_class_char(chars, pos, line, col)?;
            ranges.push((c, end));
        } else {
            ranges.push((c, c));
        }
    }

    if *pos >= chars.len() {
        return Err(ParseError {
            line: *line,
            column: *col,
            message: "unterminated character class".into(),
        });
    }

    *pos += 1; // skip ]
    *col += 1;

    Ok(ranges)
}

/// Read a single character from inside a character class, handling escapes.
pub fn read_char_class_char(
    chars: &[char],
    pos: &mut usize,
    line: &mut usize,
    col: &mut usize,
) -> Result<u8, ParseError> {
    if *pos >= chars.len() {
        return Err(ParseError {
            line: *line,
            column: *col,
            message: "unexpected end of character class".into(),
        });
    }

    let ch = chars[*pos];
    if ch == '\\' {
        *pos += 1;
        *col += 1;
        if *pos >= chars.len() {
            return Err(ParseError {
                line: *line,
                column: *col,
                message: "unexpected end of escape in character class".into(),
            });
        }
        let escaped = chars[*pos];
        *pos += 1;
        *col += 1;
        let byte = match escaped {
            'n' => b'\n',
            'r' => b'\r',
            't' => b'\t',
            '\\' => b'\\',
            ']' => b']',
            '[' => b'[',
            '-' => b'-',
            _ => escaped as u8,
        };
        Ok(byte)
    } else {
        *pos += 1;
        *col += 1;
        Ok(ch as u8)
    }
}

/// Scan an identifier starting at `pos`. Assumes `chars[pos]` is already known to be
/// alphabetic or `_`. Advances `pos` and `col` past the identifier.
pub fn scan_identifier(chars: &[char], pos: &mut usize, col: &mut usize) -> String {
    let mut ident = String::new();
    while *pos < chars.len() && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_') {
        ident.push(chars[*pos]);
        *pos += 1;
        *col += 1;
    }
    ident
}

/// Skip a line comment. `pos` should point just past the comment-start sequence
/// (e.g., past `//` or `#`). Advances `pos` to the `\n` (not past it).
pub fn skip_line_comment(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos] != '\n' {
        *pos += 1;
    }
}

/// Skip a block comment. `pos` should already be past the opening sequence (e.g., `/*`).
/// `close_seq` defines the closing sequence (e.g., `('*', '/')`).
/// `start` is `(line, col)` of the opening sequence for error reporting.
#[allow(clippy::too_many_arguments)]
pub fn skip_block_comment(
    chars: &[char],
    pos: &mut usize,
    line: &mut usize,
    col: &mut usize,
    close_first: char,
    close_second: char,
    start_line: usize,
    start_col: usize,
) -> Result<(), ParseError> {
    loop {
        if *pos >= chars.len() {
            return Err(ParseError {
                line: start_line,
                column: start_col,
                message: "unterminated block comment".into(),
            });
        }
        if chars[*pos] == close_first
            && *pos + 1 < chars.len()
            && chars[*pos + 1] == close_second
        {
            *pos += 2;
            *col += 2;
            return Ok(());
        }
        if chars[*pos] == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
        *pos += 1;
    }
}
