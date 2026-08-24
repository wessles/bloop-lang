#![cfg(test)]

use crate::blir::test_support::compile_program_str_to_ir;
use crate::llir_generator::llir_program::LLIRProgram;

pub(super) fn compile_program_str_to_llir<'ctx>(
    context: &'ctx inkwell::context::Context,
    program: &str,
) -> LLIRProgram<'ctx> {
    let ir = compile_program_str_to_ir(program).unwrap();
    LLIRProgram::compile_from_blir(context, &ir).unwrap()
}
