use crate::ast::expressions::{Expression, TypedExpression};
use crate::ast::types::Type;
use crate::ast::TypedAST;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

#[derive(Default)]
pub(crate) struct LinkSymbols {
    pub(crate) m_map: HashMap<String, Type>,
}

impl LinkSymbols {
    pub(crate) fn new(ast: &TypedAST) -> LinkSymbols {
        fn collect(exprs: &[TypedExpression], out: &mut HashMap<String, Type>) {
            for expr in exprs {
                match &expr.m_expression {
                    Expression::FunctionDefinition(Some(name), _, _) => {
                        out.insert(name.clone(), expr.m_info.clone());
                    }
                    Expression::Module(_, Some(body)) => collect(body, out),
                    _ => {}
                }
            }
        }
        let mut out = HashMap::new();
        collect(&ast.m_expressions, &mut out);
        LinkSymbols {
            m_map: out
        }
    }

    pub(crate) fn empty() -> LinkSymbols {
        Default::default()
    }

    pub(crate) fn merge_symbol_maps<'a>(
        symbol_maps: impl IntoIterator<Item = &'a LinkSymbols>,
    ) -> Result<LinkSymbols, LinkSymbolsMergeError> {
        let mut merged = HashMap::new();
        for symbol_map in symbol_maps {
            for (symbol, ty) in &symbol_map.m_map {
                if merged.contains_key(symbol) {
                    return Err(LinkSymbolsMergeError(symbol.clone()));
                }
                merged.insert(symbol.clone(), ty.clone());
            }
        }
        Ok(LinkSymbols { m_map: merged })
    }
}

pub(crate) struct LinkSymbolsMergeError(String);

impl Debug for LinkSymbolsMergeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for LinkSymbolsMergeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for LinkSymbolsMergeError {}
