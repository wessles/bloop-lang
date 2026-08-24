use crate::ast::expressions::{Expression, ParsedExpression};
use crate::ast::operators::Operator;
use crate::ast::tokens::{KeywordType, Token, TokenType};
use crate::ast::types::Type;
use crate::ast::values::Value;
use crate::ast::ParsedAST;
use crate::positional_error::PositionalError;
use std::error::Error;
use std::fmt::Display;

#[derive(Debug)]
enum ParsingError {
    UnexpectedEndOfExpression,
    ExpectedClosingParenthesis,
    InvalidFunctionDefinitionExpectedParameterIdentifiers,
    InvalidFunctionDefinitionExpectedBody,
    UnexpectedKeyword(KeywordType),
    UnexpectedToken(TokenType),
    InvalidLetStatement,
    InvalidPrintStatement,
    UnexpectedOperator,
    UnexpectedExpression,
}
impl Display for ParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParsingError::UnexpectedToken(t) => write!(f, "Unexpected token {}", t),
            ParsingError::ExpectedClosingParenthesis => {
                write!(f, "Expected closing parenthesis!")
            }
            ParsingError::InvalidFunctionDefinitionExpectedParameterIdentifiers => write!(
                f,
                "Invalid function definition! Expected parameter identifiers."
            ),
            ParsingError::InvalidFunctionDefinitionExpectedBody => write!(
                f,
                "Invalid function definition! Expected function body expression."
            ),
            ParsingError::UnexpectedKeyword(keyword) => {
                write!(f, "Unexpected keyword: {keyword:?}")
            }
            ParsingError::UnexpectedEndOfExpression => {
                write!(f, "Unexpected end of expression")
            }
            ParsingError::InvalidLetStatement => write!(f, "Invalid let statement"),
            ParsingError::InvalidPrintStatement => write!(f, "Invalid print statement"),
            ParsingError::UnexpectedExpression => write!(f, "Unexpected expression"),
            ParsingError::UnexpectedOperator => write!(f, "Unexpected operator"),
        }
    }
}
impl Error for ParsingError {}

fn parser_error(error_type: ParsingError, token: &Token) -> PositionalError {
    let Token((line_no, char_no), _token_type) = token;
    PositionalError::new(Box::new(error_type), *line_no, *char_no)
}

fn parser_error_unexpected(token: &Token) -> PositionalError {
    let Token(_, token_type) = &token;
    parser_error(ParsingError::UnexpectedToken(token_type.clone()), token)
}

type ParsedStatementsResult<'a> = Result<(&'a [Token], Vec<ParsedExpression>), PositionalError>;
type OptionalParsedExpressionResult<'a> =
    Result<(&'a [Token], Option<ParsedExpression>), PositionalError>;
type MandatoryParsedExpressionResult<'a> = Result<(&'a [Token], ParsedExpression), PositionalError>;
type ParsedTypeResult<'a> = Result<(&'a [Token], Type), PositionalError>;
type ParsedTypeTupleResult<'a> = Result<(&'a [Token], Vec<Type>), PositionalError>;

fn consume_token(tokens: &mut &[Token]) {
    *tokens = &tokens[1..];
}

macro_rules! consume_expected_or_error {
    ($tokens:expr, $token_type:pat) => {
        match $tokens {
            [token @ Token(_, $token_type), ..] => {
                $tokens = &$tokens[1..];
                token
            }
            [Token(_, TokenType::EndOfTokens)] => {
                return Err(parser_error(
                    ParsingError::UnexpectedEndOfExpression,
                    &$tokens[0],
                ))
            }
            _ => return Err(parser_error_unexpected(&$tokens[0])),
        }
    };
}

fn parse_expression(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    parse_expression_rpn(start, false)
}

fn parse_expression_no_operators(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    parse_expression_rpn(start, true)
}

fn parse_expression_rpn(
    start: &[Token],
    just_read_one_expression: bool,
) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;

    // using the Shunting-yard algorithm

    // A compound-assignment operator (`+=`, `-=`, `*=`, `/=`, `^=`) is
    // syntactic sugar for `lhs = lhs <op> rhs`; it shares `=`'s (lowest)
    // priority, but expands into a nested `Assign` node rather than a flat
    // `BinaryOp` once its operands are popped off the queue.
    enum ShuntingOperator {
        Plain(Operator),
        CompoundAssign(Operator),
    }
    impl ShuntingOperator {
        fn get_op_priority(&self) -> usize {
            match self {
                ShuntingOperator::Plain(op) => op.get_op_priority(),
                ShuntingOperator::CompoundAssign(_) => Operator::Assign.get_op_priority(),
            }
        }
    }
    enum RpnItem {
        Expression(ParsedExpression),
        Operator(ShuntingOperator),
    }
    let mut rpn_queue: Vec<RpnItem> = Vec::new();
    let mut operator_stack: Vec<ShuntingOperator> = Vec::new();

    // We only expect a sub-expression at the beginning and after an operator.
    // Two expressions or operators in a row are not allowed.
    let mut expecting_expression = true;

    loop {
        match tokens {
            // empty token list
            [Token(_, TokenType::EndOfTokens)] => {
                if expecting_expression {
                    return Err(parser_error(
                        ParsingError::UnexpectedEndOfExpression,
                        &tokens[0],
                    ));
                } else {
                    break;
                }
            }

            // unary minus/not: only valid where an expression is expected --
            // otherwise `TokenType::Minus` below is the binary operator (`!`
            // has no binary meaning, but is likewise only legal here).
            [Token(_, token @ (TokenType::Minus | TokenType::Not)), ..]
                if expecting_expression =>
            {
                let op = if *token == TokenType::Minus {
                    Operator::Minus
                } else {
                    Operator::Not
                };
                let op_token = &tokens[0];
                tokens = &tokens[1..];
                let operand;
                (tokens, operand) = parse_unary_operand(tokens)?;
                let unary_expr =
                    ParsedExpression::new(Expression::UnaryOp(op, Box::new(operand)), op_token);

                if just_read_one_expression {
                    return Ok((tokens, Some(unary_expr)));
                }

                expecting_expression = false;
                rpn_queue.push(RpnItem::Expression(unary_expr));
                continue;
            }

            // handle operators if present
            [
                Token(
                    _,
                    token @ (TokenType::Plus
                    | TokenType::Minus
                    | TokenType::Div
                    | TokenType::Star
                    | TokenType::Caret
                    | TokenType::Less
                    | TokenType::LessEq
                    | TokenType::Greater
                    | TokenType::GreaterEq
                    | TokenType::EqEq
                    | TokenType::NotEq
                    | TokenType::AndAnd
                    | TokenType::OrOr
                    | TokenType::Eq
                    | TokenType::PlusEq
                    | TokenType::MinusEq
                    | TokenType::DivEq
                    | TokenType::StarEq
                    | TokenType::CaretEq),
                ),
                ..,
            ] => {
                if expecting_expression {
                    return Err(parser_error(ParsingError::UnexpectedOperator, &tokens[0]));
                } else {
                    expecting_expression = true;
                }

                tokens = &tokens[1..]; // consume the operator token

                let op = match token {
                    TokenType::PlusEq => ShuntingOperator::CompoundAssign(Operator::Add),
                    TokenType::MinusEq => ShuntingOperator::CompoundAssign(Operator::Minus),
                    TokenType::DivEq => ShuntingOperator::CompoundAssign(Operator::Divide),
                    TokenType::StarEq => ShuntingOperator::CompoundAssign(Operator::Multiply),
                    TokenType::CaretEq => ShuntingOperator::CompoundAssign(Operator::Exponent),
                    _ => ShuntingOperator::Plain(Operator::from_token_type(token).unwrap()),
                };
                while let Some(top) = operator_stack.last() {
                    if top.get_op_priority() > op.get_op_priority() {
                        break;
                    }
                    let top = operator_stack.pop().unwrap();
                    rpn_queue.push(RpnItem::Operator(top));
                }
                operator_stack.push(op);
                continue;
            }
            _ => (),
        }

        let token_at_start = &tokens[0];

        let expr;
        (tokens, expr) = parse_atom(tokens)?;
        let expr = match expr {
            Some(expr) => expr,
            None => break,
        };

        if just_read_one_expression {
            return Ok((tokens, Some(expr)));
        }

        if !expecting_expression {
            return Err(parser_error(
                ParsingError::UnexpectedExpression,
                token_at_start,
            ));
        } else {
            expecting_expression = false;
        }
        rpn_queue.push(RpnItem::Expression(expr));
    }

    // pop the rest of the operators into the rpn_queue
    while let Some(top) = operator_stack.pop() {
        rpn_queue.push(RpnItem::Operator(top));
    }

    fn pop_expression_from_rpn(
        rpn_queue: &mut Vec<RpnItem>,
        start: &[Token],
    ) -> Option<ParsedExpression> {
        if rpn_queue.is_empty() {
            return None;
        }
        let top = rpn_queue.pop().unwrap();
        match top {
            RpnItem::Expression(expr) => Some(expr),
            RpnItem::Operator(op) => {
                // reverse order since this is a queue we're iterating back through
                let right = pop_expression_from_rpn(rpn_queue, start)?;
                let left = pop_expression_from_rpn(rpn_queue, start)?;
                let expr = match op {
                    ShuntingOperator::Plain(op) => {
                        Expression::BinaryOp(Box::new(left), op, Box::new(right))
                    }
                    ShuntingOperator::CompoundAssign(base_op) => {
                        let combined = ParsedExpression::new(
                            Expression::BinaryOp(Box::new(left.clone()), base_op, Box::new(right)),
                            &start[0],
                        );
                        Expression::BinaryOp(Box::new(left), Operator::Assign, Box::new(combined))
                    }
                };
                Some(ParsedExpression::new(expr, &start[0]))
            }
        }
    }
    let expr = pop_expression_from_rpn(&mut rpn_queue, start);
    Ok((tokens, expr))
}

// Parses a single non-operator atom: literals, identifiers, function calls,
// parenthesized/tuple expressions, anonymous functions, and blocks. Returns
// `None` (without consuming anything) on a token that ends an expression
// (`;`, `)`, `}`, `,`, `=`, or a keyword) rather than erroring, so callers
// decide for themselves whether that's expected.
fn parse_atom(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;
    let token_at_start = &tokens[0];

    match tokens {
        // subexpression
        [Token(_, TokenType::LParen), ..] => {
            tokens = &tokens[1..];
            let start = &tokens[0];
            let expression_list;
            (tokens, expression_list) = parse_expressions_comma_separated(tokens)?;
            let expr = match &expression_list {
                None => Expression::Unit,
                Some(list) => match list.as_slice() {
                    [] => Expression::Unit,
                    [only] => only.m_expression.clone(),
                    [..] => Expression::Tuple(list.clone()),
                },
            };
            Ok((tokens, Some(ParsedExpression::new(expr, start))))
        }

        // identifier, or a `module::child::name` path -- either a function
        // call (if followed by `(`) or a variable/function reference
        [Token(_, TokenType::Identifier(_)), ..] => {
            let path;
            (tokens, path) = parse_qualified_identifier(tokens)?;
            if let [Token(_, TokenType::LParen), ..] = tokens {
                tokens = &tokens[1..];
                let expr;
                (tokens, expr) = parse_function_call_expression(tokens, path)?;
                Ok((tokens, Some(expr)))
            } else {
                Ok((
                    tokens,
                    Some(ParsedExpression::new(
                        Expression::Variable(path),
                        token_at_start,
                    )),
                ))
            }
        }

        // literals
        [Token(_, TokenType::String(val)), ..] => {
            consume_token(&mut tokens);
            Ok((
                tokens,
                Some(ParsedExpression::new(
                    Expression::Constant(Value::String(val.to_string())),
                    token_at_start,
                )),
            ))
        }
        [Token(_, TokenType::Integer(val)), ..] => {
            consume_token(&mut tokens);
            Ok((
                tokens,
                Some(ParsedExpression::new(
                    Expression::Constant(Value::Integer(*val)),
                    token_at_start,
                )),
            ))
        }
        [Token(_, TokenType::Double(val)), ..] => {
            consume_token(&mut tokens);
            Ok((
                tokens,
                Some(ParsedExpression::new(
                    Expression::Constant(Value::Double(*val)),
                    token_at_start,
                )),
            ))
        }
        [
            Token(_, val_token @ (TokenType::True | TokenType::False)),
            ..,
        ] => {
            consume_token(&mut tokens);
            let value = *val_token == TokenType::True;
            Ok((
                tokens,
                Some(ParsedExpression::new(
                    Expression::Constant(Value::Boolean(value)),
                    token_at_start,
                )),
            ))
        }

        // anonymous function call
        [Token(_, TokenType::Keyword(KeywordType::Fn)), ..] => {
            let expr;
            (tokens, expr) = parse_function_definition(&tokens[1..])?;
            Ok((tokens, Some(expr)))
        }

        // parse a brace
        [Token(_, TokenType::LBrace), ..] => {
            let expr;
            (tokens, expr) = parse_block_expression(tokens)?;
            Ok((tokens, expr))
        }

        // end of expression tokens
        [
            Token(
                _,
                TokenType::Semicolon
                | TokenType::RParen
                | TokenType::RBrace
                | TokenType::Comma
                | TokenType::Eq
                | TokenType::Keyword(_),
            ),
            ..,
        ] => Ok((tokens, None)),

        [..] => Err(parser_error_unexpected(&tokens[0])),
    }
}

// Parses the operand of a unary minus: either another unary minus (so
// `--x` parses as `-(-x)`), or a single atom. Unlike a general expression,
// this never consumes any following binary operators -- those are left for
// the enclosing `parse_expression_rpn` to combine with correct precedence.
fn parse_unary_operand(start: &[Token]) -> MandatoryParsedExpressionResult<'_> {
    let mut tokens = start;

    if let [Token(_, token @ (TokenType::Minus | TokenType::Not)), ..] = tokens {
        let op = if *token == TokenType::Minus {
            Operator::Minus
        } else {
            Operator::Not
        };
        let op_token = &tokens[0];
        tokens = &tokens[1..];
        let operand;
        (tokens, operand) = parse_unary_operand(tokens)?;
        return Ok((
            tokens,
            ParsedExpression::new(Expression::UnaryOp(op, Box::new(operand)), op_token),
        ));
    }

    let token_at_start = &tokens[0];
    let expr;
    (tokens, expr) = parse_atom(tokens)?;
    match expr {
        Some(expr) => Ok((tokens, expr)),
        None => Err(parser_error(
            ParsingError::UnexpectedEndOfExpression,
            token_at_start,
        )),
    }
}

// Parses `ident (:: ident)*` into a single `::`-joined path string, e.g.
// `outer::inner::name`. Used wherever a reference (variable, function call,
// import) may name an item by its module path, Rust-style.
fn parse_qualified_identifier(start: &[Token]) -> Result<(&[Token], String), PositionalError> {
    let mut tokens = start;

    let mut path = match tokens {
        [Token(_, TokenType::Identifier(id)), ..] => {
            tokens = &tokens[1..];
            id.clone()
        }
        _ => return Err(parser_error_unexpected(&tokens[0])),
    };

    while let [Token(_, TokenType::ColonColon), ..] = tokens {
        tokens = &tokens[1..];
        match tokens {
            [Token(_, TokenType::Identifier(id)), ..] => {
                path.push_str("::");
                path.push_str(id);
                tokens = &tokens[1..];
            }
            _ => return Err(parser_error_unexpected(&tokens[0])),
        }
    }

    Ok((tokens, path))
}

fn parse_function_definition(start: &[Token]) -> MandatoryParsedExpressionResult<'_> {
    let mut tokens = start;

    let name = match tokens {
        [
            Token(_, TokenType::Identifier(s)),
            Token(_, TokenType::LParen),
            ..,
        ] => {
            tokens = &tokens[2..]; // consume 2
            Some(s.clone())
        }
        [Token(_, TokenType::LParen), ..] => {
            tokens = &tokens[1..]; // consume 1
            None
        }
        _ => {
            return Err(parser_error_unexpected(&tokens[0]));
        }
    };

    let mut params: Vec<(String, Type)> = Vec::new();
    {
        let start = &tokens[0];

        loop {
            match tokens {
                [
                    Token(_, TokenType::Identifier(id)),
                    Token(_, TokenType::Colon),
                    ..,
                ] => {
                    tokens = &tokens[2..];
                    let typee;
                    (tokens, typee) = parse_type(tokens)?;
                    params.push((id.clone(), typee));
                }
                [Token(_, TokenType::Identifier(id)), ..] => {
                    tokens = &tokens[1..];
                    params.push((id.clone(), Type::Unknown));
                }
                [Token(_, TokenType::RParen), ..] => {
                    consume_token(&mut tokens);
                    break;
                }
                _ => {
                    return Err(parser_error(
                        ParsingError::InvalidFunctionDefinitionExpectedParameterIdentifiers,
                        start,
                    ));
                }
            }
            if let [Token(_, TokenType::Comma), ..] = tokens {
                consume_token(&mut tokens);
                continue;
            }
        }
    }

    let body;
    (tokens, body) = parse_block_expression(tokens)?;
    let body = match body {
        Some(expr) => expr,
        None => {
            return Err(parser_error(
                ParsingError::InvalidFunctionDefinitionExpectedBody,
                &tokens[0],
            ));
        }
    };

    let body_exprs = match body.m_expression {
        Expression::Block(exprs) => exprs,
        _ => unreachable!(),
    };

    Ok((
        tokens,
        ParsedExpression::new(
            Expression::FunctionDefinition(name, params, body_exprs),
            &start[0],
        ),
    ))
}

fn parse_type_tuple(start: &[Token]) -> ParsedTypeTupleResult<'_> {
    let mut tokens = start;

    let mut types = Vec::<Type>::new();

    loop {
        match tokens {
            [Token(_, TokenType::LParen), ..] => {
                consume_token(&mut tokens);
                let elements;
                (tokens, elements) = parse_type_tuple(tokens)?;
                types.push(Type::Tuple(elements));
            }
            [Token(_, TokenType::RParen), ..] => {
                consume_token(&mut tokens);
                return Ok((tokens, types));
            }
            [Token(_, TokenType::Keyword(_)), ..] => {
                let typ;
                (tokens, typ) = parse_type(tokens)?;
                types.push(typ);
            }
            _ => {
                return Err(parser_error(
                    ParsingError::UnexpectedToken(tokens[0].1.clone()),
                    &tokens[0],
                ));
            }
        }
        if tokens[0].1 == TokenType::Comma {
            consume_token(&mut tokens);
        }
    }
}

fn parse_type(start: &[Token]) -> ParsedTypeResult<'_> {
    let mut tokens = start;

    let result = match tokens {
        [Token(_, TokenType::LParen), ..] => {
            consume_token(&mut tokens);
            let elts;
            (tokens, elts) = parse_type_tuple(tokens)?;
            if !elts.is_empty() {
                Ok(Type::Tuple(elts))
            } else {
                Ok(Type::Unit)
            }
        }
        [Token(_, TokenType::Keyword(keyword)), ..] => match keyword {
            KeywordType::I64 => {
                consume_token(&mut tokens);
                Ok(Type::I64)
            }
            KeywordType::F64 => {
                consume_token(&mut tokens);
                Ok(Type::F64)
            }
            KeywordType::String => {
                consume_token(&mut tokens);
                Ok(Type::String)
            }
            KeywordType::Fn => {
                consume_token(&mut tokens);

                let args = {
                    let input_t;
                    (tokens, input_t) = parse_type(tokens)?;

                    match input_t {
                        Type::Tuple(types) => types,
                        other => vec![other],
                    }
                };

                consume_expected_or_error!(tokens, TokenType::Arrow);

                let output_type;
                (tokens, output_type) = parse_type(tokens)?;

                Ok(Type::Function(args, Box::new(output_type)))
            }
            _ => Err(parser_error_unexpected(&tokens[1])),
        },
        [token, ..] => Err(parser_error_unexpected(token)),
        [] => Err(parser_error(
            ParsingError::UnexpectedEndOfExpression,
            &tokens[0],
        )),
    }?;
    Ok((tokens, result))
}

fn parse_let_statement(start: &[Token]) -> MandatoryParsedExpressionResult<'_> {
    let mut tokens = start;

    let assigned;
    (tokens, assigned) = parse_expression_no_operators(tokens)?;
    match &assigned {
        Some(ParsedExpression {
            m_expression: Expression::Variable(id),
            ..
        }) if id.contains("::") => {
            return Err(parser_error(ParsingError::InvalidLetStatement, &start[0]));
        }
        Some(ParsedExpression {
            m_expression: Expression::Variable(_) | Expression::Tuple(_),
            ..
        }) => (),
        _ => {
            return Err(parser_error(
                ParsingError::UnexpectedEndOfExpression,
                &start[0],
            ));
        }
    };
    let assigned = assigned.unwrap();

    // read optional type annotation
    let var_type = if tokens[0].1 == TokenType::Colon {
        consume_token(&mut tokens);
        let var_type: Type;
        (tokens, var_type) = parse_type(tokens)?;
        var_type
    } else {
        Type::Unknown
    };

    consume_expected_or_error!(tokens, TokenType::Eq);

    let value = {
        let start = &tokens[0];
        let value;
        (tokens, value) = parse_expression(tokens)?;
        match value {
            Some(value) => value,
            None => {
                return Err(parser_error(ParsingError::InvalidLetStatement, start));
            }
        }
    };

    Ok((
        tokens,
        ParsedExpression::new(
            Expression::LetStatement(Box::new(assigned), var_type, Box::new(value)),
            &start[0],
        ),
    ))
}

// Parses `mod name;` or `mod name { .. }`. The body of a module block is a
// list of further top-level statements (via `parse_top_level_block`), so a
// module can nest another one, and `use` (valid everywhere) works inside it
// too.
fn parse_mod_statement(start: &[Token]) -> MandatoryParsedExpressionResult<'_> {
    let mut tokens = start;
    let start_token = &tokens[0];
    let name_token = consume_expected_or_error!(tokens, TokenType::Identifier(_));
    let name = match &name_token.1 {
        TokenType::Identifier(id) => id.clone(),
        _ => unreachable!(),
    };

    let body = if let [Token(_, TokenType::LBrace), ..] = tokens {
        let body;
        (tokens, body) = parse_top_level_block(tokens)?;
        Some(body)
    } else {
        None
    };

    Ok((
        tokens,
        ParsedExpression::new(Expression::Module(name, body), start_token),
    ))
}

fn parse_use_statement(start: &[Token]) -> MandatoryParsedExpressionResult<'_> {
    let start_token = &start[0];
    let (tokens, path) = parse_qualified_identifier(start)?;
    Ok((
        tokens,
        ParsedExpression::new(Expression::UseStatement(path), start_token),
    ))
}

fn parse_print_statement(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;
    let start = &tokens[0];
    let value;
    (tokens, value) = parse_expression(tokens)?;
    let value = match value {
        Some(value) => value,
        None => {
            return Err(parser_error(ParsingError::InvalidPrintStatement, start));
        }
    };

    let expr = ParsedExpression::new(Expression::PrintStatement(Box::new(value)), start);
    Ok((tokens, Some(expr)))
}

fn parse_if_expression(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;

    consume_expected_or_error!(tokens, TokenType::LParen);

    let condition = {
        let start = &tokens[0];
        let condition;
        (tokens, condition) = parse_expression(tokens)?;
        match condition {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    consume_expected_or_error!(tokens, TokenType::RParen);

    let block_expr = {
        let start = &tokens[0];
        let block_expr;
        (tokens, block_expr) = parse_block_expression(tokens)?;
        match block_expr {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    let else_expr: Option<Box<ParsedExpression>> =
        if let [Token(_, TokenType::Keyword(KeywordType::Else)), Token(_, TokenType::Keyword(KeywordType::If)), ..] =
            tokens
        {
            let else_start = &tokens[0];
            tokens = &tokens[2..];

            let start = &tokens[0];
            let chained_if;
            (tokens, chained_if) = parse_if_expression(tokens)?;
            match chained_if {
                // `else if ..` is sugar for `else { if .. }` -- wrapping it in
                // a Block keeps it shaped exactly like a manually-nested
                // `else { if .. }`, which compile_if_branch (blir_lowering.rs)
                // already knows how to lower both as a value and as a
                // statement.
                Some(expr) => Some(Box::new(ParsedExpression::new(
                    Expression::Block(vec![expr]),
                    else_start,
                ))),
                None => {
                    return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
                }
            }
        } else if let [Token(_, TokenType::Keyword(KeywordType::Else)), ..] = tokens {
            tokens = &tokens[1..];

            let start = &tokens[0];
            let else_block;
            (tokens, else_block) = parse_block_expression(tokens)?;
            match else_block {
                Some(expr) => Some(Box::new(expr)),
                None => {
                    return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
                }
            }
        } else {
            None
        };

    Ok((
        tokens,
        Some(ParsedExpression::new(
            Expression::If(Box::new(condition), Box::new(block_expr), else_expr),
            &start[0],
        )),
    ))
}

fn parse_while_expression(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;

    consume_expected_or_error!(tokens, TokenType::LParen);

    let condition = {
        let start = &tokens[0];
        let condition;
        (tokens, condition) = parse_expression(tokens)?;
        match condition {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    consume_expected_or_error!(tokens, TokenType::RParen);

    let block_expr = {
        let start = &tokens[0];
        let block_expr;
        (tokens, block_expr) = parse_expression_no_operators(tokens)?;
        match block_expr {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    Ok((
        tokens,
        Some(ParsedExpression::new(
            Expression::While(Box::new(condition), Box::new(block_expr)),
            &start[0],
        )),
    ))
}

fn parse_for_expression(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;

    consume_expected_or_error!(tokens, TokenType::LParen);

    // The init clause is optional, and -- when present -- may be either a
    // `let` statement or a plain assignment/expression (e.g. reusing an
    // existing variable). Delegating to parse_statement (rather than
    // parse_expression, which can't parse a `let` at all -- parse_atom
    // treats every keyword as an end-of-expression marker and returns
    // without consuming it) reuses its existing `let`-keyword handling
    // instead of duplicating it here.
    let init: Option<ParsedExpression> = match tokens {
        [Token(_, TokenType::Semicolon), ..] => None,
        _ => {
            let init;
            (tokens, init) = parse_statement(tokens)?;
            init
        }
    };

    consume_expected_or_error!(tokens, TokenType::Semicolon);

    let condition = {
        let start = &tokens[0];
        let condition;
        (tokens, condition) = parse_expression(tokens)?;
        match condition {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    consume_expected_or_error!(tokens, TokenType::Semicolon);

    let update = {
        let start = &tokens[0];
        let update;
        (tokens, update) = parse_expression(tokens)?;
        match update {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    consume_expected_or_error!(tokens, TokenType::RParen);

    let body = {
        let start = &tokens[0];
        let body;
        (tokens, body) = parse_expression_no_operators(tokens)?;
        match body {
            Some(expr) => expr,
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        }
    };

    Ok((
        tokens,
        Some(ParsedExpression::new(
            Expression::For(
                init.map(Box::new),
                Box::new(condition),
                Box::new(update),
                Box::new(body),
            ),
            &start[0],
        )),
    ))
}

fn parse_block_expression(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;

    consume_expected_or_error!(tokens, TokenType::LBrace);

    let mut expressions = Vec::new();

    loop {
        match tokens {
            [Token(_, TokenType::RBrace), ..] => {
                consume_token(&mut tokens);
                break;
            }
            [] => {
                return Err(parser_error(
                    ParsingError::UnexpectedEndOfExpression,
                    &tokens[0],
                ));
            }
            _ => (),
        }

        let expr;
        (tokens, expr) = parse_statement(tokens)?;
        if let Some(expr) = expr {
            expressions.push(expr);
        }
    }

    Ok((
        tokens,
        Some(ParsedExpression::new(
            Expression::Block(expressions),
            &start[0],
        )),
    ))
}

fn parse_function_call_expression(
    start: &[Token],
    id: String,
) -> MandatoryParsedExpressionResult<'_> {
    let mut tokens = start;

    let mut args = vec![ParsedExpression::new(Expression::Variable(id), &tokens[0])];
    let call_args;
    (tokens, call_args) = parse_expressions_comma_separated(tokens)?;
    if let Some(call_args) = call_args {
        args.extend(call_args);
    }

    Ok((
        tokens,
        ParsedExpression::new(Expression::FunctionCall(args), &start[0]),
    ))
}

fn parse_expressions_comma_separated(
    start: &[Token],
) -> Result<(&[Token], Option<Vec<ParsedExpression>>), PositionalError> {
    let mut tokens = start;

    let mut elements: Vec<ParsedExpression> = Vec::new();

    loop {
        match tokens {
            [Token(_, TokenType::RParen), ..] => {
                tokens = &tokens[1..];
                break;
            }
            [] => {
                return Err(parser_error(
                    ParsingError::UnexpectedEndOfExpression,
                    &tokens[0],
                ));
            }
            [..] => {}
        }

        let start = &tokens[0];
        let expr;
        (tokens, expr) = parse_expression(tokens)?;
        match expr {
            Some(expr) => elements.push(expr),
            None => {
                return Err(parser_error(ParsingError::UnexpectedEndOfExpression, start));
            }
        };

        match tokens {
            [Token(_, TokenType::RParen), ..] => {
                tokens = &tokens[1..];
                break;
            }
            [Token(_, TokenType::Comma), ..] => {
                tokens = &tokens[1..];
                continue;
            }
            _ => {
                return Err(parser_error(
                    ParsingError::ExpectedClosingParenthesis,
                    &tokens[0],
                ));
            }
        }
    }
    Ok((tokens, (!elements.is_empty()).then_some(elements)))
}

fn parse_statements(start: &[Token]) -> ParsedStatementsResult<'_> {
    let mut tokens = start;

    let mut statements = Vec::new();

    loop {
        match tokens {
            // the last token should always be EndOfTokens
            [Token(_, TokenType::EndOfTokens)] => {
                consume_token(&mut tokens);
                break;
            }
            [] => panic!("Last token was not EndOfTokens!"),
            _ => {}
        }
        let statement;
        (tokens, statement) = parse_top_level_statement(tokens)?;
        if let Some(statement) = statement {
            statements.push(statement);
        }
    }

    Ok((tokens, statements))
}

// A statement, plus the top-level-only `mod` form (`use` is handled by
// `parse_statement` itself, so it's valid everywhere a statement is, not
// just here). Used for the program's own top level and for a module
// block's own body, so modules can nest; `mod` inside a function body or
// an if/while/for block still falls through to `parse_statement`'s
// "unexpected keyword" error.
fn parse_top_level_statement(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let tokens = start;

    match tokens {
        [Token(_, TokenType::Keyword(KeywordType::Mod)), ..] => {
            let (tokens, expr) = parse_mod_statement(&tokens[1..])?;
            Ok((tokens, Some(expr)))
        }
        _ => parse_statement(tokens),
    }
}

// Parses the body of a `mod name { .. }` block: a brace-delimited list of
// further top-level statements, so a module can nest another one.
fn parse_top_level_block(start: &[Token]) -> ParsedStatementsResult<'_> {
    let mut tokens = start;

    consume_expected_or_error!(tokens, TokenType::LBrace);

    let mut statements = Vec::new();

    loop {
        match tokens {
            [Token(_, TokenType::RBrace), ..] => {
                consume_token(&mut tokens);
                break;
            }
            [] => {
                return Err(parser_error(
                    ParsingError::UnexpectedEndOfExpression,
                    &tokens[0],
                ));
            }
            _ => {}
        }

        let statement;
        (tokens, statement) = parse_top_level_statement(tokens)?;
        if let Some(statement) = statement {
            statements.push(statement);
        }
    }

    Ok((tokens, statements))
}

fn parse_statement(start: &[Token]) -> OptionalParsedExpressionResult<'_> {
    let mut tokens = start;

    match tokens {
        // abort on this
        [Token(_, TokenType::RBrace), ..] => Ok((tokens, None)),

        // ignore these
        [Token(_, TokenType::Semicolon), ..] => {
            consume_token(&mut tokens);
            Ok((tokens, None))
        }

        // statement types
        [Token(_, TokenType::Keyword(keyword)), ..] => match keyword {
            KeywordType::Fn => {
                let expr;
                (tokens, expr) = parse_function_definition(&tokens[1..])?;
                Ok((tokens, Some(expr)))
            }
            KeywordType::Let => {
                let expr;
                (tokens, expr) = parse_let_statement(&tokens[1..])?;
                Ok((tokens, Some(expr)))
            }
            KeywordType::Print => {
                let expr;
                (tokens, expr) = parse_print_statement(&tokens[1..])?;
                Ok((tokens, expr))
            }
            KeywordType::If => {
                let expr;
                (tokens, expr) = parse_if_expression(&tokens[1..])?;
                Ok((tokens, expr))
            }
            KeywordType::While => {
                let expr;
                (tokens, expr) = parse_while_expression(&tokens[1..])?;
                Ok((tokens, expr))
            }
            KeywordType::For => {
                let expr;
                (tokens, expr) = parse_for_expression(&tokens[1..])?;
                Ok((tokens, expr))
            }
            KeywordType::Use => {
                let expr;
                (tokens, expr) = parse_use_statement(&tokens[1..])?;
                Ok((tokens, Some(expr)))
            }
            _ => Err(parser_error(
                ParsingError::UnexpectedKeyword(keyword.clone()),
                &tokens[0],
            )),
        },

        // erroneous tokens
        [
            Token(
                _,
                token_type @ (TokenType::RParen
                | TokenType::Comma
                | TokenType::Plus
                | TokenType::Div
                | TokenType::Star
                | TokenType::Caret
                | TokenType::Eq
                | TokenType::EqEq
                | TokenType::NotEq
                | TokenType::Less
                | TokenType::LessEq
                | TokenType::Greater
                | TokenType::GreaterEq),
            ),
            ..,
        ] => Err(parser_error(
            ParsingError::UnexpectedToken(token_type.clone()),
            &tokens[0],
        )),

        // Parse an expression
        [
            Token(
                _,
                TokenType::LParen
                | TokenType::Identifier(_)
                | TokenType::Integer(_)
                | TokenType::Double(_)
                | TokenType::String(_)
                | TokenType::True
                | TokenType::False
                | TokenType::LBrace
                | TokenType::Minus
                | TokenType::Not,
            ),
            ..,
        ] => parse_expression(tokens),
        [..] => Err(parser_error_unexpected(&tokens[0])),
    }
}

pub(crate) fn parse(start: &[Token]) -> Result<ParsedAST, PositionalError> {
    let mut tokens = start;

    let expressions;
    (tokens, expressions) = parse_statements(tokens)?;

    assert!(tokens.is_empty());

    Ok(ParsedAST {
        m_expressions: expressions,
    })
}
