#![warn(unreachable_pub)]

#[cfg(any(feature = "cli", feature = "wasm"))]
mod repl;
#[cfg(any(feature = "cli", feature = "wasm"))]
mod wasm;

mod ast;
#[cfg(feature = "cli")]
mod cli;
// Consumed by the native CLI's -b/-r/-c/-e flags and by the wasm REPL
// (`ast::interpreter`, `bloop_stdlib`) alike. Has no LLVM/inkwell
// dependency itself -- only the sibling `llir_generator` module (actual
// LLVM IR) needs that, on top of `cli`.
#[cfg(any(feature = "cli", feature = "wasm"))]
mod blir;
// Depends on `blir::blir_program`, so it shares that module's gating.
#[cfg(any(feature = "cli", feature = "wasm"))]
mod blir_interpreter;
#[cfg(all(feature = "cli", feature = "llvm"))]
mod llir_generator;
mod positional_error;

#[cfg(all(feature = "wasm", feature = "llvm"))]
compile_error!(
    "the `wasm` and `llvm` features are mutually exclusive: LLVM/inkwell cannot target wasm32"
);

#[cfg(feature = "cli")]
pub use cli::run_cli;
