use crate::blir::bloop_library::BloopLibrary;
use crate::blir_interpreter::interpreter::BLIRInterpreter;
use wasm_bindgen::prelude::*;

/// Compiles one whole source file (must define `fn main()`) the same way
/// the native CLI's `-b` flag does, and returns its BLIR pretty-printed --
/// or, on failure at any front-end stage, the same positional error text
/// the CLI prints to stderr, as `Err` so JS can branch on success/failure.
#[wasm_bindgen]
pub fn compile_to_blir(source: &str) -> Result<String, String> {
    let library = lower(source)?;
    Ok(library.m_compile_unit.to_pretty_string(source))
}

/// Compiles and runs `main` the same way the native CLI's default
/// (interpreted) mode does, returning everything it `print`ed plus its
/// return value (if any) -- or, on failure at any stage (front-end or
/// runtime), the same error text the CLI prints to stderr.
#[wasm_bindgen]
pub fn run_program(source: &str) -> Result<String, String> {
    let library = lower(source)?;
    let mut interpreter = BLIRInterpreter::new();
    interpreter
        .link(crate::blir::bloop_stdlib::get_stdlib())
        .expect("a freshly constructed interpreter has nothing yet to clash with stdlib");
    interpreter.link(library).map_err(|e| format!("{:?}", e))?;
    let result = interpreter
        .execute_entry_point("main")
        .map_err(|e| format!("{:?}", e))?;
    let mut text = result.output;
    if let Some(value) = result.value {
        text.push_str(&format!("{:?}\n", value));
    }
    Ok(text)
}

/// The front-end stages shared by both entry points above: tokenize, parse,
/// type-check (against the stdlib's exported signatures, so calls into it
/// resolve), then lower to BLIR.
fn lower(source: &str) -> Result<BloopLibrary, String> {
    use crate::ast::AST;

    let stdlib_metadata = crate::blir::bloop_stdlib::get_stdlib().m_metadata;
    let program = AST::compile_src_with_externals(source.to_string(), stdlib_metadata)
        .map_err(|err| err.get_err(source, Some("main.blp")).to_string())?;
    BloopLibrary::new("main.blp", &program).map_err(|err| err.get_err(source, Some("main.blp")).to_string())
}
