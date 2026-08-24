use crate::ast::tokens::TokenType;
use std::fmt::Display;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub(crate) enum Operator {
    Add,
    Minus,
    Divide,
    Multiply,
    Exponent,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    Eq,
    NotEq,
    And,
    Or,
    Assign,
    Not,
}

impl Operator {
    pub(super) fn get_op_priority(&self) -> usize {
        match self {
            // follows PEMDAS; operators within the same tier must share a
            // priority so the shunting-yard algorithm in the parser treats
            // them as equally-binding (and therefore left-associative)
            Operator::Exponent => 0,
            Operator::Multiply | Operator::Divide => 1,
            Operator::Add | Operator::Minus => 2,
            Operator::Less
            | Operator::LessEq
            | Operator::Greater
            | Operator::GreaterEq
            | Operator::NotEq
            | Operator::Eq => 3,
            Operator::And => 4,
            Operator::Or => 5,
            Operator::Assign => 6,
            Operator::Not => unreachable!(
                "unary-only operator; never pushed onto the shunting-yard operator stack"
            ),
        }
    }
}

impl Operator {
    pub(super) fn from_token_type(token_type: &TokenType) -> Option<Operator> {
        match token_type {
            TokenType::Plus => Some(Operator::Add),
            TokenType::Minus => Some(Operator::Minus),
            TokenType::Div => Some(Operator::Divide),
            TokenType::Star => Some(Operator::Multiply),
            TokenType::Caret => Some(Operator::Exponent),
            TokenType::Less => Some(Operator::Less),
            TokenType::LessEq => Some(Operator::LessEq),
            TokenType::Greater => Some(Operator::Greater),
            TokenType::GreaterEq => Some(Operator::GreaterEq),
            TokenType::EqEq => Some(Operator::Eq),
            TokenType::NotEq => Some(Operator::NotEq),
            TokenType::AndAnd => Some(Operator::And),
            TokenType::OrOr => Some(Operator::Or),
            TokenType::Eq => Some(Operator::Assign),
            _ => None,
        }
    }
}

impl Display for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operator::Add => write!(f, "+"),
            Operator::Minus => write!(f, "-"),
            Operator::Divide => write!(f, "/"),
            Operator::Multiply => write!(f, "*"),
            Operator::Exponent => write!(f, "^"),
            Operator::Less => write!(f, "<"),
            Operator::LessEq => write!(f, "<="),
            Operator::Greater => write!(f, ">"),
            Operator::GreaterEq => write!(f, ">="),
            Operator::Eq => write!(f, "=="),
            Operator::NotEq => write!(f, "!="),
            Operator::And => write!(f, "&&"),
            Operator::Or => write!(f, "||"),
            Operator::Assign => write!(f, "="),
            Operator::Not => write!(f, "!"),
        }
    }
}
