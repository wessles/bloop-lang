pub(crate) mod link_symbols;
pub(crate) mod expressions;
pub(crate) mod operators;
pub(crate) mod parser;
pub(crate) mod tokens;
pub(crate) mod type_checker;
mod type_checker_tests;
pub(crate) mod types;
pub(crate) mod values;

use crate::ast::link_symbols::LinkSymbols;
use crate::ast::expressions::{Expression, ParsedExpression, TypedExpression};
use crate::ast::types::Type;
use crate::positional_error::PositionalError;
use tokens::tokenizer;
use tokens::Token;

#[derive(Clone)]
pub(super) struct AST<ExpressionType> {
    pub(super) m_expressions: Vec<ExpressionType>,
}

pub(super) type ParsedAST = AST<ParsedExpression>;
pub(super) type TypedAST = AST<TypedExpression>;

impl TypedAST {
    pub(super) fn compile_src(source: String) -> Result<TypedAST, PositionalError> {
        Self::compile_src_with_externals(source, LinkSymbols::empty())
    }

    /// Like `compile_src`, but seeds type-checking with `externals` --
    /// qualified-name -> `Type::Function` signatures brought in from other,
    /// already-compiled units (e.g. what `use std;` should resolve to when
    /// `mod std { .. }` isn't defined in `source` itself). See
    /// `exported_function_signatures` for how to produce this map from an
    /// already-checked `TypedAST`.
    pub(super) fn compile_src_with_externals(
        source: String,
        externals: LinkSymbols,
    ) -> Result<TypedAST, PositionalError> {
        let lines = source.split("\n");
        let tokens: Vec<Token> = tokenizer::tokenize(lines)?;
        let parsed_ast = parser::parse(&tokens)?;
        let typed_ast = type_checker::type_check(parsed_ast, externals)?;
        Ok(typed_ast)
    }
}

impl From<ParsedAST> for TypedAST {
    fn from(parsed_ast: ParsedAST) -> Self {
        type PExpression = Expression<ParsedExpression>;
        type TExpression = Expression<TypedExpression>;
        fn convert_parsed_expr_to_typed(expr: PExpression) -> TExpression {
            match expr {
                PExpression::LetStatement(lhs, annotated_type, rhs) => {
                    let lhs = Box::new(convert_parsed_expr_container_to_typed(*lhs));
                    let rhs = Box::new(convert_parsed_expr_container_to_typed(*rhs));
                    TExpression::LetStatement(lhs, annotated_type, rhs)
                }
                PExpression::PrintStatement(expr) => {
                    let inner = Box::new(convert_parsed_expr_container_to_typed(*expr));
                    TExpression::PrintStatement(inner)
                }
                PExpression::FunctionDefinition(id, params, body) => {
                    let body = body
                        .into_iter()
                        .map(convert_parsed_expr_container_to_typed)
                        .collect();
                    TExpression::FunctionDefinition(id, params, body)
                }
                PExpression::FunctionCall(args) => {
                    let args = args
                        .into_iter()
                        .map(convert_parsed_expr_container_to_typed)
                        .collect();
                    TExpression::FunctionCall(args)
                }
                PExpression::BinaryOp(lhs, op, rhs) => {
                    let lhs = Box::new(convert_parsed_expr_container_to_typed(*lhs));
                    let rhs = Box::new(convert_parsed_expr_container_to_typed(*rhs));
                    TExpression::BinaryOp(lhs, op, rhs)
                }
                PExpression::UnaryOp(op, operand) => {
                    let operand = Box::new(convert_parsed_expr_container_to_typed(*operand));
                    TExpression::UnaryOp(op, operand)
                }
                PExpression::If(cond_expr, then_expr, else_expr) => {
                    let cond_expr = Box::new(convert_parsed_expr_container_to_typed(*cond_expr));
                    let then_expr = Box::new(convert_parsed_expr_container_to_typed(*then_expr));
                    let else_expr = else_expr.map(|else_expr| {
                        Box::new(convert_parsed_expr_container_to_typed(*else_expr))
                    });
                    TExpression::If(cond_expr, then_expr, else_expr)
                }
                PExpression::Block(body) => {
                    let body = body
                        .into_iter()
                        .map(convert_parsed_expr_container_to_typed)
                        .collect();
                    TExpression::Block(body)
                }
                PExpression::Variable(id) => TExpression::Variable(id),
                PExpression::Constant(constant) => TExpression::Constant(constant),
                PExpression::Tuple(tuple) => TExpression::Tuple(
                    tuple
                        .into_iter()
                        .map(convert_parsed_expr_container_to_typed)
                        .collect(),
                ),
                PExpression::Unit => TExpression::Unit,
                PExpression::Module(name, body) => TExpression::Module(
                    name,
                    body.map(|body| {
                        body.into_iter()
                            .map(convert_parsed_expr_container_to_typed)
                            .collect()
                    }),
                ),
                PExpression::UseStatement(name) => TExpression::UseStatement(name),
                PExpression::While(cond, body) => TExpression::While(
                    Box::new(convert_parsed_expr_container_to_typed(*cond)),
                    Box::new(convert_parsed_expr_container_to_typed(*body)),
                ),
                PExpression::For(init, cond, update, body) => TExpression::For(
                    init.map(|init| Box::new(convert_parsed_expr_container_to_typed(*init))),
                    Box::new(convert_parsed_expr_container_to_typed(*cond)),
                    Box::new(convert_parsed_expr_container_to_typed(*update)),
                    Box::new(convert_parsed_expr_container_to_typed(*body)),
                ),
            }
        }

        fn convert_parsed_expr_container_to_typed(expr: ParsedExpression) -> TypedExpression {
            TypedExpression {
                m_expression: convert_parsed_expr_to_typed(expr.m_expression),
                m_location: expr.m_location,
                m_info: Type::Unknown,
            }
        }

        TypedAST {
            m_expressions: parsed_ast
                .m_expressions
                .into_iter()
                .map(convert_parsed_expr_container_to_typed)
                .collect(),
        }
    }
}
