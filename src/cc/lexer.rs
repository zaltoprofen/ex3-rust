use super::{CcError, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Number(u32, bool),
    Symbol(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, Vec<CcError>> {
    let bytes = source.as_bytes();
    let (mut i, mut line, mut column) = (0, 1, 1);
    let mut out = Vec::new();
    let mut errors = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            if c == b'\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
            i += 1;
            continue;
        }
        if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
                column += 1;
            }
            continue;
        }
        if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = Span { line, column };
            i += 2;
            column += 2;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    i += 2;
                    column += 2;
                    closed = true;
                    break;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                i += 1;
            }
            if !closed {
                errors.push(CcError::new(start, "unterminated block comment"));
            }
            continue;
        }
        let span = Span { line, column };
        if c == b'_' || c.is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric()) {
                i += 1;
                column += 1;
            }
            out.push(Token {
                kind: TokenKind::Ident(source[start..i].to_string()),
                span,
            });
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            let hex = c == b'0' && matches!(bytes.get(i + 1), Some(b'x' | b'X'));
            if hex {
                i += 2;
                column += 2;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                    column += 1;
                }
            } else {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                    column += 1;
                }
            }
            let digits_end = i;
            let unsigned = matches!(bytes.get(i), Some(b'u' | b'U'));
            if unsigned {
                i += 1;
                column += 1;
            }
            let text = &source[start..digits_end];
            let parsed = if hex {
                u64::from_str_radix(&text[2..], 16)
            } else {
                text.parse::<u64>()
            };
            match parsed {
                Ok(v) if v <= u32::MAX as u64 => out.push(Token {
                    kind: TokenKind::Number(v as u32, unsigned || v > i32::MAX as u64),
                    span,
                }),
                _ => errors.push(CcError::new(
                    span,
                    format!("integer literal `{text}` is out of range"),
                )),
            }
            continue;
        }
        let two = bytes.get(i + 1).map(|&d| [c, d]);
        let op = match two {
            Some([b'=', b'=']) => Some("=="),
            Some([b'!', b'=']) => Some("!="),
            Some([b'<', b'=']) => Some("<="),
            Some([b'>', b'=']) => Some(">="),
            Some([b'&', b'&']) => Some("&&"),
            Some([b'|', b'|']) => Some("||"),
            _ => None,
        };
        if let Some(op) = op {
            out.push(Token {
                kind: TokenKind::Symbol(op),
                span,
            });
            i += 2;
            column += 2;
            continue;
        }
        let op = match c {
            b'(' => "(",
            b')' => ")",
            b'{' => "{",
            b'}' => "}",
            b';' => ";",
            b',' => ",",
            b':' => ":",
            b'=' => "=",
            b'+' => "+",
            b'-' => "-",
            b'*' => "*",
            b'/' => "/",
            b'%' => "%",
            b'~' => "~",
            b'!' => "!",
            b'&' => "&",
            b'|' => "|",
            b'^' => "^",
            b'<' => "<",
            b'>' => ">",
            _ => "",
        };
        if op.is_empty() {
            errors.push(CcError::new(
                span,
                format!("unexpected character `{}`", c as char),
            ));
        } else {
            out.push(Token {
                kind: TokenKind::Symbol(op),
                span,
            });
        }
        i += 1;
        column += 1;
    }
    out.push(Token {
        kind: TokenKind::Eof,
        span: Span { line, column },
    });
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}
