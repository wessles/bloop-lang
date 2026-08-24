// IR interpreter: executes a lowered `BLIRCompileUnit` directly, as an
// alternative to walking the AST (`ast::tree_evaluator`) or compiling
// through LLVM (`llir_generator::llir_compilation`). Only supports what
// BLIR itself can represent -- no aggregate type, no first-class functions
// (only `CallFunction` by static name) -- so tuples aren't reachable here.
pub(crate) mod interpreter;
mod interpreter_tests;
pub(crate) mod state;
mod test_support;
pub(crate) mod value;
