#![cfg(test)]

use crate::repl::repl_interpreter::Interpreter;
use crate::blir_interpreter::value::InterpValue;

#[test]
fn a_let_binding_is_visible_to_a_later_submission() {
    let mut repl = Interpreter::new();
    repl.interpret("let x = 5;").unwrap();
    let result = repl.interpret("x + 1").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(6)));
}

#[test]
fn a_function_defined_in_one_submission_is_callable_in_a_later_one() {
    let mut repl = Interpreter::new();
    repl.interpret("fn double(x: i64) { x * 2 }").unwrap();
    let result = repl.interpret("double(21)").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(42)));
}

#[test]
fn stdlib_is_available_without_declaring_it_locally() {
    // `use` only aliases within the submission that wrote it (each
    // submission type-checks independently -- see the module doc comment),
    // so `use std;` and its use need to be in the same one.
    let mut repl = Interpreter::new();
    let result = repl.interpret("use std; sqr(3.0)").unwrap();
    assert_eq!(result.value, Some(InterpValue::Float64(9.0)));
}

#[test]
fn a_qualified_stdlib_call_needs_no_use_and_works_across_submissions() {
    let mut repl = Interpreter::new();
    repl.interpret("let x = std::sqr(2.0);").unwrap();
    let result = repl.interpret("x").unwrap();
    assert_eq!(result.value, Some(InterpValue::Float64(4.0)));
}

#[test]
fn output_is_only_whats_new_since_the_last_submission() {
    let mut repl = Interpreter::new();
    let first = repl.interpret(r#"print "one";"#).unwrap();
    assert_eq!(first.output, "one\n");
    let second = repl.interpret(r#"print "two";"#).unwrap();
    assert_eq!(second.output, "two\n");
}

#[test]
fn a_let_or_print_submission_has_no_value() {
    let mut repl = Interpreter::new();
    let result = repl.interpret("let x = 5;").unwrap();
    assert_eq!(result.value, None);
}

#[test]
fn a_failed_submission_does_not_join_the_session() {
    let mut repl = Interpreter::new();
    repl.interpret("let x = 5;").unwrap();
    assert!(repl.interpret("x + y").is_err());
    // `y` was never bound, so this must still resolve to the state from
    // before the failed submission, not include a half-applied trace of it.
    let result = repl.interpret("x").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(5)));
}

#[test]
fn a_bare_expression_without_a_trailing_semicolon_does_not_break_later_submissions() {
    // Each submission is compiled independently (not concatenated with
    // prior source), so a value-query submission like `x + 1` (no trailing
    // `;`, same idiom as a block's own trailing return expression) has no
    // effect on how a later, separate submission parses.
    let mut repl = Interpreter::new();
    repl.interpret("let x = 5;").unwrap();
    repl.interpret("x + 1").unwrap();
    let result = repl.interpret("x + 2").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(7)));
}

#[test]
fn a_declaration_only_submission_has_no_value_even_after_an_earlier_value_producing_one() {
    // A `fn`/`mod`/`use` submission doesn't touch `main`'s trailing
    // expression, so it must not echo back a value left over from an
    // earlier, unrelated submission.
    let mut repl = Interpreter::new();
    let first = repl.interpret("std::sqr(5.0)").unwrap();
    assert_eq!(first.value, Some(InterpValue::Float64(25.0)));
    let second = repl.interpret("fn test(a: i64) { a + 1 }").unwrap();
    assert_eq!(second.value, None);
    let third = repl.interpret("use std;").unwrap();
    assert_eq!(third.value, None);
}

#[test]
fn a_user_defined_main_is_not_shadowed_by_the_repl_driver() {
    // The REPL's own per-submission driver function used to literally be
    // named `main`, which meant a REPL submission defining its own `fn
    // main() { .. }` would get silently clobbered by it (both ended up in
    // the same compile unit's function list under the same name). It now
    // uses a reserved name the tokenizer can never produce from real
    // source, so a user's own `main` survives untouched and is callable
    // like any other function.
    let mut repl = Interpreter::new();
    repl.interpret("fn main() { 99 }").unwrap();
    let result = repl.interpret("main()").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(99)));
}

#[test]
fn a_runtime_error_is_reported_and_does_not_join_the_session() {
    let mut repl = Interpreter::new();
    let err = repl.interpret("1 / 0").unwrap_err();
    assert!(err.contains("DivisionByZero"), "unexpected message: {}", err);
}

#[test]
fn redefining_a_function_in_a_later_submission_is_a_graceful_error() {
    let mut repl = Interpreter::new();
    repl.interpret("fn double(x: i64) { x * 2 }").unwrap();
    let err = repl.interpret("fn double(x: i64) { x * 3 }").unwrap_err();
    assert!(
        err.contains("DuplicateFunction") && err.contains("double"),
        "unexpected message: {}",
        err
    );
    // The failed redefinition didn't touch the original.
    let result = repl.interpret("double(10)").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(20)));
}

#[test]
fn rebinding_a_let_variable_in_a_later_submission_is_not_a_clash() {
    // Unlike function names, `let` rebinding a name (even under a new
    // type) across submissions is normal REPL redefinition, not an error --
    // `link`'s clash-checking is specifically about the linked function
    // table.
    let mut repl = Interpreter::new();
    repl.interpret("let x = 5;").unwrap();
    repl.interpret("let x = 5.5;").unwrap();
    let result = repl.interpret("x").unwrap();
    assert_eq!(result.value, Some(InterpValue::Float64(5.5)));
}

#[test]
fn a_function_linked_before_a_runtime_error_stays_linked() {
    // Defining a function and *calling* it are separate concerns -- a
    // submission that links a function but then fails at runtime while
    // running its own bare statements shouldn't undefine the function.
    let mut repl = Interpreter::new();
    let err = repl
        .interpret("fn helper(x: i64) { x + 1 } 1 / 0")
        .unwrap_err();
    assert!(err.contains("DivisionByZero"), "unexpected message: {}", err);
    let result = repl.interpret("helper(4)").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(5)));
}

#[test]
fn referencing_a_let_from_a_branch_that_never_ran_is_a_graceful_error() {
    // BLIR has no block-level scoping for a `let`, only function-level, so
    // this type-checks fine -- but the branch never running means the
    // variable's register never gets bound. Must be a graceful error, not a
    // panic.
    let mut repl = Interpreter::new();
    repl.interpret("if (1 < 0) { let x = 5; }").unwrap();
    let err = repl.interpret("x").unwrap_err();
    assert!(err.contains("UnboundRegister"), "unexpected message: {}", err);
}

#[test]
fn a_let_cannot_silently_shadow_an_already_linked_function() {
    let mut repl = Interpreter::new();
    repl.interpret("fn f() { 42 }").unwrap();
    let err = repl.interpret("let f = 10;").unwrap_err();
    assert!(err.contains('f'), "unexpected message: {}", err);
    // The failed `let` didn't join the session -- `f` is still the function.
    let result = repl.interpret("f()").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(42)));
}

#[test]
fn a_function_cannot_silently_shadow_an_already_bound_variable() {
    let mut repl = Interpreter::new();
    repl.interpret("let f = 10;").unwrap();
    let err = repl.interpret("fn f() { 42 }").unwrap_err();
    assert!(err.contains('f'), "unexpected message: {}", err);
    // The failed `fn` didn't join the session -- `f` is still the variable.
    let result = repl.interpret("f").unwrap();
    assert_eq!(result.value, Some(InterpValue::Int64(10)));
}
