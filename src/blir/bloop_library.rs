use crate::ast::link_symbols::LinkSymbols;
use crate::ast::TypedAST;
use crate::blir::BLIRCompileUnit;
use crate::positional_error::PositionalError;

pub(crate) struct BloopLibrary {
    pub(crate) m_compile_unit: BLIRCompileUnit,
    pub(crate) m_metadata: LinkSymbols,
}

impl BloopLibrary {
    pub(crate) fn new(name: &str, ast: &TypedAST) -> Result<BloopLibrary, PositionalError> {
        Ok(BloopLibrary {
            m_compile_unit: BLIRCompileUnit::new(name.to_string(), &ast)?,
            m_metadata: LinkSymbols::new(ast),
        })
    }
}
