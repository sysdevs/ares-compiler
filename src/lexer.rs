//! The RuneScript lexer, on logos.
//!
//! The lexer takes the longest match and breaks ties by rule order; logos
//! breaks ties by `priority`, so every overlapping rule gets a priority that
//! falls with its position in the grammar. The driver ([`tokenize`]) owns what
//! logos cannot express: the mode stack, the `depth` counter that turns `>`
//! into `STRING_EXPR_END`, and error recovery.
//!
//! Positions: lines are 1-based and bump only on `\n`; columns are
//! 0-based and count code points.

use logos::Logos;
use serde::Serialize;

/// Token types; variant names are the grammar's symbolic names and the
/// discriminants its token numbers. Attributes are the DEFAULT-mode rules; the
/// String-mode types are produced by [`StringToken`].
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Logos, Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
#[repr(u8)]
pub enum TokenKind {
    #[token("(")]
    LPAREN = 1,
    #[token(")")]
    RPAREN,
    #[token(":", priority = 100)]
    COLON,
    #[token(";")]
    SEMICOLON,
    #[token(",")]
    COMMA,
    #[token("[")]
    LBRACK,
    #[token("]")]
    RBRACK,
    #[token("{")]
    LBRACE,
    #[token("}")]
    RBRACE,
    #[token("+", priority = 100)]
    PLUS,
    #[token("-")]
    MINUS,
    #[token("*")]
    MUL,
    #[token("/")]
    DIV,
    #[token(".%")]
    DOTMOD,
    #[token("%")]
    MOD,
    #[token("&")]
    AND,
    #[token("|")]
    OR,
    #[token("=")]
    EQ,
    #[token("!")]
    EXCL,
    #[token("$")]
    DOLLAR,
    #[token("^")]
    CARET,
    #[token("~")]
    TILDE,
    #[token("@")]
    AT,
    #[token(">")]
    GT,
    #[token(">=")]
    GTE,
    #[token("<")]
    LT,
    #[token("<=")]
    LTE,
    #[token("if", priority = 99)]
    IF,
    #[token("else", priority = 99)]
    ELSE,
    #[token("while", priority = 99)]
    WHILE,
    #[token("case", priority = 99)]
    CASE,
    #[token("default", priority = 99)]
    DEFAULT,
    #[token("return", priority = 99)]
    RETURN,
    #[token("calc", priority = 99)]
    CALC,
    #[regex(r"[a-zA-Z0-9_+.:]+array", priority = 98)]
    TYPE_ARRAY,
    #[regex(r"def_[a-zA-Z0-9_+.:]+", priority = 97)]
    DEF_TYPE,
    #[regex(r"switch_[a-zA-Z0-9_+.:]+", priority = 96)]
    SWITCH_TYPE,
    #[regex(r"-?[0-9]+", priority = 95)]
    INTEGER_LITERAL,
    #[regex(r"0[xX][0-9a-fA-F]+", priority = 94)]
    HEX_LITERAL,
    #[regex(r"0[bB][01]+", priority = 93)]
    BIN_LITERAL,
    #[regex(r"[0-9]+_[0-9]+_[0-9]+_[0-9]+_[0-9]+", priority = 92)]
    COORD_LITERAL,
    #[regex(r"[0-9]+_[0-9]+_[0-9]+", priority = 91)]
    MAPZONE_LITERAL,
    #[token("true", priority = 90)]
    #[token("false", priority = 90)]
    BOOLEAN_LITERAL,
    #[regex(r"'(\\[\\']|[^'\\\r\n])'")]
    CHAR_LITERAL,
    #[token("null", priority = 89)]
    NULL_LITERAL,
    // `'//' .*? ('\n' | EOF)`
    #[regex(r"//[^\n]*\n?", allow_greedy = true)]
    LINE_COMMENT,
    // `'/*' .*? '*/'`
    #[regex(r"/\*([^*]|\*+[^*/])*\*+/")]
    BLOCK_COMMENT,
    #[token("\"")]
    QUOTE_OPEN,
    #[regex(r"[a-zA-Z0-9_+.:]+", priority = 1)]
    IDENTIFIER,
    #[regex(r"[ \t\n\r]+")]
    WHITESPACE,
    QUOTE_CLOSE,
    STRING_TEXT,
    STRING_TAG,
    STRING_CLOSE_TAG,
    STRING_PARTIAL_TAG,
    STRING_P_TAG,
    STRING_EXPR_START,
    STRING_EXPR_END,
    /// `EOF` (`-1`); never produced by [`tokenize`], appended by the parser driver.
    EOF,
}

/// The grammar's `String` mode. Its `STRING_EXPR_END : '>'` rule is omitted: it
/// always ties with the earlier `STRING_TEXT`, so it can never be produced.
#[derive(Logos, Clone, Copy, PartialEq, Eq, Debug)]
enum StringToken {
    #[token("\"")]
    QuoteClose,
    #[regex(r#"(\\[\\"<]|[^\\"<\r\n])+"#)]
    Text,
    #[regex(r"<(br|col|str|shad|u|img|gt|lt)(=[^<>]+)?>")]
    Tag,
    #[regex(r"</(br|col|str|shad|u|img|gt|lt)>")]
    CloseTag,
    #[regex(r"<(br|col|str|shad|u|img|gt|lt)=")]
    PartialTag,
    #[regex(r"<p,[^<>]+>")]
    PTag,
    #[token("<")]
    ExprStart,
}

impl From<StringToken> for TokenKind {
    fn from(t: StringToken) -> Self {
        match t {
            StringToken::QuoteClose => TokenKind::QUOTE_CLOSE,
            StringToken::Text => TokenKind::STRING_TEXT,
            StringToken::Tag => TokenKind::STRING_TAG,
            StringToken::CloseTag => TokenKind::STRING_CLOSE_TAG,
            StringToken::PartialTag => TokenKind::STRING_PARTIAL_TAG,
            StringToken::PTag => TokenKind::STRING_P_TAG,
            StringToken::ExprStart => TokenKind::STRING_EXPR_START,
        }
    }
}

impl TokenKind {
    /// `channel(HIDDEN)` rules.
    pub fn is_hidden(self) -> bool {
        matches!(self, TokenKind::LINE_COMMENT | TokenKind::BLOCK_COMMENT | TokenKind::WHITESPACE)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Default,
    String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub text: &'a str,
    pub line: u32,
    pub column: u32,
    /// 0 = default, 1 = hidden.
    pub channel: u8,
    /// Byte offset of `text` in the source.
    pub start: usize,
}

/// A run of characters no token matches (or a line end inside a string), reported at its start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LexError {
    pub line: u32,
    pub column: u32,
    /// The dropped characters.
    pub text: String,
}

impl LexError {
    pub fn message(&self) -> String {
        match self.text.as_str() {
            "\n" | "\r\n" => "unterminated string".to_owned(),
            t => format!("unexpected character `{t}`"),
        }
    }
}

/// Output of [`tokenize`]; `eof_line`/`eof_column` are where the EOF token goes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Lexed<'a> {
    pub tokens: Vec<Token<'a>>,
    pub errors: Vec<LexError>,
    pub eof_line: u32,
    pub eof_column: u32,
}

/// Every token on every channel, without EOF.
pub fn tokenize(src: &str) -> Lexed<'_> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let (mut pos, mut line, mut column) = (0, 1u32, 0u32);
    let mut mode = Mode::Default;
    let mut stack = Vec::new();
    let mut depth = 0i32;

    while pos < src.len() {
        let rest = &src[pos..];
        let matched = match mode {
            Mode::Default => {
                let mut lx = TokenKind::lexer(rest);
                lx.next().and_then(|r| r.ok()).map(|k| (k, lx.span().end))
            }
            Mode::String => {
                let mut lx = StringToken::lexer(rest);
                lx.next().and_then(|r| r.ok()).map(|k| (k.into(), lx.span().end))
            }
        };
        let len = match matched {
            Some((mut kind, len)) => {
                match kind {
                    TokenKind::GT if depth > 0 => {
                        kind = TokenKind::STRING_EXPR_END;
                        mode = stack.pop().expect("depth > 0 implies an open mode");
                    }
                    TokenKind::QUOTE_OPEN => {
                        depth += 1;
                        stack.push(mode);
                        mode = Mode::String;
                    }
                    TokenKind::QUOTE_CLOSE => {
                        depth -= 1;
                        mode = stack.pop().expect("QUOTE_CLOSE only in String mode");
                    }
                    TokenKind::STRING_EXPR_START => {
                        stack.push(mode);
                        mode = Mode::Default;
                    }
                    _ => {}
                }
                let channel = kind.is_hidden() as u8;
                tokens.push(Token { kind, text: &rest[..len], line, column, channel, start: pos });
                len
            }
            None => {
                let len = error_len(mode, rest);
                errors.push(LexError { line, column, text: rest[..len].to_owned() });
                len
            }
        };
        for c in rest[..len].chars() {
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }
        pos += len;
    }
    Lexed { tokens, errors, eof_line: line, eof_column: column }
}

/// Bytes dropped when no rule accepts at the start of `rest`: the prefix a
/// rule consumed before dying, plus one more char unless at EOF. Only
/// `CHAR_LITERAL` (DEFAULT) and the `STRING_TEXT` escape (String) can die after
/// consuming input; every other dead start consumes nothing.
fn error_len(mode: Mode, rest: &str) -> usize {
    let mut chars = rest.chars();
    let first = chars.next().expect("non-empty");
    let c1 = chars.next();
    let viable: &[char] = match (mode, first) {
        (Mode::Default, '\'') => match c1 {
            Some('\\') => match chars.next() {
                Some(c2 @ ('\\' | '\'')) => &['\'', '\\', c2][..],
                _ => &['\'', '\\'][..],
            },
            Some(c) if !matches!(c, '\'' | '\r' | '\n') => &['\'', c][..],
            _ => &['\''][..],
        },
        (Mode::String, '\\') => &['\\'][..],
        _ => &[],
    };
    let consumed: usize = viable.iter().map(|c| c.len_utf8()).sum();
    match rest[consumed..].chars().next() {
        Some(c) => consumed + c.len_utf8(),
        None => consumed,
    }
}
