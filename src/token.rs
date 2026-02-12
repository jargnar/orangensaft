use crate::error::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Pipe,
    Question,
    Prompt(String),

    Ident(String),
    Int(i64),
    Float(f64),
    String(String),

    F,
    If,
    Else,
    For,
    In,
    Ret,
    Assert,
    And,
    Or,
    Not,
    True,
    False,
    Nil,

    Newline,
    Indent,
    Dedent,
    Eof,
}

impl TokenKind {
    pub fn same_variant(&self, other: &TokenKind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
