use crate::ast::tokens::{KeywordType, Token, TokenType};
use crate::positional_error::PositionalError;
use std::error::Error;
use std::fmt::Display;
use std::iter::Peekable;

#[derive(Debug, PartialEq)]
pub(super) enum TokenizeErrorType {
    UnterminatedString,
    UnterminatedBlockComment,
    InvalidNumber,
}
impl Display for TokenizeErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizeErrorType::UnterminatedString => write!(f, "Unterminated string"),
            TokenizeErrorType::UnterminatedBlockComment => {
                write!(f, "Unterminated block comment")
            }
            TokenizeErrorType::InvalidNumber => write!(f, "Invalid number"),
        }
    }
}
impl Error for TokenizeErrorType {}

struct TokenizeCharReader<I>
where
    I: Iterator<Item = char>,
{
    m_it: Peekable<I>,
    m_char_no: usize,
    m_line_no: usize,
}

impl<I> TokenizeCharReader<I>
where
    I: Iterator<Item = char>,
{
    fn next(&mut self) -> Option<char> {
        if let Some(c) = self.m_it.next() {
            self.m_char_no += 1;
            return Some(c);
        }
        None
    }

    fn peek(&mut self) -> Option<char> {
        if let Some(c) = self.m_it.peek() {
            return Some(*c);
        }
        None
    }

    fn err(&mut self, err_type: TokenizeErrorType) -> PositionalError {
        PositionalError::new(Box::new(err_type), self.m_line_no, self.m_char_no)
    }

    // Consumes chars looking for the closing `*/` of a block comment
    // already past its opening `/*`. Returns whether it found one on this
    // line -- a `false` means the comment runs past the end of the line and
    // the caller must keep skipping into the next one.
    fn skip_block_comment(&mut self) -> bool {
        let mut prev = None;
        while let Some(c) = self.next() {
            if prev == Some('*') && c == '/' {
                return true;
            }
            prev = Some(c);
        }
        false
    }
}

// A block comment can span multiple lines, so its open/closed state (and,
// while open, where it started -- for a graceful `UnterminatedBlockComment`
// rather than silently swallowing the rest of the file) has to survive
// across separate `read_tokens_from_line` calls, one per source line.
fn read_tokens_from_line(
    line: &str,
    line_no: usize,
    tokens: &mut Vec<Token>,
    in_block_comment: &mut Option<(usize, usize)>,
) -> Result<(), PositionalError> {
    let mut reader = TokenizeCharReader {
        m_it: line.chars().peekable(),
        m_char_no: 1, // these are 1 indexed when making links to files in debugger
        m_line_no: line_no,
    };

    if in_block_comment.is_some() {
        if reader.skip_block_comment() {
            *in_block_comment = None;
        } else {
            return Ok(());
        }
    }

    loop {
        let token_start_char = reader.m_char_no;
        let token_start_line = reader.m_line_no;

        macro_rules! push_token_and_continue {
            ($a:expr) => {
                let token = Token((token_start_line, token_start_char), $a);
                tokens.push(token);
                continue;
            };
        }

        let c = match reader.next() {
            Some(c) => c,
            None => break,
        };

        if c == '/' && reader.peek() == Some('*') {
            reader.next();
            if !reader.skip_block_comment() {
                *in_block_comment = Some((token_start_line, token_start_char));
                break;
            }
            continue;
        }

        if c == '/'
            && let Some(c) = reader.peek()
            && c == '/'
        {
            break;
        }

        if c.is_whitespace() {
            continue;
        }

        if let ('-', Some('>')) = (c, reader.peek()) {
            reader.next();
            push_token_and_continue!(TokenType::Arrow);
        }

        let token_type = match c {
            ':' => {
                if reader.peek() == Some(':') {
                    reader.next();
                    Some(TokenType::ColonColon)
                } else {
                    Some(TokenType::Colon)
                }
            }
            '(' => Some(TokenType::LParen),
            ')' => Some(TokenType::RParen),
            '+' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::PlusEq)
                } else {
                    Some(TokenType::Plus)
                }
            }
            '-' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::MinusEq)
                } else {
                    Some(TokenType::Minus)
                }
            }
            '/' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::DivEq)
                } else {
                    Some(TokenType::Div)
                }
            }
            '*' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::StarEq)
                } else {
                    Some(TokenType::Star)
                }
            }
            '^' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::CaretEq)
                } else {
                    Some(TokenType::Caret)
                }
            }
            '=' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::EqEq)
                } else {
                    Some(TokenType::Eq)
                }
            }
            '!' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::NotEq)
                } else {
                    Some(TokenType::Not)
                }
            }
            '<' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::LessEq)
                } else {
                    Some(TokenType::Less)
                }
            }
            '>' => {
                if reader.peek() == Some('=') {
                    reader.next();
                    Some(TokenType::GreaterEq)
                } else {
                    Some(TokenType::Greater)
                }
            }
            '&' if reader.peek() == Some('&') => {
                reader.next();
                Some(TokenType::AndAnd)
            }
            '|' if reader.peek() == Some('|') => {
                reader.next();
                Some(TokenType::OrOr)
            }
            ',' => Some(TokenType::Comma),
            ';' => Some(TokenType::Semicolon),
            '{' => Some(TokenType::LBrace),
            '}' => Some(TokenType::RBrace),
            '"' => {
                let mut string_token = String::new();
                loop {
                    if let Some(c) = reader.next() {
                        match c {
                            '"' => break,
                            '\\' => {
                                if let Some(escaped) = reader.next() {
                                    match escaped {
                                        'n' => string_token.push('\n'),
                                        't' => string_token.push('\t'),
                                        '\\' => string_token.push('\\'),
                                        '"' => string_token.push('"'),
                                        _ => string_token.push(escaped),
                                    }
                                } else {
                                    return Err(reader.err(TokenizeErrorType::UnterminatedString));
                                }
                            }
                            _ => string_token.push(c),
                        }
                    } else {
                        return Err(reader.err(TokenizeErrorType::UnterminatedString));
                    }
                }
                Some(TokenType::String(string_token))
            }
            _ => None,
        };
        if let Some(token_type) = token_type {
            push_token_and_continue!(token_type);
        }

        if c.is_alphabetic() || c == '_' {
            let mut token_str = String::new();
            token_str.push(c);
            while let Some(c) = reader.peek() {
                if c.is_alphanumeric() || c == '_' {
                    let c = reader.next().unwrap();
                    token_str.push(c);
                } else {
                    break;
                }
            }
            let token = match token_str.as_str() {
                "if" => TokenType::Keyword(KeywordType::If),
                "else" => TokenType::Keyword(KeywordType::Else),
                "while" => TokenType::Keyword(KeywordType::While),
                "for" => TokenType::Keyword(KeywordType::For),
                "fn" => TokenType::Keyword(KeywordType::Fn),
                "let" => TokenType::Keyword(KeywordType::Let),
                "print" => TokenType::Keyword(KeywordType::Print),
                "i64" => TokenType::Keyword(KeywordType::I64),
                "f64" => TokenType::Keyword(KeywordType::F64),
                "string" => TokenType::Keyword(KeywordType::String),
                "mod" => TokenType::Keyword(KeywordType::Mod),
                "use" => TokenType::Keyword(KeywordType::Use),
                "true" => TokenType::True,
                "false" => TokenType::False,
                _ => TokenType::Identifier(token_str),
            };
            push_token_and_continue!(token);
        }

        if c.is_numeric() || c == '.' {
            let mut dot_seen = c == '.';
            let mut number_str = String::new();
            number_str.push(c);
            while let Some(c) = reader.peek() {
                if c.is_numeric() {
                    let c = reader.next().unwrap();
                    number_str.push(c);
                } else if c == '.' {
                    number_str.push(c);
                    reader.next();
                    if !dot_seen {
                        dot_seen = true;
                    } else {
                        return Err(reader.err(TokenizeErrorType::InvalidNumber));
                    }
                } else if c.is_alphabetic() {
                    return Err(reader.err(TokenizeErrorType::InvalidNumber));
                } else {
                    break;
                }
            }
            if dot_seen {
                if let Ok(double) = number_str.parse() {
                    push_token_and_continue!(TokenType::Double(double));
                } else {
                    return Err(reader.err(TokenizeErrorType::InvalidNumber));
                }
            } else {
                if let Ok(integer) = number_str.parse() {
                    push_token_and_continue!(TokenType::Integer(integer));
                } else {
                    return Err(reader.err(TokenizeErrorType::InvalidNumber));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn tokenize<'a>(
    line_reader: impl Iterator<Item = &'a str> + Clone,
) -> Result<Vec<Token>, PositionalError> {
    let mut tokens: Vec<Token> = Vec::<Token>::new();
    let mut in_block_comment: Option<(usize, usize)> = None;
    for (line_idx, line) in line_reader.clone().enumerate() {
        read_tokens_from_line(line, line_idx + 1, &mut tokens, &mut in_block_comment)?;
    }
    if let Some((line, char)) = in_block_comment {
        return Err(PositionalError::new(
            Box::new(TokenizeErrorType::UnterminatedBlockComment),
            line,
            char,
        ));
    }
    let end_of_tokens_loc = match line_reader.enumerate().last() {
        Some((last_line, line_str)) => (last_line + 1, line_str.len() + 1),
        None => (1, 1),
    };
    tokens.push(Token(end_of_tokens_loc, TokenType::EndOfTokens));
    Ok(tokens)
}
