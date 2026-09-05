use super::{
    ast::{IntegerLiteral, IntegerLiteralKind},
    CcError, Span,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Int,
    Int32,
    UInt32,
    Unsigned,
    Void,
    If,
    Else,
    While,
    Switch,
    Case,
    Default,
    Break,
    Continue,
    Goto,
    Return,
}

impl Keyword {
    fn from_identifier(identifier: &str) -> Option<Self> {
        Some(match identifier {
            "int" => Self::Int,
            "int32_t" => Self::Int32,
            "uint32_t" => Self::UInt32,
            "unsigned" => Self::Unsigned,
            "void" => Self::Void,
            "if" => Self::If,
            "else" => Self::Else,
            "while" => Self::While,
            "switch" => Self::Switch,
            "case" => Self::Case,
            "default" => Self::Default,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "goto" => Self::Goto,
            "return" => Self::Return,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Semicolon,
    Comma,
    Colon,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Tilde,
    Bang,
    Ampersand,
    Pipe,
    Caret,
    Less,
    Greater,
    EqualEqual,
    BangEqual,
    LessEqual,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
}

impl Symbol {
    pub fn text(self) -> &'static str {
        match self {
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::Semicolon => ";",
            Self::Comma => ",",
            Self::Colon => ":",
            Self::Assign => "=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::Tilde => "~",
            Self::Bang => "!",
            Self::Ampersand => "&",
            Self::Pipe => "|",
            Self::Caret => "^",
            Self::Less => "<",
            Self::Greater => ">",
            Self::EqualEqual => "==",
            Self::BangEqual => "!=",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
            Self::LogicalAnd => "&&",
            Self::LogicalOr => "||",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
    Integer(IntegerLiteral),
    Keyword(Keyword),
    Symbol(Symbol),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

struct Cursor<'src> {
    source: &'src str,
    offset: usize,
    line: usize,
    column: usize,
}

impl<'src> Cursor<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
        }
    }
    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }
    fn peek_next(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset + 1).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        if byte == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(byte)
    }
    fn span(&self) -> Span {
        Span {
            line: self.line,
            column: self.column,
        }
    }
    fn slice_from(&self, start: usize) -> &'src str {
        &self.source[start..self.offset]
    }
}

pub fn lex(source: &str) -> Result<Vec<Token>, Vec<CcError>> {
    let mut cursor = Cursor::new(source);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    while let Some(byte) = cursor.peek() {
        if byte.is_ascii_whitespace() {
            cursor.bump();
            continue;
        }
        if byte == b'/' && cursor.peek_next() == Some(b'/') {
            while cursor.peek().is_some_and(|byte| byte != b'\n') {
                cursor.bump();
            }
            continue;
        }
        if byte == b'/' && cursor.peek_next() == Some(b'*') {
            let span = cursor.span();
            cursor.bump();
            cursor.bump();
            let mut closed = false;
            while let Some(byte) = cursor.peek() {
                if byte == b'*' && cursor.peek_next() == Some(b'/') {
                    cursor.bump();
                    cursor.bump();
                    closed = true;
                    break;
                }
                cursor.bump();
            }
            if !closed {
                errors.push(CcError::new(span, "unterminated block comment"));
            }
            continue;
        }
        let span = cursor.span();
        if byte == b'_' || byte.is_ascii_alphabetic() {
            let start = cursor.offset;
            while cursor
                .peek()
                .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
            {
                cursor.bump();
            }
            let identifier = cursor.slice_from(start);
            let kind = Keyword::from_identifier(identifier)
                .map(TokenKind::Keyword)
                .unwrap_or_else(|| TokenKind::Identifier(identifier.to_owned()));
            tokens.push(Token { kind, span });
            continue;
        }
        if byte.is_ascii_digit() {
            lex_integer(&mut cursor, span, &mut tokens, &mut errors);
            continue;
        }
        if let Some(symbol) = lex_symbol(&mut cursor) {
            tokens.push(Token {
                kind: TokenKind::Symbol(symbol),
                span,
            });
        } else {
            errors.push(CcError::new(
                span,
                format!("unexpected character `{}`", byte as char),
            ));
            cursor.bump();
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: cursor.span(),
    });
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

fn lex_integer(
    cursor: &mut Cursor<'_>,
    span: Span,
    tokens: &mut Vec<Token>,
    errors: &mut Vec<CcError>,
) {
    let start = cursor.offset;
    let hexadecimal =
        cursor.peek() == Some(b'0') && matches!(cursor.peek_next(), Some(b'x' | b'X'));
    if hexadecimal {
        cursor.bump();
        cursor.bump();
        while cursor.peek().is_some_and(|byte| byte.is_ascii_hexdigit()) {
            cursor.bump();
        }
    } else {
        while cursor.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            cursor.bump();
        }
    }
    let digits_end = cursor.offset;
    let explicit_unsigned = matches!(cursor.peek(), Some(b'u' | b'U'));
    if explicit_unsigned {
        cursor.bump();
    }
    let text = &cursor.source[start..digits_end];
    let parsed = if hexadecimal {
        u64::from_str_radix(&text[2..], 16)
    } else {
        text.parse::<u64>()
    };
    match parsed {
        Ok(value) if value <= u32::MAX as u64 => tokens.push(Token {
            kind: TokenKind::Integer(IntegerLiteral {
                value: value as u32,
                kind: if explicit_unsigned || value > i32::MAX as u64 {
                    IntegerLiteralKind::Unsigned
                } else {
                    IntegerLiteralKind::Signed
                },
            }),
            span,
        }),
        _ => errors.push(CcError::new(
            span,
            format!("integer literal `{text}` is out of range"),
        )),
    }
}

fn lex_symbol(cursor: &mut Cursor<'_>) -> Option<Symbol> {
    let two = match (cursor.peek(), cursor.peek_next()) {
        (Some(b'='), Some(b'=')) => Some(Symbol::EqualEqual),
        (Some(b'!'), Some(b'=')) => Some(Symbol::BangEqual),
        (Some(b'<'), Some(b'=')) => Some(Symbol::LessEqual),
        (Some(b'>'), Some(b'=')) => Some(Symbol::GreaterEqual),
        (Some(b'&'), Some(b'&')) => Some(Symbol::LogicalAnd),
        (Some(b'|'), Some(b'|')) => Some(Symbol::LogicalOr),
        _ => None,
    };
    if let Some(symbol) = two {
        cursor.bump();
        cursor.bump();
        return Some(symbol);
    }
    let symbol = match cursor.peek()? {
        b'(' => Symbol::LeftParen,
        b')' => Symbol::RightParen,
        b'{' => Symbol::LeftBrace,
        b'}' => Symbol::RightBrace,
        b';' => Symbol::Semicolon,
        b',' => Symbol::Comma,
        b':' => Symbol::Colon,
        b'=' => Symbol::Assign,
        b'+' => Symbol::Plus,
        b'-' => Symbol::Minus,
        b'*' => Symbol::Star,
        b'/' => Symbol::Slash,
        b'%' => Symbol::Percent,
        b'~' => Symbol::Tilde,
        b'!' => Symbol::Bang,
        b'&' => Symbol::Ampersand,
        b'|' => Symbol::Pipe,
        b'^' => Symbol::Caret,
        b'<' => Symbol::Less,
        b'>' => Symbol::Greater,
        _ => return None,
    };
    cursor.bump();
    Some(symbol)
}
