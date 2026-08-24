#![cfg(test)]

use crate::blir::bloop_library::BloopLibrary;
use crate::blir_interpreter::interpreter::{BLIRInterpreter, InterpretError};
use crate::blir_interpreter::test_support::compile_program_str_to_ir;
use crate::blir_interpreter::value::InterpValue;
use crate::ast::link_symbols::LinkSymbols;
use crate::blir::test_support::{compile_program_str_to_ir_with_externals, external_int_function_symbol};

// Wraps a bare `BLIRCompileUnit` (no cross-unit exported-signature metadata
// needed for these tests) as a linkable library.
fn as_library(m_compile_unit: crate::blir::BLIRCompileUnit) -> BloopLibrary {
    BloopLibrary {
        m_compile_unit,
        m_metadata: LinkSymbols::empty(),
    }
}

fn linked(program: &str) -> BLIRInterpreter {
    let mut interpreter = BLIRInterpreter::new();
    interpreter
        .link(as_library(compile_program_str_to_ir(program)))
        .unwrap();
    interpreter
}

fn run(program: &str) -> Result<Option<InterpValue>, InterpretError> {
    linked(program).execute_entry_point("main").map(|r| r.value)
}

fn run_output(program: &str) -> String {
    linked(program).execute_entry_point("main").unwrap().output
}

#[test]
fn executes_a_constant_returning_main() {
    assert_eq!(run("fn main() { 1 }"), Ok(Some(InterpValue::Int64(1))));
}

#[test]
fn arithmetic_follows_precedence() {
    assert_eq!(
        run("fn main() { 2 + 3 * 4 }"),
        Ok(Some(InterpValue::Int64(14)))
    );
}

#[test]
fn let_binding_and_variable_reference() {
    assert_eq!(
        run("fn main() { let x = 5; x + 1 }"),
        Ok(Some(InterpValue::Int64(6)))
    );
}

#[test]
fn if_as_value() {
    assert_eq!(
        run("fn main() { if (1 < 2) { 10 } else { 20 } }"),
        Ok(Some(InterpValue::Int64(10)))
    );
    assert_eq!(
        run("fn main() { if (1 > 2) { 10 } else { 20 } }"),
        Ok(Some(InterpValue::Int64(20)))
    );
}

#[test]
fn else_if_chain_as_value() {
    let program = r#"
        fn classify(x) {
            if (x < 0) { -1 } else if (x == 0) { 0 } else { 1 }
        }
        fn main() {
            classify(-5) * 100 + classify(0) * 10 + classify(5)
        }
    "#;
    assert_eq!(run(program), Ok(Some(InterpValue::Int64(-99))));
}

#[test]
fn else_if_chain_as_statement_with_no_trailing_else() {
    let program = r#"
        fn main() {
            let x = 2;
            if (x == 1) {
                print "one";
            } else if (x == 2) {
                print "two";
            } else if (x == 3) {
                print "three";
            }
        }
    "#;
    assert_eq!(run_output(program), "two\n");
}

#[test]
fn comparison_result_is_usable_as_an_int() {
    assert_eq!(run("fn main() { 3 > 2 }"), Ok(Some(InterpValue::Int64(1))));
    assert_eq!(run("fn main() { 3 < 2 }"), Ok(Some(InterpValue::Int64(0))));
}

#[test]
fn unary_not() {
    assert_eq!(
        run("fn main() { !true }"),
        Ok(Some(InterpValue::Int64(0)))
    );
    assert_eq!(
        run("fn main() { !false }"),
        Ok(Some(InterpValue::Int64(1)))
    );
    assert_eq!(
        run("fn main() { !!true }"),
        Ok(Some(InterpValue::Int64(1)))
    );
    assert_eq!(
        run("fn main() { !(1 < 2) }"),
        Ok(Some(InterpValue::Int64(0)))
    );
}

#[test]
fn unary_minus() {
    assert_eq!(run("fn main() { -5 }"), Ok(Some(InterpValue::Int64(-5))));
}

#[test]
fn while_loop_with_mutation() {
    let program = r#"
        fn main() {
            let x = 0;
            while(x < 5) {
                x = x + 1;
            }
            x
        }
    "#;
    assert_eq!(run(program), Ok(Some(InterpValue::Int64(5))));
}

#[test]
fn for_loop_basic() {
    let program = r#"
        fn main() {
            let sum: i64 = 0;
            for (let i = 0; i < 5; i = i + 1) {
                sum = sum + i;
            }
            sum
        }
    "#;
    assert_eq!(run(program), Ok(Some(InterpValue::Int64(10))));
}

#[test]
fn direct_recursion() {
    let program = r#"
        fn fact(n) {
            if (n <= 1) { 1 } else { n * fact(n - 1) }
        }
        fn main() {
            fact(5)
        }
    "#;
    assert_eq!(run(program), Ok(Some(InterpValue::Int64(120))));
}

#[test]
fn mutual_recursion() {
    let program = r#"
        fn is_even(n) {
            if (n == 0) { 1 } else { is_odd(n - 1) }
        }
        fn is_odd(n) {
            if (n == 0) { 0 } else { is_even(n - 1) }
        }
        fn main() {
            is_even(10)
        }
    "#;
    assert_eq!(run(program), Ok(Some(InterpValue::Int64(1))));
}

#[test]
fn sibling_function_in_module_resolves_unqualified() {
    let program = r#"
        mod outer {
            fn double(x) { x * 2 }
            fn quadruple(x) { double(double(x)) }
        }
        fn main() {
            outer::quadruple(5)
        }
    "#;
    assert_eq!(run(program), Ok(Some(InterpValue::Int64(20))));
}

#[test]
fn float_exponent_positive_and_negative_power() {
    let pos = r#"
        fn main() {
            let base: f64 = 2.0;
            base ^ 3
        }
    "#;
    assert_eq!(run(pos), Ok(Some(InterpValue::Float64(8.0))));

    let neg = r#"
        fn main() {
            let base: f64 = 2.0;
            let zero: i64 = 0;
            base ^ (zero - 2)
        }
    "#;
    assert_eq!(run(neg), Ok(Some(InterpValue::Float64(0.25))));
}

#[test]
fn integer_exponent_negative_power_truncates() {
    assert_eq!(
        run("fn main() { 2 ^ (0 - 1) }"),
        Ok(Some(InterpValue::Int64(0)))
    );
}

#[test]
fn integer_division_by_zero_is_a_graceful_error() {
    assert_eq!(
        run("fn main() { 1 / 0 }"),
        Err(InterpretError::DivisionByZero)
    );
}

#[test]
fn zero_to_a_negative_power_is_a_graceful_error() {
    assert_eq!(
        run("fn main() { 0 ^ (0 - 1) }"),
        Err(InterpretError::DivisionByZero)
    );
}

#[test]
fn print_string_writes_raw_text_plus_newline() {
    assert_eq!(
        run_output(r#"fn main() { print "hello world"; }"#),
        "hello world\n"
    );
}

#[test]
fn print_int_and_float() {
    assert_eq!(run_output("fn main() { print 42; }"), "42\n");
    assert_eq!(run_output("fn main() { print 1.5; }"), "1.500000\n");
}

#[test]
fn calling_an_unlinked_external_function_is_a_graceful_error() {
    let program = r#"
        use extern_mod;
        fn main() {
            mystery(5)
        }
    "#;
    let externals = external_int_function_symbol("extern_mod::mystery", 1);
    let ir = compile_program_str_to_ir_with_externals(program, externals).unwrap();
    let mut interpreter = BLIRInterpreter::new();
    interpreter.link(as_library(ir)).unwrap();
    assert_eq!(
        interpreter.execute_entry_point("main").map(|r| r.value),
        Err(InterpretError::UndefinedFunction(
            "extern_mod::mystery".to_string()
        ))
    );
}

#[test]
fn stdlib_functions_are_linkable_and_callable() {
    // Unlike `linked()`/`run()`, this test specifically exercises linking a
    // second, already-compiled library (stdlib) and calling into it -- so it
    // reaches for the real `bloop_stdlib` rather than the empty-metadata
    // `as_library` stub the other tests use.
    let program = r#"
        use std;
        fn main() {
            sqr(3.0)
        }
    "#;
    let mut interpreter = BLIRInterpreter::new();
    interpreter.link(crate::blir::bloop_stdlib::get_stdlib()).unwrap();
    interpreter
        .link(as_library(compile_program_str_to_ir(program)))
        .unwrap();
    assert_eq!(
        interpreter.execute_entry_point("main").map(|r| r.value),
        Ok(Some(InterpValue::Float64(9.0)))
    );
}

#[test]
fn linking_a_duplicate_function_name_is_a_graceful_error() {
    let mut interpreter = BLIRInterpreter::new();
    interpreter
        .link(as_library(compile_program_str_to_ir(
            "fn main() { 1 }",
        )))
        .unwrap();
    let err = interpreter
        .link(as_library(compile_program_str_to_ir(
            "fn main() { 2 }",
        )))
        .unwrap_err();
    assert_eq!(err, InterpretError::DuplicateFunction("main".to_string()));
    // The failed link left the interpreter untouched -- the first `main`
    // is still the one that runs.
    assert_eq!(
        interpreter.execute_entry_point("main").map(|r| r.value),
        Ok(Some(InterpValue::Int64(1)))
    );
}

#[test]
fn logical_and_or_evaluate_truth_tables() {
    assert_eq!(
        run("fn main() { true && true }"),
        Ok(Some(InterpValue::Int64(1)))
    );
    assert_eq!(
        run("fn main() { true && false }"),
        Ok(Some(InterpValue::Int64(0)))
    );
    assert_eq!(
        run("fn main() { false || true }"),
        Ok(Some(InterpValue::Int64(1)))
    );
    assert_eq!(
        run("fn main() { false || false }"),
        Ok(Some(InterpValue::Int64(0)))
    );
}

#[test]
fn logical_and_or_short_circuit_the_never_taken_side() {
    // If `&&`/`||` weren't short-circuiting, the untaken side would still
    // run and this would panic on a bound-check-free division by zero via
    // `1 / 0` -- instead it should never even be evaluated.
    let and_program = r#"
        fn main() {
            let x: i64 = 0;
            false && ((1 / x) > 0)
        }
    "#;
    assert_eq!(run(and_program), Ok(Some(InterpValue::Int64(0))));

    let or_program = r#"
        fn main() {
            let x: i64 = 0;
            true || ((1 / x) > 0)
        }
    "#;
    assert_eq!(run(or_program), Ok(Some(InterpValue::Int64(1))));
}

#[test]
fn referencing_an_unbound_register_is_a_graceful_error_not_a_panic() {
    // BLIR has no block-level scoping for a `let`, only function-level, so
    // this type-checks fine -- but the branch never running means the
    // variable's register never gets bound.
    assert_eq!(
        run("fn main() { if (1 < 0) { let x = 5; } x }"),
        Err(InterpretError::UnboundRegister("x".to_string()))
    );
}
