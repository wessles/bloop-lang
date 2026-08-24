//! REPL session wrapper: runs each incrementally-submitted snippet through
//! the same front-end -> BLIR -> `BLIRInterpreter` pipeline `cli.rs` uses
//! for whole files, without concatenating it with prior source.
//!
//! `fn`/`mod`/`use` items go straight into the session's `BLIRInterpreter`
//! via `link()` (redefining one under an already-linked name is a hard
//! error). Everything else (`let`s, bare expressions, `print`s) is wrapped
//! in a throwaway function -- purely to reuse `blir_lowering` -- that's
//! never linked or called; its ops run directly against the session's own
//! long-lived registers instead, the same way a `let` stays bound for the
//! rest of an ordinary function body. That works because a `let`-bound
//! variable's register is named after the variable itself rather than
//! qualified by an enclosing function (see `BLIRInterpreterState`'s doc
//! comment), so successive submissions sharing one register map is enough
//! to keep a `let` from an earlier submission visible in a later one.
//!
//! Type-checking a submission needs both every linked function
//! (`BLIRInterpreter::linked_metadata`) and every bound top-level
//! variable's type (`m_variable_types`) as externals. Both are only
//! committed once a submission fully succeeds, so a failed submission
//! never joins the session -- except its own function declarations, which
//! stay linked even if its bare statements then fail at runtime.

use crate::ast::expressions::{visit_expression, Expression, ParsedExpression, TypedExpression};
use crate::ast::tokens::tokenizer;
use crate::ast::types::Type;
use crate::ast::{parser, type_checker, ParsedAST};
use crate::blir::bloop_library::BloopLibrary;
use crate::blir_interpreter::interpreter::BLIRInterpreter;
use crate::blir_interpreter::value::InterpValue;
use std::collections::HashMap;
use crate::ast::link_symbols::LinkSymbols;
use crate::blir::bloop_stdlib;

pub(crate) struct Interpreter {
    m_engine: BLIRInterpreter,
    m_registers: HashMap<String, InterpValue>,
    // Top-level `let`-bound variable name -> its type, across every
    // submission so far -- the type checker's own externals mechanism only
    // carries function signatures (`BLIRInterpreter::linked_metadata`), so
    // this is the parallel piece needed to let a later submission reference
    // an earlier one's variable without re-declaring it.
    m_variable_types: HashMap<String, Type>,
    // Every submission's throwaway driver function needs a name distinct
    // from every other submission's (see the module doc comment for why it
    // must never collide with a *linked* name at all) -- BLIR's temporary
    // register names are qualified by their enclosing function's name, so
    // two submissions reusing the same driver name would silently collide
    // on any temporary they happen to need in the same relative position.
    m_next_entry_id: usize,
}

/// The outcome of interpreting one REPL submission: the value of its last
/// expression (`None` for a `let`/`print`/void-returning submission), plus
/// any text buffered by `print` statements evaluated during this call (not
/// the whole session's output -- just what's new).
#[derive(Debug)]
pub(crate) struct InterpretResult {
    pub(crate) value: Option<InterpValue>,
    pub(crate) output: String,
}

impl Interpreter {
    pub(crate) fn new() -> Self {
        Self {
            m_engine: {
                let mut engine = BLIRInterpreter::new();
                engine
                    .link(bloop_stdlib::get_stdlib())
                    .expect("a freshly constructed interpreter has nothing yet to clash with stdlib");
                engine
            },
            m_registers: HashMap::new(),
            m_variable_types: HashMap::new(),
            m_next_entry_id: 0,
        }
    }

    /// Compiles and runs `line` against everything linked/bound by earlier
    /// submissions. On success, `line`'s own functions and variables join
    /// the session permanently; on any failure the session is left exactly
    /// as it was, so the user can correct and resubmit. The error is
    /// rendered to a plain string (rather than a `PositionalError`) since a
    /// runtime `InterpretError` has no source location to render against.
    pub(crate) fn interpret(&mut self, line: &str) -> Result<InterpretResult, String> {
        let tokens = tokenizer::tokenize(line.split('\n'))
            .map_err(|e| e.get_err(line, Some("repl")).to_string())?;
        let parsed_ast =
            parser::parse(&tokens).map_err(|e| e.get_err(line, Some("repl")).to_string())?;

        let entry_name = format!("$repl_{}", self.m_next_entry_id);
        self.m_next_entry_id += 1;

        let synthetic_ast = Self::wrap_bare_statements_in_entry_point(parsed_ast, entry_name.clone());

        let mut externals = self.m_engine.linked_metadata().m_map;
        externals.extend(self.m_variable_types.iter().map(|(k, v)| (k.clone(), v.clone())));

        let typed_ast = type_checker::type_check(synthetic_ast, LinkSymbols { m_map: externals })
            .map_err(|e| e.get_err(line, Some("repl")).to_string())?;

        // Read off the newly-(re)bound top-level variables' types from the
        // driver function's own (typed, substituted) body before it's
        // consumed by lowering -- committed to `self.m_variable_types` only
        // once this whole submission succeeds, alongside everything else.
        let mut new_variable_types = self.m_variable_types.clone();
        if let Some(Expression::FunctionDefinition(_, _, body)) = typed_ast
            .m_expressions
            .iter()
            .map(|expr| &expr.m_expression)
            .find(|expr| matches!(expr, Expression::FunctionDefinition(Some(name), _, _) if name == &entry_name))
        {
            Self::collect_let_bindings(body, &mut new_variable_types);
        }

        let mut library = BloopLibrary::new(&entry_name, &typed_ast)
            .map_err(|e| e.get_err(line, Some("repl")).to_string())?;

        // Pull the driver function back out before linking -- it's a
        // compilation vehicle only (see the module doc comment): it's
        // never linked (so it's not reachable via `CallFunction`) and never
        // called via `call_function` either (so running it doesn't get its
        // own fresh scope that would discard any `let` bindings when it
        // returns).
        let driver_index = library
            .m_compile_unit
            .m_functions
            .iter()
            .position(|f| f.m_name == entry_name)
            .expect("we always define exactly one function under this submission's entry name");
        let driver = library.m_compile_unit.m_functions.remove(driver_index);
        library.m_metadata.m_map.remove(&entry_name);

        // A variable and a function sharing a name is as much a clash as
        // two functions sharing one, which `link` already refuses -- so
        // refuse it here too, before touching any persistent state.
        let clash = new_variable_types.keys().find(|name| {
            self.m_engine.is_linked(name)
                || library
                    .m_compile_unit
                    .m_functions
                    .iter()
                    .any(|f| &f.m_name == *name)
        });
        if let Some(name) = clash {
            return Err(format!(
                "`{}` is already defined as a function -- a variable can't reuse its name",
                name
            ));
        }

        self.m_engine.link(library).map_err(|e| format!("{:?}", e))?;

        let mut output = String::new();
        let (registers, result) =
            self.m_engine
                .execute_ops(&driver.m_body, self.m_registers.clone(), &mut output);
        result.map_err(|e| format!("{:?}", e))?;

        self.m_registers = registers;
        self.m_variable_types = new_variable_types;

        let value = driver
            .m_return_reg
            .as_ref()
            .and_then(|reg| self.m_registers.get(reg.as_str()).cloned());

        Ok(InterpretResult { value, output })
    }

    fn is_top_level_declaration(expr: &ParsedExpression) -> bool {
        matches!(
            expr.m_expression,
            Expression::FunctionDefinition(..)
                | Expression::Module(..)
                | Expression::UseStatement(..)
        )
    }

    /// `fn`/`mod`/`use` items pass through to the top level unchanged (BLIR
    /// lowering only accepts those there, and they're what `interpret` then
    /// links); everything else is collected, in original order, into a
    /// trailing driver function under `entry_name` so BLIR always sees a
    /// well-formed program to lower.
    fn wrap_bare_statements_in_entry_point(ast: ParsedAST, entry_name: String) -> ParsedAST {
        let mut declarations = Vec::new();
        let mut statements = Vec::new();
        for expr in ast.m_expressions {
            if Self::is_top_level_declaration(&expr) {
                declarations.push(expr);
            } else {
                statements.push(expr);
            }
        }
        declarations.push(ParsedExpression {
            m_expression: Expression::FunctionDefinition(Some(entry_name), Vec::new(), statements),
            m_location: (1, 1),
            m_info: (),
        });
        ParsedAST {
            m_expressions: declarations,
        }
    }

    /// Walks every `let x = ..` / `let (a, b) = ..` anywhere in `body`
    /// (including nested inside an `if`/`while`/`for`/block -- BLIR has no
    /// block-level scoping for a `let`-bound variable, only function-level,
    /// so a nested one is exactly as session-persistent as a top-level one
    /// once its ops run against the session's own registers), recording
    /// each bound name's inferred type into `out`.
    fn collect_let_bindings(body: &[TypedExpression], out: &mut HashMap<String, Type>) {
        for stmt in body {
            // The callback never itself errors, so this can't fail.
            let _ = visit_expression(stmt, &mut |expr| {
                if let Expression::LetStatement(lhs, _, _) = &expr.m_expression {
                    match &lhs.m_expression {
                        Expression::Variable(name) => {
                            out.insert(name.clone(), lhs.m_info.clone());
                        }
                        Expression::Tuple(vars) => {
                            for var in vars {
                                if let Expression::Variable(name) = &var.m_expression {
                                    out.insert(name.clone(), var.m_info.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            });
        }
    }
}
