#![cfg(test)]

use crate::blir::test_support::compile_program_str_to_ir;
use crate::blir::BLIRCompileUnit;
use crate::llir_generator::llir_program::LLIRProgram;
use crate::llir_generator::test_support::compile_program_str_to_llir;
use inkwell::context::Context;
use std::io::BufWriter;

fn compile_to_ir_text(program: &str) -> String {
    let llvm_context = Context::create();
    let ir_program_llvm = compile_program_str_to_llir(&llvm_context, program);

    let mut buf = Vec::new();
    {
        let mut writer = BufWriter::new(&mut buf);
        ir_program_llvm.write_to(&mut writer).unwrap();
    }
    String::from_utf8(buf).unwrap()
}

// ── LLVM IR text emission tests ───────────────────────────────────────────────

#[test]
fn test_llvm_ir_integer_variable() {
    let output = compile_to_ir_text(r#"fn main() { let x: i64 = 42; }"#);
    assert!(output.contains("define void @main()"));
    assert!(output.contains("%x = alloca i64"));
    assert!(output.contains("store i64 42, ptr %x"));
    assert!(output.contains("ret void"));
}

#[test]
fn test_llvm_ir_printf_declared() {
    let output = compile_to_ir_text(r#"fn greet() { let msg: string = "hello"; print msg; }"#);
    assert!(output.contains("declare i32 @printf(ptr"));
}

#[test]
fn test_llvm_ir_string_global_emitted() {
    let output = compile_to_ir_text(r#"fn greet() { let msg: string = "hello"; print msg; }"#);
    // A private unnamed_addr global should appear for the string constant.
    assert!(output.contains("private unnamed_addr constant"));
    assert!(output.contains("@str."));
}

#[test]
fn test_llvm_ir_string_print() {
    let output = compile_to_ir_text(r#"fn greet() { let msg: string = "hello"; print msg; }"#);
    assert!(output.contains("define void @greet()"));
    assert!(output.contains("%msg = alloca ptr"));
    assert!(output.contains("store ptr @str."));
    assert!(output.contains("ret void"));
}

#[test]
fn test_llvm_ir_integer_print() {
    let output = compile_to_ir_text(r#"fn main() { print 5; }"#);
    assert!(output.contains(r#"@fmt.int = private unnamed_addr constant [6 x i8] c"%lld\0A\00""#));
    assert!(output.contains("call i32 (ptr, ...) @printf(ptr @fmt.int, i64 5)"));
}

#[test]
fn test_llvm_ir_float_print() {
    let output = compile_to_ir_text(r#"fn main() { print 3.14; }"#);
    assert!(output.contains(r#"@fmt.float = private unnamed_addr constant [4 x i8] c"%f\0A\00""#));
    assert!(output.contains("call i32 (ptr, ...) @printf(ptr @fmt.float, double"));
}

#[test]
fn test_llvm_ir_function_call() {
    let output = compile_to_ir_text(
        r#"
        fn fortytwo() {
            42
        }

        fn main() {
            fortytwo()
        }
    "#,
    );
    assert!(output.contains("define i64 @fortytwo()"));
    assert!(output.contains("define i64 @main()"));
    assert!(output.contains("call i64 @fortytwo()"));
}

#[test]
fn test_llvm_ir_float_exponent_uses_the_powi_intrinsic() {
    // A float base routes through `llvm.powi` (see build_pow_intrinsic in
    // llir_compilation.rs) instead of a hand-rolled multiplication loop --
    // no int-power loop, just a declaration + call.
    let output = compile_to_ir_text(
        r#"
        fn main() {
            let base: f64 = 2.0;
            base ^ 3
        }
    "#,
    );
    assert!(output.contains("declare double @llvm.powi.f64.i32("));
    assert!(output.contains("call double @llvm.powi.f64.i32(double"));
    assert!(!output.contains(".mul = "));
}

#[test]
fn test_llvm_ir_integer_exponent_still_uses_a_multiplication_loop() {
    // No integer-only pow intrinsic exists in LLVM (llvm.powi/llvm.pow are
    // float-only), so an integer base must still go through the
    // hand-rolled loop in build_int_pow, unlike the float case above.
    let output = compile_to_ir_text("fn main() { 2 ^ 3 }");
    assert!(output.contains(".cond:"));
    assert!(output.contains(".mul = mul i64"));
    assert!(!output.contains("llvm.powi"));
}

#[test]
fn test_llvm_ir_if_branch() {
    let output = compile_to_ir_text(
        r#"
    fn compare(x: i64, y: i64) {
        if(x < y) { print "less"; } else { print "greater than or equal to"; }
    }
"#,
    );
    assert!(output.contains("define void @compare(i64 %x, i64 %y)"));
    assert!(output.contains("icmp slt i64 %x, %y"));
    assert!(output.contains(
        "br i1 %compare.stmt0.cmp.2, label %compare.stmt0.if3.true.0, label %compare.stmt0.if3.false.1"
    ));
    assert!(output.contains("compare.stmt0.if3.true.0:"));
    assert!(output.contains("compare.stmt0.if3.false.1:"));
    assert!(output.contains("compare.stmt0.if3.end.2:"));
    // then-block jumps to the end label, not falls through to else
    assert!(output.contains("br label %compare.stmt0.if3.end.2"));
}

// ── modules ────────────────────────────────────────────────────────────────

#[test]
fn test_llvm_ir_function_inside_module_uses_qualified_name() {
    let output = compile_to_ir_text(
        r#"
        mod foo {
            fn f() { 42 }
        }
        fn main() {
            foo::f()
        }
    "#,
    );
    // The module-qualified name appears in both the function's own
    // definition and at its call site in `main` -- LLVM identifiers with
    // special characters like `::` get quoted when printed, so this checks
    // for the name itself rather than assuming the exact quoting syntax.
    assert!(output.contains("foo::f"));
    assert!(output.contains("define i64 @main()"));
}

// ── regression tests ──────────────────────────────────────────────────────────

#[test]
fn test_unsupported_nested_expr_is_a_compile_error_not_a_silent_zero() {
    // `f`'s argument is a tuple literal, which `compile_expr_to_reg` doesn't
    // support. Previously this silently compiled `main` to return 0; now
    // it must surface as a real error out of `BLIRCompileUnit::new`.
    let program = r#"
        fn f(t) {
            1
        }

        fn main() {
            f((1, 2))
        }
    "#;
    let result = compile_program_str_to_ir(program);
    assert!(
        result.is_err(),
        "expected a compile error, got {:?}",
        result
    );
}

#[test]
fn test_missing_return_reg_is_an_error_not_a_panic_or_silent_zero() {
    // Directly construct a malformed BLIRCompileUnit (bypassing blir_lowering,
    // which should never produce this shape) to exercise the invariant
    // check in llir_compilation.rs's auto-return fallback.
    use crate::blir::{BLIRFunction, BLIRType};
    use std::collections::HashSet;

    let malformed = BLIRCompileUnit {
        m_name: "program".to_string(),
        m_string_consts: HashSet::new(),
        m_functions: vec![BLIRFunction {
            m_name: "main".to_string(),
            m_params: vec![],
            m_return: BLIRType::Int64,
            m_return_reg: None,
            m_body: vec![],
            m_locs: vec![],
        }],
    };

    let llvm_context = Context::create();
    let result = LLIRProgram::compile_from_blir(&llvm_context, &malformed);
    assert!(result.is_err(), "expected an error, got {:?}", result.err());
}

#[test]
fn test_calling_a_function_parameter_is_an_error_not_a_panic() {
    // `f` is a function-typed parameter of `apply`, not a `fn`-declared
    // top-level function, so no LLVM function named "f" is ever
    // declared. Calling it through `apply(double, 5)` used to panic
    // with `Option::unwrap()` on `None` in the CallFunction lowering;
    // it should be a normal (if "not yet supported") compile error.
    let program = r#"
        fn double(x) {
            x * 2
        }

        fn apply(f, x) {
            f(x)
        }

        fn main() {
            apply(double, 5)
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let llvm_context = Context::create();
    let result = LLIRProgram::compile_from_blir(&llvm_context, &ir);
    assert!(result.is_err(), "expected an error, got {:?}", result.err());
}
