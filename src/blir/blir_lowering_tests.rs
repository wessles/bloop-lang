#![cfg(test)]

use crate::blir::test_support::compile_program_str_to_ir;
use crate::blir::{
    BLIRArithOp, BLIRCmpOp, BLIRConstant, BLIRLabel, BLIROperation, BLIRRegister, BLIRType,
};
use std::assert_matches;
// ── strings & printing ────────────────────────────────────────────────────────

#[test]
fn test_string_constant_collected() {
    let ir = compile_program_str_to_ir(
        r#"
    fn main() { let x: string = "hello world"; print x; }
"#,
    )
    .unwrap();
    assert!(ir.m_string_consts.contains("hello world"));
}

#[test]
fn test_multiple_string_constants_collected() {
    let ir = compile_program_str_to_ir(
        r#"
    fn main() { let a: string = "foo"; let b: string = "bar"; print a; }
"#,
    )
    .unwrap();
    assert!(ir.m_string_consts.contains("foo"));
    assert!(ir.m_string_consts.contains("bar"));
}

#[test]
fn test_string_print_function_compiles() {
    let ir = compile_program_str_to_ir(
        r#"
    fn greet() { let msg: string = "hello"; print msg; }
"#,
    )
    .unwrap();
    assert_eq!(ir.m_functions.len(), 1);
    assert_eq!(ir.m_functions[0].m_name, "greet");
    assert!(ir.m_string_consts.contains("hello"));
}

#[test]
fn test_printing_strings() {
    let program = r#"
        fn main() {
            print "Hello world!";
            0
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    assert_eq!(ir.m_functions.len(), 1);
    let main_func = &ir.m_functions[0];
    assert_eq!(main_func.m_name, "main");
    assert_eq!(main_func.m_body.len(), 3);
    assert_eq!(
        main_func.m_body[0],
        BLIROperation::LoadConst(
            BLIRConstant::String("Hello world!".into()),
            BLIRRegister("main.print0.const.0".into(), BLIRType::Ptr)
        )
    );
    assert_eq!(
        main_func.m_body[1],
        BLIROperation::CallPrintf(BLIRRegister("main.print0.const.0".into(), BLIRType::Ptr))
    );
    assert_eq!(
        main_func.m_body[2],
        BLIROperation::LoadConst(
            BLIRConstant::Int64(0),
            BLIRRegister("main.return1.const.0".into(), BLIRType::Int64)
        )
    );
}

#[test]
fn test_printing_numbers() {
    let program = r#"
        fn main() {
            print 5;
            print 3.14;
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let main_func = &ir.m_functions[0];
    assert_eq!(
        main_func.m_body[1],
        BLIROperation::CallPrintf(BLIRRegister("main.print0.const.0".into(), BLIRType::Int64))
    );
    assert_eq!(
        main_func.m_body[3],
        BLIROperation::CallPrintf(BLIRRegister(
            "main.print1.const.0".into(),
            BLIRType::Float64
        ))
    );
}

// ── let statements ────────────────────────────────────────────────────────────

#[test]
fn test_tuple_destructure_let_from_non_literal_tuple_is_a_compile_error() {
    // No aggregate/struct type exists in BLIR yet, so tuple destructuring
    // isn't supported at all -- this must fail loudly at compile time, not
    // silently drop the let, regardless of what the RHS is.
    let program = r#"
        fn make_pair() {
            (1, 2)
        }
        fn main() {
            let (a, b) = make_pair();
            print a + b;
        }
    "#;
    let result = compile_program_str_to_ir(program);
    assert!(result.is_err());
}

#[test]
fn test_tuple_destructure_let_from_literal_tuple_is_a_compile_error() {
    // Tuples/structures are left for when BLIR gains general aggregate
    // support -- even destructuring a literal tuple RHS must fail loudly
    // at compile time, not silently drop the let (it used to succeed by
    // destructuring literal-tuple RHSes element-wise).
    let program = r#"
        fn main() {
            let (a, b) = (1, 2);
            print a + b;
        }
    "#;
    let result = compile_program_str_to_ir(program);
    assert!(result.is_err());
}

// ── exponent ──────────────────────────────────────────────────────────────────

#[test]
fn test_exponent_int_base_lowers_to_pow_ii() {
    let program = r#"
        fn main() {
            2 ^ 3
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let body = &ir.m_functions[0].m_body;
    assert_eq!(
        body,
        &vec![
            BLIROperation::LoadConst(
                BLIRConstant::Int64(2),
                BLIRRegister("main.return0.const.0".into(), BLIRType::Int64)
            ),
            BLIROperation::LoadConst(
                BLIRConstant::Int64(3),
                BLIRRegister("main.return0.const.1".into(), BLIRType::Int64)
            ),
            BLIROperation::PowII(
                BLIRRegister("main.return0.pow.2".into(), BLIRType::Int64),
                BLIRRegister("main.return0.const.0".into(), BLIRType::Int64),
                BLIRRegister("main.return0.const.1".into(), BLIRType::Int64),
            ),
        ]
    );
}

#[test]
fn test_exponent_float_base_lowers_to_pow_fi() {
    let program = r#"
        fn main() {
            let base: f64 = 2.0;
            base ^ 3
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let body = &ir.m_functions[0].m_body;
    assert_eq!(
        body[3],
        BLIROperation::LoadConst(
            BLIRConstant::Int64(3),
            BLIRRegister("main.return0.const.1".into(), BLIRType::Int64)
        )
    );
    assert_eq!(
        body[4],
        BLIROperation::PowFI(
            BLIRRegister("main.return0.pow.2".into(), BLIRType::Float64),
            BLIRRegister("main.return0.base.loaded.0".into(), BLIRType::Float64),
            BLIRRegister("main.return0.const.1".into(), BLIRType::Int64),
        )
    );
}

// ── unary operators ───────────────────────────────────────────────────────────

#[test]
fn test_unary_minus_int_lowers_to_zero_minus_operand() {
    let program = r#"
        fn main() {
            let x: i64 = 5;
            -x
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let body = &ir.m_functions[0].m_body;
    assert_eq!(
        body,
        &vec![
            BLIROperation::StackAllocVariable(BLIRRegister("x".into(), BLIRType::Int64)),
            BLIROperation::StoreFromConst(
                BLIRConstant::Int64(5),
                BLIRRegister("x".into(), BLIRType::Int64)
            ),
            BLIROperation::Load(
                BLIRRegister("x".into(), BLIRType::Int64),
                BLIRRegister("main.return0.x.loaded.0".into(), BLIRType::Int64)
            ),
            BLIROperation::LoadConst(
                BLIRConstant::Int64(0),
                BLIRRegister("main.return0.const.1".into(), BLIRType::Int64)
            ),
            BLIROperation::IArith(
                BLIRArithOp::Sub,
                BLIRRegister("main.return0.arith.2".into(), BLIRType::Int64),
                BLIRRegister("main.return0.const.1".into(), BLIRType::Int64),
                BLIRRegister("main.return0.x.loaded.0".into(), BLIRType::Int64),
            ),
        ]
    );
}

#[test]
fn test_unary_minus_float_uses_a_float_zero_constant() {
    // The zero constant subtracted from must match the operand's type, or
    // the IArith it feeds ends up with mismatched i64/f64 operands.
    let program = r#"
        fn main() {
            let x: f64 = 2.5;
            -x
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let body = &ir.m_functions[0].m_body;
    assert_eq!(
        body[3],
        BLIROperation::LoadConst(
            BLIRConstant::Float64(0.0),
            BLIRRegister("main.return0.const.1".into(), BLIRType::Float64)
        )
    );
    assert_eq!(
        body[4],
        BLIROperation::IArith(
            BLIRArithOp::Sub,
            BLIRRegister("main.return0.arith.2".into(), BLIRType::Float64),
            BLIRRegister("main.return0.const.1".into(), BLIRType::Float64),
            BLIRRegister("main.return0.x.loaded.0".into(), BLIRType::Float64),
        )
    );
}

// ── control flow: if / while ──────────────────────────────────────────────────

#[test]
fn test_if_statement_ir_ops() {
    let ir = compile_program_str_to_ir(
        r#"
    fn compare(x: i64, y: i64) {
        if(x < y) { print "less"; } else { print "greater than or equal to"; }
    }
"#,
    )
    .unwrap();
    assert_eq!(ir.m_functions.len(), 1);
    assert!(ir.m_string_consts.contains("less"));
    assert!(ir.m_string_consts.contains("greater than or equal to"));

    let body = &ir.m_functions[0].m_body;
    // Parameters are loaded into SSA names before use.
    assert_eq!(
        body[0],
        BLIROperation::Load(
            BLIRRegister("x".into(), BLIRType::Int64),
            BLIRRegister("compare.stmt0.x.loaded.0".into(), BLIRType::Int64)
        )
    );
    assert_eq!(
        body[1],
        BLIROperation::Load(
            BLIRRegister("y".into(), BLIRType::Int64),
            BLIRRegister("compare.stmt0.y.loaded.1".into(), BLIRType::Int64)
        )
    );
    assert_eq!(
        body[2],
        BLIROperation::ICmp(
            BLIRCmpOp::Lt,
            BLIRRegister("compare.stmt0.cmp.2".into(), BLIRType::Int64),
            BLIRRegister("compare.stmt0.x.loaded.0".into(), BLIRType::Int64),
            BLIRRegister("compare.stmt0.y.loaded.1".into(), BLIRType::Int64),
        )
    );
    assert_eq!(
        body[3],
        BLIROperation::CondBranch(
            BLIRRegister("compare.stmt0.cmp.2".into(), BLIRType::Int64),
            BLIRLabel("compare.stmt0.if3.false.1".into()),
            BLIRLabel("compare.stmt0.if3.true.0".into()),
        )
    );
    assert_eq!(
        body[4],
        BLIROperation::MarkLabel(BLIRLabel("compare.stmt0.if3.true.0".into()))
    );
    assert_eq!(
        body[5],
        BLIROperation::LoadConst(
            BLIRConstant::String("less".into()),
            BLIRRegister(
                "compare.stmt0.if3.then3.print0.const.0".into(),
                BLIRType::Ptr
            )
        )
    );
    assert_eq!(
        body[6],
        BLIROperation::CallPrintf(BLIRRegister(
            "compare.stmt0.if3.then3.print0.const.0".into(),
            BLIRType::Ptr
        ))
    );
    assert_eq!(
        body[7],
        BLIROperation::JumpToLabel(BLIRLabel("compare.stmt0.if3.end.2".into()))
    );
    assert_eq!(
        body[8],
        BLIROperation::MarkLabel(BLIRLabel("compare.stmt0.if3.false.1".into()))
    );
    assert_eq!(
        body[9],
        BLIROperation::LoadConst(
            BLIRConstant::String("greater than or equal to".into()),
            BLIRRegister(
                "compare.stmt0.if3.else4.print0.const.0".into(),
                BLIRType::Ptr
            )
        )
    );
    assert_eq!(
        body[10],
        BLIROperation::CallPrintf(BLIRRegister(
            "compare.stmt0.if3.else4.print0.const.0".into(),
            BLIRType::Ptr
        ))
    );
    assert_eq!(
        body[11],
        BLIROperation::JumpToLabel(BLIRLabel("compare.stmt0.if3.end.2".into()))
    );
    assert_eq!(
        body[12],
        BLIROperation::MarkLabel(BLIRLabel("compare.stmt0.if3.end.2".into()))
    );
}

#[test]
fn test_if_expr() {
    let program = r#"
        fn main() {
            if (1 < 2) {
                1
            } else {
                2
            }
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let main_func = &ir.m_functions[0];
    let body = &main_func.m_body;

    assert_eq!(
        body,
        &vec![
            BLIROperation::LoadConst(
                BLIRConstant::Int64(1),
                BLIRRegister("main.return0.const.0".into(), BLIRType::Int64)
            ),
            BLIROperation::LoadConst(
                BLIRConstant::Int64(2),
                BLIRRegister("main.return0.const.1".into(), BLIRType::Int64)
            ),
            BLIROperation::ICmp(
                BLIRCmpOp::Lt,
                BLIRRegister("main.return0.cmp.2".into(), BLIRType::Int64),
                BLIRRegister("main.return0.const.0".into(), BLIRType::Int64),
                BLIRRegister("main.return0.const.1".into(), BLIRType::Int64),
            ),
            BLIROperation::StackAllocVariable(BLIRRegister(
                "main.return0.if3.result.0".into(),
                BLIRType::Int64
            )),
            BLIROperation::CondBranch(
                BLIRRegister("main.return0.cmp.2".into(), BLIRType::Int64),
                BLIRLabel("main.return0.if3.false.2".into()),
                BLIRLabel("main.return0.if3.true.1".into()),
            ),
            BLIROperation::MarkLabel(BLIRLabel("main.return0.if3.true.1".into())),
            BLIROperation::LoadConst(
                BLIRConstant::Int64(1),
                BLIRRegister(
                    "main.return0.if3.then4.branch0.const.0".into(),
                    BLIRType::Int64
                )
            ),
            BLIROperation::StoreFromReg(
                BLIRRegister(
                    "main.return0.if3.then4.branch0.const.0".into(),
                    BLIRType::Int64
                ),
                BLIRRegister("main.return0.if3.result.0".into(), BLIRType::Int64),
            ),
            BLIROperation::JumpToLabel(BLIRLabel("main.return0.if3.end.3".into())),
            BLIROperation::MarkLabel(BLIRLabel("main.return0.if3.false.2".into())),
            BLIROperation::LoadConst(
                BLIRConstant::Int64(2),
                BLIRRegister(
                    "main.return0.if3.else5.branch0.const.0".into(),
                    BLIRType::Int64
                )
            ),
            BLIROperation::StoreFromReg(
                BLIRRegister(
                    "main.return0.if3.else5.branch0.const.0".into(),
                    BLIRType::Int64
                ),
                BLIRRegister("main.return0.if3.result.0".into(), BLIRType::Int64),
            ),
            BLIROperation::JumpToLabel(BLIRLabel("main.return0.if3.end.3".into())),
            BLIROperation::MarkLabel(BLIRLabel("main.return0.if3.end.3".into())),
            BLIROperation::Load(
                BLIRRegister("main.return0.if3.result.0".into(), BLIRType::Int64),
                BLIRRegister("main.return0.if3.result.loaded.6".into(), BLIRType::Int64),
            ),
        ]
    );
}

#[test]
fn test_while_loop() {
    let program = r#"
        fn main() {
            let x = 0;
            while(x < 5) {
                x = x + 1;
            }
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    assert_eq!(ir.m_functions.len(), 1);
    let main_func = &ir.m_functions[0];
    assert_eq!(main_func.m_name, "main");
    let body = &main_func.m_body;
    assert_eq!(body.len(), 15);
    // while preamble: jump to condition, mark condition label
    assert_eq!(
        body[2],
        BLIROperation::JumpToLabel(BLIRLabel("main.while0.cond_evaluate.0".into()))
    );
    assert_eq!(
        body[3],
        BLIROperation::MarkLabel(BLIRLabel("main.while0.cond_evaluate.0".into()))
    );
    // condition: load x, LoadConst(5), icmp, conditional branch
    assert_matches!(&body[6], BLIROperation::ICmp(BLIRCmpOp::Lt, _, _, _));
    assert_eq!(
        body[7],
        BLIROperation::CondBranch(
            BLIRRegister("main.while0.cond3.cmp.2".into(), BLIRType::Int64),
            BLIRLabel("main.while0.loop_end.2".into()),
            BLIRLabel("main.while0.loop_start.1".into()),
        )
    );
    assert_eq!(
        body[8],
        BLIROperation::MarkLabel(BLIRLabel("main.while0.loop_start.1".into()))
    );
    // back-edge jump and loop end label
    assert_eq!(
        body[13],
        BLIROperation::JumpToLabel(BLIRLabel("main.while0.cond_evaluate.0".into()))
    );
    assert_eq!(
        body[14],
        BLIROperation::MarkLabel(BLIRLabel("main.while0.loop_end.2".into()))
    );
}

#[test]
fn test_for_loop() {
    let program = r#"
        fn main() {
            for (let x = 0; x < 5; x = x + 1) {
                print x;
            }
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    assert_eq!(ir.m_functions.len(), 1);
    let main_func = &ir.m_functions[0];
    let body = &main_func.m_body;
    assert_eq!(body.len(), 17);
    // init: allocate + initialize x, once, before the loop
    assert_eq!(
        body[0],
        BLIROperation::StackAllocVariable(BLIRRegister("x".into(), BLIRType::Int64))
    );
    assert_eq!(
        body[1],
        BLIROperation::StoreFromConst(
            BLIRConstant::Int64(0),
            BLIRRegister("x".into(), BLIRType::Int64)
        )
    );
    // preamble: jump to condition, mark condition label
    assert_eq!(
        body[2],
        BLIROperation::JumpToLabel(BLIRLabel("main.for0.cond_evaluate.1".into()))
    );
    assert_eq!(
        body[3],
        BLIROperation::MarkLabel(BLIRLabel("main.for0.cond_evaluate.1".into()))
    );
    // condition and branch
    assert_matches!(&body[6], BLIROperation::ICmp(BLIRCmpOp::Lt, _, _, _));
    assert_eq!(
        body[7],
        BLIROperation::CondBranch(
            BLIRRegister("main.for0.cond4.cmp.2".into(), BLIRType::Int64),
            BLIRLabel("main.for0.loop_end.3".into()),
            BLIRLabel("main.for0.loop_start.2".into()),
        )
    );
    assert_eq!(
        body[8],
        BLIROperation::MarkLabel(BLIRLabel("main.for0.loop_start.2".into()))
    );
    // update runs after the body, before the back-edge jump
    assert_matches!(&body[13], BLIROperation::IArith(BLIRArithOp::Add, _, _, _));
    assert_eq!(
        body[14],
        BLIROperation::StoreFromReg(
            BLIRRegister("main.for0.update6.assign0.arith.2".into(), BLIRType::Int64),
            BLIRRegister("x".into(), BLIRType::Int64),
        )
    );
    assert_eq!(
        body[15],
        BLIROperation::JumpToLabel(BLIRLabel("main.for0.cond_evaluate.1".into()))
    );
    assert_eq!(
        body[16],
        BLIROperation::MarkLabel(BLIRLabel("main.for0.loop_end.3".into()))
    );
}

#[test]
fn test_for_loop_without_init_emits_no_extra_ops_before_the_preamble() {
    let program = r#"
        fn main() {
            let x: i64 = 0;
            for (; x < 5; x = x + 1) {
                print x;
            }
        }
    "#;
    let ir = compile_program_str_to_ir(program).unwrap();
    let body = &ir.m_functions[0].m_body;
    // Only the preceding top-level `let x` allocates/stores (body[0..2]);
    // the loop's own (absent) init contributes nothing before its preamble.
    assert_eq!(
        body[2],
        BLIROperation::JumpToLabel(BLIRLabel("main.for0.cond_evaluate.0".into()))
    );
}

// ── function calls ────────────────────────────────────────────────────────────

#[test]
fn test_function_call_emits_call_function_op() {
    let ir = compile_program_str_to_ir(
        r#"
        fn fortytwo() {
            42
        }

        fn main() {
            fortytwo()
        }
    "#,
    )
    .unwrap();
    assert_eq!(ir.m_functions.len(), 2);

    let fortytwo_func = &ir.m_functions[0];
    assert_eq!(fortytwo_func.m_name, "fortytwo");
    assert_eq!(fortytwo_func.m_body.len(), 1);
    assert_eq!(
        fortytwo_func.m_body[0],
        BLIROperation::LoadConst(
            BLIRConstant::Int64(42),
            BLIRRegister("fortytwo.return0.const.0".into(), BLIRType::Int64)
        )
    );

    let main_func = &ir.m_functions[1];
    assert_eq!(main_func.m_name, "main");
    assert_eq!(main_func.m_body.len(), 1);
    assert_eq!(
        main_func.m_body[0],
        BLIROperation::CallFunction(
            "fortytwo".into(),
            vec![],
            BLIRRegister("main.return0.call.0".into(), BLIRType::Int64)
        )
    );
}

#[test]
fn test_mutually_recursive_functions_both_lower_successfully() {
    // `is_even` calls `is_odd` before `is_odd` has been lowered (and vice
    // versa) -- every function must be known regardless of definition
    // order, since the LLVM backend pre-declares all of them up front.
    let ir = compile_program_str_to_ir(
        r#"
        fn is_even(n) {
            if (n == 0) { 1 } else { is_odd(n - 1) }
        }

        fn is_odd(n) {
            if (n == 0) { 0 } else { is_even(n - 1) }
        }

        fn main() {
            is_even(10)
        }
    "#,
    )
    .unwrap();

    assert_eq!(ir.m_functions.len(), 3);
    let calls_target = |body: &[BLIROperation], target: &str| {
        body.iter()
            .any(|op| matches!(op, BLIROperation::CallFunction(name, _, _) if name == target))
    };
    assert!(calls_target(&ir.m_functions[0].m_body, "is_odd"));
    assert!(calls_target(&ir.m_functions[1].m_body, "is_even"));
}

// ── modules ────────────────────────────────────────────────────────────────

#[test]
fn test_function_inside_module_lowers_with_qualified_name() {
    let ir = compile_program_str_to_ir(
        r#"
        mod foo {
            fn f() { 42 }
        }
        fn main() { foo::f() }
    "#,
    )
    .unwrap();
    assert_eq!(ir.m_functions.len(), 2);
    assert_eq!(ir.m_functions[0].m_name, "foo::f");
    assert_eq!(ir.m_functions[1].m_name, "main");
}

#[test]
fn test_function_call_to_module_function_uses_qualified_name_in_call_op() {
    let ir = compile_program_str_to_ir(
        r#"
        mod foo {
            fn f() { 42 }
        }
        fn main() { foo::f() }
    "#,
    )
    .unwrap();
    let main_func = &ir.m_functions[1];
    assert_eq!(main_func.m_name, "main");
    assert_eq!(
        main_func.m_body[0],
        BLIROperation::CallFunction(
            "foo::f".into(),
            vec![],
            BLIRRegister("main.return0.call.0".into(), BLIRType::Int64)
        )
    );
}

#[test]
fn test_use_brought_in_name_lowers_to_a_call_of_the_qualified_function() {
    // `use foo;` is fully resolved during type checking (the callee's
    // qualified name is baked into the AST), so BLIR lowering sees a plain
    // `foo::f` call here despite the source calling it unqualified.
    let ir = compile_program_str_to_ir(
        r#"
        mod foo {
            fn f() { 42 }
        }
        use foo;
        fn main() { f() }
    "#,
    )
    .unwrap();
    let main_func = &ir.m_functions[1];
    assert_eq!(main_func.m_name, "main");
    assert_eq!(
        main_func.m_body[0],
        BLIROperation::CallFunction(
            "foo::f".into(),
            vec![],
            BLIRRegister("main.return0.call.0".into(), BLIRType::Int64)
        )
    );
}

#[test]
fn test_mod_declaration_without_body_contributes_no_function() {
    // A bodiless `mod foo;` is purely a declaration -- it must not produce a
    // BLIR function or otherwise trip the "every top-level item is a
    // function definition" check.
    let ir = compile_program_str_to_ir(
        r#"
        mod foo;
        fn main() { 1 }
    "#,
    )
    .unwrap();
    assert_eq!(ir.m_functions.len(), 1);
    assert_eq!(ir.m_functions[0].m_name, "main");
}

#[test]
fn test_mutually_recursive_functions_inside_a_module_both_lower_successfully() {
    // Same guarantee as top-level mutual recursion (every function is known
    // regardless of definition order), but for two siblings inside the same
    // module, calling each other unqualified.
    let ir = compile_program_str_to_ir(
        r#"
        mod parity {
            fn is_even(n) {
                if (n == 0) { 1 } else { is_odd(n - 1) }
            }
            fn is_odd(n) {
                if (n == 0) { 0 } else { is_even(n - 1) }
            }
        }
        fn main() { parity::is_even(10) }
    "#,
    )
    .unwrap();

    assert_eq!(ir.m_functions.len(), 3);
    assert_eq!(ir.m_functions[0].m_name, "parity::is_even");
    assert_eq!(ir.m_functions[1].m_name, "parity::is_odd");
    let calls_target = |body: &[BLIROperation], target: &str| {
        body.iter()
            .any(|op| matches!(op, BLIROperation::CallFunction(name, _, _) if name == target))
    };
    assert!(calls_target(&ir.m_functions[0].m_body, "parity::is_odd"));
    assert!(calls_target(&ir.m_functions[1].m_body, "parity::is_even"));
}

#[test]
fn test_non_function_statement_inside_module_block_is_a_compile_error() {
    // The LLVM backend only supports function definitions (plus inert
    // mod/use statements) at top level; that restriction must apply
    // recursively inside a module block too, not just at the true top level.
    let program = r#"
        mod foo {
            let x = 5;
        }
    "#;
    let result = compile_program_str_to_ir(program);
    assert!(result.is_err());
}
