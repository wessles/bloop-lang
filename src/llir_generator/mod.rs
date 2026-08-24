// LLIR lowers `blir::blir_program::BLIRCompileUnit` into real LLVM IR via
// inkwell, so (unlike the sibling `blir` module) it needs the `llvm`
// feature -- gated at the `mod llir_generator;` declaration in lib.rs.
mod llir_compilation;
mod llir_convert_tests;
mod llir_execution_tests;
pub(super) mod llir_program;
mod test_support;
