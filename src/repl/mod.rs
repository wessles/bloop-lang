pub(crate) mod history;
#[cfg(feature = "cli")]
pub(crate) mod terminal_repl;
// The REPL session driver: re-parses/type-checks/lowers/interprets the
// accumulated session on every submission via `blir`/`blir_interpreter`, so
// it's only available where those are (native CLI or wasm).
#[cfg(any(feature = "cli", feature = "wasm"))]
pub(crate) mod repl_interpreter;
#[cfg(any(feature = "cli", feature = "wasm"))]
mod repl_interpreter_tests;
