use crate::repl::repl_interpreter::Interpreter;
use crate::repl::history::History;
use wasm_bindgen::prelude::*;

// Surfaces Rust panic messages (there are a handful of invariant-violation
// `unreachable!`/`panic!` paths in the evaluator) as `console.error` instead
// of an opaque "unreachable executed" WebAssembly trap with no diagnostic.
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// A single REPL session, mirroring the native terminal REPL's evaluator
/// core (see `terminal_repl.rs`) but driven by JS input instead of a
/// raw-mode TTY.
#[wasm_bindgen]
pub struct WasmRepl {
    interpreter: Interpreter,
    history: History,
}

#[wasm_bindgen]
impl WasmRepl {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmRepl {
        WasmRepl {
            interpreter: Interpreter::new(),
            history: History::new(),
        }
    }

    /// Evaluates one submitted line against the accumulated session state,
    /// recording it in history like the native REPL does even on failure.
    /// On success, returns buffered `print` output plus the trailing
    /// expression's value (suppressed if `()`), same as the native REPL
    /// would print. On failure, returns the same pretty positional-error
    /// text as `Err`, so JS can branch without parsing the string.
    pub fn eval(&mut self, line: &str) -> Result<String, String> {
        self.history.push(line.to_string());
        match self.interpreter.interpret(line) {
            Ok(result) => {
                let mut text = result.output;
                if let Some(value) = result.value {
                    text.push_str(&value.to_string());
                    text.push('\n');
                }
                Ok(text)
            }
            Err(err) => Err(err),
        }
    }

    /// Recalls an older submitted line (Up-arrow equivalent). Returns
    /// `undefined` to JS when there's no history to show.
    pub fn history_up(&mut self) -> Option<String> {
        self.history.up()
    }

    /// Recalls a newer submitted line, or clears back to live input once
    /// already at the newest one (Down-arrow equivalent). Returns
    /// `undefined` to JS only when there's no history at all.
    pub fn history_down(&mut self) -> Option<String> {
        self.history.down()
    }
}

impl Default for WasmRepl {
    fn default() -> Self {
        Self::new()
    }
}
