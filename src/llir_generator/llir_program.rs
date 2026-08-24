use crate::blir::BLIRCompileUnit;
use crate::llir_generator::llir_compilation::convert_blir_to_llir_module;
use inkwell::context::Context;
use inkwell::module::Module;
use std::error::Error;
use std::io::Write;

pub(crate) struct LLIRProgram<'ctx> {
    pub(crate) m_module: Module<'ctx>,
}

impl<'ctx> LLIRProgram<'ctx> {
    pub(crate) fn compile_from_blir(
        context: &'ctx Context,
        blir_program: &BLIRCompileUnit,
    ) -> Result<Self, Box<dyn Error>> {
        let module = convert_blir_to_llir_module(context, blir_program)?;
        Ok(LLIRProgram { m_module: module })
    }

    pub(crate) fn write_to(&self, writer: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let mem = self.m_module.print_to_string().to_string();
        writer.write_all(mem.as_bytes())?;
        Ok(())
    }
}
