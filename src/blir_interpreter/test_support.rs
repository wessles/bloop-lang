#![cfg(test)]

//! Shared "source -> BLIR" compile helper for the `blir_interpreter`
//! module's tests, mirroring `blir::test_support`.

use crate::blir::BLIRCompileUnit;
use crate::blir::test_support::compile_program_str_to_ir_with_externals;

pub(crate) fn compile_program_str_to_ir(program: &str) -> BLIRCompileUnit {
    let stdlib_metadata = crate::blir::bloop_stdlib::get_stdlib().m_metadata;
    compile_program_str_to_ir_with_externals(program, stdlib_metadata)
        .unwrap_or_else(|err| panic!("{}", err.get_err(program, Some("program"))))
}
