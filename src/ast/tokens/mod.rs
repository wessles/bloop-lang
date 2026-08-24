pub(crate) mod tokenizer;

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum KeywordType {
    If,
    Else,
    While,
    For,
    Fn,
    Let,
    Print,
    I64,
    F64,
    String,
    Mod,
    Use,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenType {
    Keyword(KeywordType),
    Identifier(String),
    Integer(i64),
    Double(f64),
    String(String),
    Plus,
    Minus,
    Div,
    Star,
    Caret,
    PlusEq,
    MinusEq,
    DivEq,
    StarEq,
    CaretEq,
    Eq,
    EqEq,
    Not,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    AndAnd,
    OrOr,
    LParen,
    RParen,
    Comma,
    Semicolon,
    LBrace,
    RBrace,
    Colon,
    ColonColon,
    Arrow,
    True,
    False,
    EndOfTokens,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
// ((lineNo, charNo), type)
pub(crate) struct Token(pub(crate) (usize, usize), pub(crate) TokenType);

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} at {:?}", self.1, self.0)
    }
}
