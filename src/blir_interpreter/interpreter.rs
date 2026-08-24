use crate::blir::bloop_library::{BloopLibrary};
use crate::blir::{BLIRFunction, BLIROperation};
use crate::blir_interpreter::state::BLIRInterpreterState;
use crate::blir_interpreter::value::InterpValue;
use std::collections::{HashMap, HashSet};
use crate::ast::link_symbols::LinkSymbols;

#[derive(Debug, PartialEq)]
pub(crate) enum InterpretError {
    MissingEntryPoint(String),
    UndefinedFunction(String),
    DuplicateFunction(String),
    // A `let` inside a conditional branch that never ran type-checks fine
    // (BLIR only has function-level scoping) but never gets its register
    // bound, so a later read of it is a real, reachable runtime condition.
    UnboundRegister(String),
    DivisionByZero,
}

// Mirrors `ast::ExecutionResult`: the value of an entry point's implicit
// return (if any), plus everything `print`ed along the way -- buffered
// rather than written straight to stdout, same rationale as
// `ast::tree_evaluator::State`.
pub(crate) struct ExecutionResult {
    pub(crate) value: Option<InterpValue>,
    pub(crate) output: String,
}

/// Accumulates linked `BloopLibrary`s (stdlib, a compiled file, a REPL
/// submission's own freshly-compiled functions, ...) into one
/// interpreter-wide function table, so a single instance can run more than
/// one entry point over time -- each execution sees everything linked so
/// far, including from earlier executions.
pub(crate) struct BLIRInterpreter {
    m_libraries: Vec<BloopLibrary>,
    m_linked_names: HashSet<String>,
}

impl BLIRInterpreter {
    pub(crate) fn new() -> Self {
        Self {
            m_libraries: Vec::new(),
            m_linked_names: HashSet::new(),
        }
    }

    /// Merges `library`'s functions into this interpreter's linked state,
    /// reachable by every execution from now on -- including from a
    /// `CallFunction` inside an *earlier* linked library, since they all
    /// share the same function table once linked. Checks every name first,
    /// so a clash leaves the interpreter entirely untouched rather than
    /// partially linked.
    pub(crate) fn link(&mut self, library: BloopLibrary) -> Result<(), InterpretError> {
        for function in &library.m_compile_unit.m_functions {
            if self.m_linked_names.contains(&function.m_name) {
                return Err(InterpretError::DuplicateFunction(function.m_name.clone()));
            }
        }
        for function in &library.m_compile_unit.m_functions {
            self.m_linked_names.insert(function.m_name.clone());
        }
        self.m_libraries.push(library);
        Ok(())
    }

    /// The merged exported signatures of everything linked so far -- e.g.
    /// for type-checking a further unit (like the REPL's next submission)
    /// against everything already linked. `link`'s own clash check already
    /// guarantees no two linked libraries share a function name, so the
    /// merge here can never actually fail.
    pub(crate) fn linked_metadata(&self) -> LinkSymbols {
        LinkSymbols::merge_symbol_maps(self.m_libraries.iter().map(|library| &library.m_metadata))
            .expect("link() already guarantees linked libraries never share a function name")
    }

    /// Whether `name` is already a linked function -- e.g. so a caller that
    /// also tracks its own, separate namespace (like the REPL's `let`-bound
    /// variables) can check for a clash against this one before committing.
    pub(crate) fn is_linked(&self, name: &str) -> bool {
        self.m_linked_names.contains(name)
    }

    fn function_table(&self) -> HashMap<&str, &BLIRFunction> {
        self.m_libraries
            .iter()
            .flat_map(|library| library.m_compile_unit.m_functions.iter())
            .map(|function| (function.m_name.as_str(), function))
            .collect()
    }

    /// Runs whichever linked function is named `entry_point` (a fresh call,
    /// same as any other) and returns its value (if any) plus everything it
    /// `print`ed -- resolving calls against every function linked so far,
    /// from any library.
    pub(crate) fn execute_entry_point(
        &self,
        entry_point: &str,
    ) -> Result<ExecutionResult, InterpretError> {
        let functions = self.function_table();

        let entry = functions
            .get(entry_point)
            .copied()
            .ok_or_else(|| InterpretError::MissingEntryPoint(entry_point.to_string()))?;

        let mut output = String::new();
        let value = BLIRInterpreterState::call_function(entry, Vec::new(), &functions, &mut output)?;
        Ok(ExecutionResult { value, output })
    }

    /// Runs `ops` directly against `registers` (owned, handed in and back)
    /// rather than a fresh scope, resolving `CallFunction`s against
    /// everything linked so far. `registers` is always returned, even on
    /// failure, so the caller decides whether to keep a partial mutation --
    /// e.g. the REPL always discards it, treating one submission as
    /// all-or-nothing.
    ///
    /// Unlike `execute_entry_point`, `ops` need not belong to a linked
    /// function -- the REPL uses this to run each submission's bare
    /// statements directly against its own session-long registers, so a
    /// `let` from one submission stays bound in a later one.
    pub(crate) fn execute_ops(
        &self,
        ops: &[BLIROperation],
        registers: HashMap<String, InterpValue>,
        output: &mut String,
    ) -> (HashMap<String, InterpValue>, Result<(), InterpretError>) {
        let functions = self.function_table();
        let mut state = BLIRInterpreterState::with_registers(registers, &functions);
        let result = state.execute(ops, output);
        (state.into_registers(), result)
    }
}
