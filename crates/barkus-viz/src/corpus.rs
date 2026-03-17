use std::fs;
use std::path::Path;

/// Load all tape files from a directory.
///
/// Each file is either raw binary (libFuzzer corpus) or Go fuzz v1 format
/// (`go test fuzz v1` header followed by a `[]byte("...")` or `string("...")` line).
pub fn load_corpus_dir(dir: &Path) -> Result<Vec<Vec<u8>>, String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("cannot read corpus directory {}: {e}", dir.display()))?;

    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    paths.sort();

    let mut tapes = Vec::with_capacity(paths.len());
    for path in &paths {
        let tape = parse_tape_file(path)?;
        tapes.push(tape);
    }
    Ok(tapes)
}

/// Parse a single tape file, auto-detecting Go fuzz v1 vs raw binary.
fn parse_tape_file(path: &Path) -> Result<Vec<u8>, String> {
    let raw = fs::read(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    // Detect Go fuzz v1: starts with "go test fuzz v1\n"
    if raw.starts_with(b"go test fuzz v1\n") {
        return parse_go_fuzz_v1(&raw, path);
    }

    // Raw binary tape.
    Ok(raw)
}

/// Parse Go fuzz v1 format. Extract the first value line only.
fn parse_go_fuzz_v1(data: &[u8], path: &Path) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(data)
        .map_err(|e| format!("{}: invalid UTF-8 in Go fuzz file: {e}", path.display()))?;

    // Skip the header line, find the first value line.
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Accept []byte("...") or string("...")
        if let Some(inner) = strip_go_value(line, "[]byte(\"", "\")") {
            return parse_go_string_literal(inner, path);
        }
        if let Some(inner) = strip_go_value(line, "string(\"", "\")") {
            return parse_go_string_literal(inner, path);
        }
        return Err(format!(
            "{}: unsupported Go fuzz value line: {line}",
            path.display()
        ));
    }

    Err(format!("{}: Go fuzz file has no value line", path.display()))
}

fn strip_go_value<'a>(line: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?;
    let inner = rest.strip_suffix(suffix)?;
    Some(inner)
}

/// Parse the inner content of a Go string literal, handling escape sequences.
fn parse_go_string_literal(s: &str, path: &Path) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c != '\\' {
            // Encode raw character as UTF-8.
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        // Escape sequence.
        let esc = chars.next().ok_or_else(|| {
            format!("{}: trailing backslash in Go string", path.display())
        })?;
        match esc {
            'n' => out.push(b'\n'),
            't' => out.push(b'\t'),
            'r' => out.push(b'\r'),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            'a' => out.push(0x07),
            'b' => out.push(0x08),
            'f' => out.push(0x0C),
            'v' => out.push(0x0B),
            'x' => {
                let hi = hex_digit(chars.next(), path)?;
                let lo = hex_digit(chars.next(), path)?;
                out.push(hi << 4 | lo);
            }
            'u' => {
                let cp = parse_hex_n(&mut chars, 4, path)?;
                encode_codepoint(cp, &mut out, path)?;
            }
            'U' => {
                let cp = parse_hex_n(&mut chars, 8, path)?;
                encode_codepoint(cp, &mut out, path)?;
            }
            '0'..='7' => {
                // Octal: up to 3 digits (first already consumed).
                let mut val = esc as u8 - b'0';
                for _ in 0..2 {
                    match chars.clone().next() {
                        Some(d @ '0'..='7') => {
                            chars.next();
                            val = val * 8 + (d as u8 - b'0');
                        }
                        _ => break,
                    }
                }
                out.push(val);
            }
            _ => {
                return Err(format!(
                    "{}: unknown escape \\{esc} in Go string",
                    path.display()
                ));
            }
        }
    }
    Ok(out)
}

fn hex_digit(c: Option<char>, path: &Path) -> Result<u8, String> {
    let c = c.ok_or_else(|| format!("{}: truncated \\x escape", path.display()))?;
    match c {
        '0'..='9' => Ok(c as u8 - b'0'),
        'a'..='f' => Ok(c as u8 - b'a' + 10),
        'A'..='F' => Ok(c as u8 - b'A' + 10),
        _ => Err(format!("{}: invalid hex digit '{c}' in escape", path.display())),
    }
}

fn parse_hex_n(chars: &mut std::str::Chars, n: usize, path: &Path) -> Result<u32, String> {
    let mut val = 0u32;
    for _ in 0..n {
        let d = hex_digit(chars.next(), path)?;
        val = val * 16 + d as u32;
    }
    Ok(val)
}

fn encode_codepoint(cp: u32, out: &mut Vec<u8>, path: &Path) -> Result<(), String> {
    let c = char::from_u32(cp).ok_or_else(|| {
        format!("{}: invalid Unicode codepoint U+{cp:04X}", path.display())
    })?;
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, contents: &[u8]) {
        fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn raw_binary_corpus() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "tape1", &[0x0A, 0x3F, 0x01]);
        write_file(dir.path(), "tape2", &[0xFF]);

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes.len(), 2);
        assert_eq!(tapes[0], vec![0x0A, 0x3F, 0x01]);
        assert_eq!(tapes[1], vec![0xFF]);
    }

    #[test]
    fn go_fuzz_v1_bytes() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "corpus1",
            b"go test fuzz v1\n[]byte(\"\\x0a\\x3f\\x01\")\n",
        );

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes.len(), 1);
        assert_eq!(tapes[0], vec![0x0A, 0x3F, 0x01]);
    }

    #[test]
    fn go_fuzz_v1_string() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "corpus1",
            b"go test fuzz v1\nstring(\"hello\\nworld\")\n",
        );

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes.len(), 1);
        assert_eq!(tapes[0], b"hello\nworld");
    }

    #[test]
    fn go_fuzz_v1_escapes() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "corpus1",
            b"go test fuzz v1\n[]byte(\"\\t\\r\\\\\\\"\\x41\\n\")\n",
        );

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes[0], b"\t\r\\\"\x41\n");
    }

    #[test]
    fn go_fuzz_v1_octal() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "corpus1",
            b"go test fuzz v1\n[]byte(\"\\101\\012\")\n",
        );

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes[0], b"A\n");
    }

    #[test]
    fn go_fuzz_v1_first_value_only() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "corpus1",
            b"go test fuzz v1\n[]byte(\"\\x0a\")\nstring(\"ignored\")\n",
        );

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes.len(), 1);
        assert_eq!(tapes[0], vec![0x0A]);
    }

    #[test]
    fn skips_subdirectories() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        write_file(dir.path(), "tape1", &[0x01]);

        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert_eq!(tapes.len(), 1);
    }

    #[test]
    fn empty_dir() {
        let dir = TempDir::new().unwrap();
        let tapes = load_corpus_dir(dir.path()).unwrap();
        assert!(tapes.is_empty());
    }
}
