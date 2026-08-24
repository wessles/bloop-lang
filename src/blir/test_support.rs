#![cfg(test)]

use crate::ast::link_symbols::LinkSymbols;
use crate::ast::types::Type;
use crate::ast::TypedAST;
use crate::blir::BLIRCompileUnit;
use crate::positional_error::PositionalError;
use std::collections::HashMap;

pub(crate) fn compile_program_str_to_ir(program: &str) -> Result<BLIRCompileUnit, PositionalError> {
    let src = program.to_string();
    let ast = match TypedAST::compile_src(src) {
        Ok(ast) => ast,
        Err(err) => panic!("{}", err.get_err(program, Some("program"))),
    };
    BLIRCompileUnit::new("program".to_string(), &ast)
}

/// Like `compile_program_str_to_ir`, but type-checks `program` against
/// `externals` (e.g. another already-compiled unit's exported signatures)
/// instead of assuming a standalone program. Lets callers outside `blir/`
/// (e.g. `blir_interpreter`'s tests) build a `BLIRCompileUnit` with external
/// symbols in scope without reaching into `ast::TypedAST` themselves.
pub(crate) fn compile_program_str_to_ir_with_externals(
    program: &str,
    externals: LinkSymbols,
) -> Result<BLIRCompileUnit, PositionalError> {
    let ast = TypedAST::compile_src_with_externals(program.to_string(), externals)?;
    BLIRCompileUnit::new("program".to_string(), &ast)
}

/// Builds a `LinkSymbols` naming one external `(i64, ...) -> i64`
/// function's signature -- e.g. for a test exercising a call to an unlinked
/// external, without that test needing to name `ast::types::Type` itself.
pub(crate) fn external_int_function_symbol(
    qualified_name: &str,
    param_count: usize,
) -> LinkSymbols {
    LinkSymbols {
        m_map: HashMap::from([(
            qualified_name.to_string(),
            Type::Function(vec![Type::I64; param_count], Box::new(Type::I64)),
        )]),
    }
}
