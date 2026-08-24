#![cfg(test)]

use crate::llir_generator::test_support::compile_program_str_to_llir;
use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::OptimizationLevel;
use std::cell::RefCell;
use std::ffi::{c_char, c_int, CStr};

fn compile_and_run_i64(program: &str) -> i64 {
    let context = Context::create();
    let llir = compile_program_str_to_llir(&context, program);
    let engine = llir
        .m_module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();
    type MainFn = unsafe extern "C" fn() -> i64;
    let f: JitFunction<MainFn> = unsafe { engine.get_function("main") }.unwrap();
    unsafe { f.call() }
}

fn compile_and_run_f64(program: &str) -> f64 {
    let context = Context::create();
    let llir = compile_program_str_to_llir(&context, program);
    let engine = llir
        .m_module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();
    type MainFn = unsafe extern "C" fn() -> f64;
    let f: JitFunction<MainFn> = unsafe { engine.get_function("main") }.unwrap();
    unsafe { f.call() }
}

thread_local! {
    static CAPTURED_PRINTF_OUTPUT: RefCell<String> = const { RefCell::new(String::new()) };
}

// Stands in for libc's printf when the JIT is told (via add_global_mapping)
// to resolve calls to it here instead. Only handles the single-argument
// shape (format string, no variadic args) that a plain `print "literal";`
// compiles to — good enough to observe what a JIT-executed program actually
// printed without touching the real process stdout.
unsafe extern "C" fn capture_printf(fmt: *const c_char) -> c_int {
    let text = unsafe { CStr::from_ptr(fmt) }
        .to_string_lossy()
        .into_owned();
    CAPTURED_PRINTF_OUTPUT.with(|buf| buf.borrow_mut().push_str(&text));
    0
}

fn compile_and_run_capturing_stdout(program: &str) -> String {
    CAPTURED_PRINTF_OUTPUT.with(|buf| buf.borrow_mut().clear());

    let context = Context::create();
    let llir = compile_program_str_to_llir(&context, program);
    let engine = llir
        .m_module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();

    let printf_fn = llir.m_module.get_function("printf").unwrap();
    engine.add_global_mapping(&printf_fn, capture_printf as *const () as usize);

    type MainFn = unsafe extern "C" fn();
    let f: JitFunction<MainFn> = unsafe { engine.get_function("main") }.unwrap();
    unsafe { f.call() };

    CAPTURED_PRINTF_OUTPUT.with(|buf| buf.borrow().clone())
}

// ── JIT execution tests ───────────────────────────────────────────────────────

#[test]
fn test_while_loop_counts_to_five() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let x = 0;
            while(x < 5) {
                x = x + 1;
            }
            x
        }
    "#,
    );
    assert_eq!(result, 5);
}

#[test]
fn test_fib_while_loop() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let i = 0;
            let x = 1;
            let y = 1;
            while(i < 5) {
                let z = x + y;
                x = y;
                y = z;
                i = i + 1;
            }
            y
        }
    "#,
    );
    assert_eq!(result, 13);
}

#[test]
fn test_function_dispatch() {
    let result = compile_and_run_i64(
        r#"
            fn fortytwo() {
                42
            }

            fn main() {
                fortytwo()
            }
        "#,
    );
    assert_eq!(result, 42);
}

#[test]
fn test_mutually_recursive_functions() {
    // Two top-level functions calling each other (not self-recursion) --
    // exercises that every function is pre-declared before any body is
    // compiled, so a forward reference to a not-yet-compiled callee still
    // resolves.
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
    let result = compile_and_run_i64(program);
    assert_eq!(result, 1);
}

#[test]
fn test_three_way_mutual_recursion() {
    let program = r#"
        fn a(n) {
            if (n == 0) { 1 } else { b(n - 1) }
        }

        fn b(n) {
            if (n == 0) { 2 } else { c(n - 1) }
        }

        fn c(n) {
            if (n == 0) { 3 } else { a(n - 1) }
        }

        fn main() {
            a(10)
        }
    "#;
    let result = compile_and_run_i64(program);
    assert_eq!(result, 2);
}

#[test]
fn test_fib_recurse() {
    let program = r#"
        fn fib(n) {
            if (n < 2) {
                1
            } else {
                fib(n - 1) + fib(n - 2)
            }
        }

        fn main() {
            fib(6)
        }
    "#;
    let result = compile_and_run_i64(program);
    assert_eq!(result, 13);
}

#[test]
fn test_print_hello_world_writes_to_stdout() {
    let output = compile_and_run_capturing_stdout(
        r#"
        fn main() {
            print "hello world";
        }
    "#,
    );
    assert_eq!(output, "hello world\n");
}

// `print`ing a number produces a variadic printf call, which this
// JIT-execution test file can't observe in-process: Apple's arm64 ABI
// passes variadic args on the stack, so a plain extra parameter on
// capture_printf reads garbage, and Rust's `c_variadic` is nightly-only.
// See tests/print_output.rs, which checks the real printed text by
// running the actual `bloop -e` binary in a separate process instead.

// ── modules ────────────────────────────────────────────────────────────────

#[test]
fn test_function_inside_module_is_callable_via_qualified_path() {
    let result = compile_and_run_i64(
        r#"
        mod foo {
            fn f(x) { x + 1 }
        }
        fn main() {
            foo::f(41)
        }
    "#,
    );
    assert_eq!(result, 42);
}

#[test]
fn test_sibling_function_in_same_module_resolves_unqualified() {
    let result = compile_and_run_i64(
        r#"
        mod outer {
            fn double(x) { x * 2 }
            fn quadruple(x) { double(double(x)) }
        }
        fn main() {
            outer::quadruple(5)
        }
    "#,
    );
    assert_eq!(result, 20);
}

#[test]
fn test_mutually_recursive_functions_inside_a_module() {
    let program = r#"
        mod parity {
            fn is_even(n) {
                if (n == 0) { 1 } else { is_odd(n - 1) }
            }
            fn is_odd(n) {
                if (n == 0) { 0 } else { is_even(n - 1) }
            }
        }
        fn main() {
            parity::is_even(10)
        }
    "#;
    let result = compile_and_run_i64(program);
    assert_eq!(result, 1);
}

// ── regression tests ──────────────────────────────────────────────────────────

#[test]
fn test_nested_if_as_value() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            if (1 < 2) {
                if (2 < 3) {
                    1
                } else {
                    2
                }
            } else {
                3
            }
        }
    "#,
    );
    assert_eq!(result, 1);
}

#[test]
fn test_if_without_else_as_statement() {
    // A bare `if cond { .. }` with no `else` is valid syntax that
    // type-checks to Unit. `compile_expr_to_reg`'s `If` arm used to
    // unconditionally unwrap the (optional) else branch, panicking
    // with "entered unreachable code" on this exact pattern.
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let x = 1;
            if (x < 2) {
                x = 5;
            }
            x
        }
    "#,
    );
    assert_eq!(result, 5);
}

#[test]
fn test_float_arithmetic_does_not_panic() {
    // IArith's LLVM lowering used to unconditionally treat every operand as
    // an integer (`.into_int_value()`), so any f64 +-*/ crashed with
    // "Found FloatValue but expected the IntValue variant".
    let result = compile_and_run_f64(
        r#"
        fn main() {
            let x: f64 = 2.5;
            let y: f64 = 1.5;
            x + y
        }
    "#,
    );
    assert_eq!(result, 4.0);
}

#[test]
fn test_float_comparison_does_not_panic() {
    // ICmp's LLVM lowering had the same integer-only assumption as IArith,
    // so comparing two f64 values also crashed.
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let x: f64 = 2.5;
            let y: f64 = 1.5;
            if (x > y) { 1 } else { 0 }
        }
    "#,
    );
    assert_eq!(result, 1);
}

#[test]
fn test_unary_minus_int() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let x: i64 = 5;
            -x
        }
    "#,
    );
    assert_eq!(result, -5);
}

#[test]
fn test_unary_minus_float() {
    let result = compile_and_run_f64(
        r#"
        fn main() {
            let x: f64 = 2.5;
            -x
        }
    "#,
    );
    assert_eq!(result, -2.5);
}

#[test]
fn test_for_loop_basic() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let sum: i64 = 0;
            for (let i = 0; i < 5; i = i + 1) {
                sum = sum + i;
            }
            sum
        }
    "#,
    );
    assert_eq!(result, 10);
}

#[test]
fn test_for_loop_with_optional_init_and_assignment_init() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let i: i64 = 100;
            let total: i64 = 0;
            for (i = 0; i < 3; i = i + 1) {
                total = total + 1;
            }
            for (; total < 10; total = total + 1) {
            }
            total
        }
    "#,
    );
    assert_eq!(result, 10);
}

#[test]
fn test_exponent_integer_positive_power() {
    let result = compile_and_run_i64("fn main() { 2 ^ 3 }");
    assert_eq!(result, 8);
}

#[test]
fn test_exponent_integer_zero_power() {
    let result = compile_and_run_i64("fn main() { 2 ^ 0 }");
    assert_eq!(result, 1);
}

#[test]
fn test_exponent_integer_negative_power() {
    let result = compile_and_run_i64(
        r#"
        fn main() {
            let zero: i64 = 0;
            2 ^ (zero - 2)
        }
    "#,
    );
    // Integer division truncates: 1 / 4 == 0, matching the evaluator.
    assert_eq!(result, 0);
}

#[test]
fn test_exponent_float_positive_and_negative_power() {
    let pos = compile_and_run_f64(
        r#"
        fn main() {
            let base: f64 = 2.0;
            base ^ 3
        }
    "#,
    );
    assert_eq!(pos, 8.0);

    let neg = compile_and_run_f64(
        r#"
        fn main() {
            let base: f64 = 2.0;
            let zero: i64 = 0;
            base ^ (zero - 2)
        }
    "#,
    );
    assert_eq!(neg, 0.25);
}

#[test]
fn test_exponent_operator_precedence() {
    let tight = compile_and_run_i64("fn main() { 1 + 2 ^ 2 }");
    assert_eq!(tight, 5);
    let grouped = compile_and_run_i64("fn main() { (1 + 2) ^ 2 }");
    assert_eq!(grouped, 9);
}

#[test]
fn test_elseless_if_nested_in_value_producing_if() {
    // An else-less `if` used as a non-tail statement inside a
    // value-producing `if`'s branch. This exercises the same bug via a
    // second path (nested inside a Block that itself must produce a
    // value for the outer if).
    let result = compile_and_run_i64(
        r#"
        fn main() {
            if (1 < 2) {
                if (3 < 4) {
                    1
                };
                5
            } else {
                6
            }
        }
    "#,
    );
    assert_eq!(result, 5);
}
