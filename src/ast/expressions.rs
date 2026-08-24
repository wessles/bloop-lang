use crate::ast::operators::Operator;
use crate::ast::tokens::Token;
use crate::ast::types::Type;
use crate::ast::values::Value;
use crate::positional_error::PositionalError;
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::ops::Deref;

#[derive(Debug, Clone, Hash, PartialEq)]
pub(crate) enum Expression<ExpressionContainer> {
    LetStatement(Box<ExpressionContainer>, Type, Box<ExpressionContainer>),
    PrintStatement(Box<ExpressionContainer>),
    FunctionDefinition(
        Option<String>,
        Vec<(String, Type)>,
        Vec<ExpressionContainer>,
    ),
    FunctionCall(Vec<ExpressionContainer>),
    BinaryOp(Box<ExpressionContainer>, Operator, Box<ExpressionContainer>),
    UnaryOp(Operator, Box<ExpressionContainer>),
    If(
        Box<ExpressionContainer>,
        Box<ExpressionContainer>,
        Option<Box<ExpressionContainer>>,
    ),
    While(Box<ExpressionContainer>, Box<ExpressionContainer>),
    For(
        Option<Box<ExpressionContainer>>, // init: optional `let` statement or assignment
        Box<ExpressionContainer>,         // condition
        Box<ExpressionContainer>,         // update, evaluated after each iteration
        Box<ExpressionContainer>,         // body
    ),
    Block(Vec<ExpressionContainer>),
    Variable(String),
    Constant(Value),
    Tuple(Vec<ExpressionContainer>),
    Unit,
    /// `mod name;` (no body) or `mod name { .. }` (body is a list of
    /// further top-level statements, which may include nested modules).
    Module(String, Option<Vec<ExpressionContainer>>),
    UseStatement(String),
}

impl<ExpressionContainer> Expression<ExpressionContainer> {
    /// A short, human-readable name for this expression's kind, e.g. for
    /// error messages that need to describe *what* wasn't supported without
    /// dumping the whole (potentially large) expression tree via `Debug`.
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Expression::LetStatement(..) => "let statement",
            Expression::PrintStatement(..) => "print statement",
            Expression::FunctionDefinition(..) => "function definition",
            Expression::FunctionCall(..) => "function call",
            Expression::BinaryOp(..) => "binary operator",
            Expression::UnaryOp(..) => "unary operator",
            Expression::If(..) => "if expression",
            Expression::While(..) => "while loop",
            Expression::For(..) => "for loop",
            Expression::Block(..) => "block",
            Expression::Variable(..) => "variable",
            Expression::Constant(..) => "constant",
            Expression::Tuple(..) => "tuple",
            Expression::Unit => "unit",
            Expression::Module(..) => "module statement",
            Expression::UseStatement(..) => "use statement",
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub(crate) struct ExpressionContainer<Info> {
    pub(crate) m_expression: Expression<Self>,
    pub(crate) m_location: (usize, usize),
    pub(crate) m_info: Info,
}

pub(crate) type ParsedExpression = ExpressionContainer<()>;
pub(crate) type TypedExpression = ExpressionContainer<Type>;

impl<Info> Display for ExpressionContainer<Info> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}({},{})",
            self.m_expression, self.m_location.0, self.m_location.1
        )
    }
}

impl ParsedExpression {
    pub(crate) fn new(expression: Expression<Self>, t: &Token) -> Self {
        let Token(location, _) = t;
        ParsedExpression {
            m_expression: expression,
            m_location: *location,
            m_info: (),
        }
    }
}

impl<Info> Deref for ExpressionContainer<Info> {
    type Target = Expression<ExpressionContainer<Info>>;
    fn deref(&self) -> &Self::Target {
        &self.m_expression
    }
}

/// Visits every node in an expression tree depth-first, calling `callback` on
/// each node (including the root) before recursing into its children.
pub(crate) fn visit_expression<Info>(
    expr: &ExpressionContainer<Info>,
    callback: &mut impl FnMut(&ExpressionContainer<Info>) -> Result<(), PositionalError>,
) -> Result<(), PositionalError> {
    callback(expr)?;
    match &expr.m_expression {
        Expression::FunctionDefinition(_, _, body) => {
            for e in body {
                visit_expression(e, callback)?;
            }
        }
        Expression::LetStatement(lhs, _, rhs) => {
            visit_expression(lhs, callback)?;
            visit_expression(rhs, callback)?;
        }
        Expression::PrintStatement(e) => visit_expression(e, callback)?,
        Expression::FunctionCall(args) => {
            for a in args {
                visit_expression(a, callback)?;
            }
        }
        Expression::BinaryOp(lhs, _, rhs) => {
            visit_expression(lhs, callback)?;
            visit_expression(rhs, callback)?;
        }
        Expression::UnaryOp(_, operand) => {
            visit_expression(operand, callback)?;
        }
        Expression::If(cond, then, els) => {
            visit_expression(cond, callback)?;
            visit_expression(then, callback)?;
            if let Some(e) = els {
                visit_expression(e, callback)?;
            }
        }
        Expression::While(cond, body) => {
            visit_expression(cond, callback)?;
            visit_expression(body, callback)?;
        }
        Expression::For(init, cond, update, body) => {
            if let Some(init) = init {
                visit_expression(init, callback)?;
            }
            visit_expression(cond, callback)?;
            visit_expression(update, callback)?;
            visit_expression(body, callback)?;
        }
        Expression::Block(stmts) => {
            for s in stmts {
                visit_expression(s, callback)?;
            }
        }
        Expression::Tuple(elts) => {
            for e in elts {
                visit_expression(e, callback)?;
            }
        }
        Expression::Module(_, body) => {
            if let Some(body) = body {
                for s in body {
                    visit_expression(s, callback)?;
                }
            }
        }
        Expression::Variable(_)
        | Expression::Constant(_)
        | Expression::Unit
        | Expression::UseStatement(_) => {}
    }
    Ok(())
}

/// Mutable counterpart of `visit_expression`: visits every node depth-first,
/// calling `callback` on each node (including the root) before recursing into
/// its children, with mutable access to each node.
pub(crate) fn visit_expression_mut<Info>(
    expr: &mut ExpressionContainer<Info>,
    callback: &mut impl FnMut(&mut ExpressionContainer<Info>),
) {
    callback(expr);
    match &mut expr.m_expression {
        Expression::FunctionDefinition(_, _, body) => {
            for e in body {
                visit_expression_mut(e, callback);
            }
        }
        Expression::LetStatement(lhs, _, rhs) => {
            visit_expression_mut(lhs, callback);
            visit_expression_mut(rhs, callback);
        }
        Expression::PrintStatement(e) => visit_expression_mut(e, callback),
        Expression::FunctionCall(args) => {
            for a in args {
                visit_expression_mut(a, callback);
            }
        }
        Expression::BinaryOp(lhs, _, rhs) => {
            visit_expression_mut(lhs, callback);
            visit_expression_mut(rhs, callback);
        }
        Expression::UnaryOp(_, operand) => {
            visit_expression_mut(operand, callback);
        }
        Expression::If(cond, then, els) => {
            visit_expression_mut(cond, callback);
            visit_expression_mut(then, callback);
            if let Some(e) = els {
                visit_expression_mut(e, callback);
            }
        }
        Expression::While(cond, body) => {
            visit_expression_mut(cond, callback);
            visit_expression_mut(body, callback);
        }
        Expression::For(init, cond, update, body) => {
            if let Some(init) = init {
                visit_expression_mut(init, callback);
            }
            visit_expression_mut(cond, callback);
            visit_expression_mut(update, callback);
            visit_expression_mut(body, callback);
        }
        Expression::Block(stmts) => {
            for s in stmts {
                visit_expression_mut(s, callback);
            }
        }
        Expression::Tuple(elts) => {
            for e in elts {
                visit_expression_mut(e, callback);
            }
        }
        Expression::Module(_, body) => {
            if let Some(body) = body {
                for s in body {
                    visit_expression_mut(s, callback);
                }
            }
        }
        Expression::Variable(_)
        | Expression::Constant(_)
        | Expression::Unit
        | Expression::UseStatement(_) => {}
    }
}

impl<Info> Display for Expression<ExpressionContainer<Info>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Expression::LetStatement(id, _, value) => write!(f, "let {} = {}", id, value),
            Expression::PrintStatement(value) => write!(f, "print {}", value),
            Expression::FunctionCall(args) => {
                let id = match &*args[0] {
                    Expression::Variable(id) => id,
                    _ => panic!("Invalid function call!"),
                };
                write!(f, "{}( ", id)?;
                for arg in &args[1..] {
                    write!(f, "{} ", arg)?;
                }
                write!(f, ")")
            }
            Expression::FunctionDefinition(id_str, params, body) => {
                let params = params
                    .iter()
                    .map(|(name, _type)| name.clone())
                    .collect::<Vec<String>>();
                let body_str = body
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                if let Some(id_str) = id_str {
                    write!(f, "fn {}({}) {{ {} }}", id_str, params.join(", "), body_str)
                } else {
                    write!(f, "fn ({}) {{ {} }}", params.join(", "), body_str)
                }
            }
            Expression::BinaryOp(left, op, right) => {
                write!(f, "({} {} {})", left, op, right)
            }
            Expression::UnaryOp(op, operand) => {
                write!(f, "({}{})", op, operand)
            }
            Expression::If(condition, block_expr, else_expr) => {
                if let Some(else_expr) = else_expr {
                    write!(f, "if ({}) {} else {}", condition, block_expr, else_expr)
                } else {
                    write!(f, "if ({}) {}", condition, block_expr)
                }
            }
            Expression::While(condition, block_expr) => {
                write!(f, "while ({}) {}", condition, block_expr)
            }
            Expression::For(init, condition, update, block_expr) => {
                if let Some(init) = init {
                    write!(
                        f,
                        "for ({}; {}; {}) {}",
                        init, condition, update, block_expr
                    )
                } else {
                    write!(f, "for (; {}; {}) {}", condition, update, block_expr)
                }
            }
            Expression::Block(expressions) => {
                write!(f, "{{ ")?;
                for expression in expressions {
                    write!(f, "{}; ", expression)?;
                }
                write!(f, "}}")
            }
            Expression::Variable(id) => write!(f, "Id({id})",),
            Expression::Constant(lit) => write!(f, "Constant({lit}"),
            Expression::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({})", items)
            }
            Expression::Unit => write!(f, "Unit"),
            Expression::Module(name, None) => write!(f, "mod {}", name),
            Expression::Module(name, Some(body)) => {
                write!(f, "mod {} {{ ", name)?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "}}")
            }
            Expression::UseStatement(name) => write!(f, "use {}", name),
        }
    }
}
