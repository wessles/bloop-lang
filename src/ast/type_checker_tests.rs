#![cfg(test)]

use crate::ast::link_symbols::LinkSymbols;
use crate::ast::operators::Operator;
use crate::ast::type_checker::TypeCheckerError;
use crate::ast::types::Type;
use crate::positional_error::PositionalError;
use std::assert_matches;
use std::ops::Deref;

fn type_check_str(program: &str) -> Result<Type, PositionalError> {
    let lines = program.lines();
    let tokens = crate::ast::tokens::tokenizer::tokenize(lines)?;
    let ast = crate::ast::parser::parse(tokens.as_slice())?;
    let extern_symbols = LinkSymbols::empty();
    let ast_checked = crate::ast::type_checker::type_check(ast, extern_symbols)?;
    Ok(ast_checked.m_expressions.last().unwrap().m_info.clone())
}
fn type_check_str_ok(program: &str) -> Type {
    type_check_str(program)
        .unwrap_or_else(|e| panic!("Expected Ok, got Err: {}", e.get_err(program, None)))
}

fn type_check_str_error(program: &str) -> TypeCheckerError {
    match type_check_str(program) {
        Ok(_) => panic!("Expected Err, got Ok"),
        Err(e) => {
            let generic_err = e.m_error;
            let typecheck_err = generic_err.downcast::<TypeCheckerError>().unwrap();
            *typecheck_err
        }
    }
}

// ── inference tests (happy path) ────────────────────────────────────────────────

#[test]
fn test_let_inference() {
    let program = "let x = 1; x";
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_let_inference_bool() {
    let program = "let x = false; x";
    assert_matches!(type_check_str_ok(program), Type::Boolean);
}

#[test]
fn test_let_inference_float_expr() {
    let program = "let y = 1.0; let z = 0.5 + y; z";
    assert_matches!(type_check_str_ok(program), Type::F64);
}

#[test]
fn test_let_inference_string() {
    let program = r#"let w = "test"; w"#;
    assert_matches!(type_check_str_ok(program), Type::String);
}

#[test]
fn test_let_inference_bool_literal() {
    let program = "let v = false; v";
    assert_matches!(type_check_str_ok(program), Type::Boolean);
}

#[test]
fn test_let_inference_function() {
    let program = "let compare = fn(x: i64, y: i64) { true }; compare";
    assert_matches!(type_check_str_ok(program), Type::Function(_, _));
}

#[test]
fn test_let_inference_tuple() {
    let program = r#"let tuple = (1, 2.5, "hey"); tuple"#;
    assert_matches!(
        type_check_str_ok(program),
        Type::Tuple(ref ts) if matches!(ts.as_slice(), [Type::I64, Type::F64, Type::String])
    );
}

#[test]
fn test_let_inference_tuple_binding() {
    let program = r#"let (x, y, z) = (1, 2.5, "hey"); (x, y, z)"#;
    assert_matches!(
        type_check_str_ok(program),
        Type::Tuple(ref ts) if matches!(ts.as_slice(), [Type::I64, Type::F64, Type::String])
    );
}

#[test]
fn test_binary_op_inference() {
    let program = r#"let x = 5; let y = 4;
    (
        x == y,
        x != y,
        x <= y,
        x < y,
        x >= y,
        x > y,

        x + y,
        x - y,
        x / y,
        x * y,
    )"#;
    assert_matches!(
    type_check_str_ok(program),
    Type::Tuple(ref ts) if matches!(ts.as_slice(), [
        Type::Boolean, Type::Boolean, Type::Boolean, Type::Boolean, Type::Boolean, Type::Boolean,
        Type::I64, Type::I64, Type::I64, Type::I64
    ]));
}

#[test]
fn test_function_param_and_return_inference() {
    let program = r#"let compare = fn(x, y) { if(y < 5.0) { x < 5 } else { x > 5 } }; compare"#;
    let result = type_check_str_ok(program);
    if let Type::Function(param_types, return_type) = result {
        assert_matches!(param_types.as_slice(), [Type::I64, Type::F64]);
        assert_matches!(return_type.deref(), Type::Boolean);
    } else {
        panic!("Expected a function type, but got {:?}", result);
    }
}

// ── error tests ──────────────────────────────────────────────────────────────

#[test]
fn test_infinite_recursive_type() {
    let program = "fn test(x) { test }";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::CircularTypes(_, _)
    );
}

#[test]
fn test_unification_fail() {
    // annotated as i64 but assigned a bool
    let program = "let x: i64 = true";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::UnificationFail(Type::I64, Type::Boolean)
    );
}

#[test]
fn test_unknown_identifier() {
    let program = "undefined_var";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::IdentifierNotFound(ref name) if name == "undefined_var"
    );
}

#[test]
fn test_arity_mismatch() {
    // f takes one argument but is called with two
    let program = "fn f(a: i64) { a }; f(1, 2)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::ArityMismatch(1, 2)
    );
}

#[test]
fn test_calling_a_non_function_value_is_rejected() {
    let program = "let x = 5; x(1)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::UnificationFail(Type::I64, Type::Function(_, _))
    );
}

#[test]
fn test_tuple_length_mismatch() {
    // destructuring a 3-tuple into 2 bindings
    let program = "let (a, b) = (1, 2, 3)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::TupleLengthMismatch(2, 3)
    );
}

#[test]
fn test_tuple_destructure_duplicate_names_is_rejected() {
    let program = "let (a, a) = (1, 2.0)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::DuplicateTupleBinding(ref name) if name == "a"
    );
}

#[test]
fn test_string_equality_is_rejected() {
    // Only primitive types are comparable; strings are arrays and can't be
    // compared without a loop the LLVM backend doesn't support yet.
    let program = r#""hello" == "hello""#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::UncomparableType(Type::String)
    );
}

#[test]
fn test_exponent_base_must_be_numeric() {
    // `^`'s left-hand side must be i64 or f64; a string base is rejected.
    let program = r#""hello" ^ 2"#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::InvalidExponentBase(Type::String)
    );
}

#[test]
fn test_exponent_power_must_be_integer() {
    // `^`'s right-hand side must be i64; a float power is rejected.
    let program = "2 ^ 2.0";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::UnificationFail(Type::I64, Type::F64)
    );
}

#[test]
fn test_unary_minus_operand_must_be_numeric() {
    // Unary `-`'s operand must be i64 or f64; a boolean is rejected.
    let program = "-true";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::InvalidUnaryOperand(Operator::Minus, Type::Boolean)
    );
}

#[test]
fn test_unary_not_operand_must_be_boolean() {
    // Unary `!`'s operand must be a bool; an i64 is rejected.
    let program = "!5";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::InvalidUnaryOperand(Operator::Not, Type::I64)
    );
}

#[test]
fn test_unary_not_negates_a_boolean() {
    assert_eq!(type_check_str_ok("!true"), Type::Boolean);
}

// ── regression tests ──────────────────────────────────────────────────────────

#[test]
fn test_uncalled_function_with_unconstrained_param_is_ambiguous() {
    // `identity`'s parameter type is never unified against anything since
    // it's never called, so there's no way to determine a concrete type
    // for it. This should be a normal compile error, not a panic.
    let program = "fn identity(x) { x }";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::AmbiguousType(_)
    );
}

#[test]
fn test_assigning_to_non_variable_is_rejected() {
    let program = "1 = 2";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::InvalidAssignmentTarget
    );
}

#[test]
fn test_tuple_destructure_with_non_variable_element_is_rejected() {
    let program = "let (1, 2) = (3, 4)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::InvalidAssignmentTarget
    );
}

#[test]
fn test_anonymous_function_cannot_recurse_via_its_let_binding() {
    // Unlike `fn name(..) {..}`, a `let name = fn(..) {..}` does not put
    // `name` in scope for its own body. Supporting that would mean the
    // function captures a binding from its enclosing scope -- i.e. a
    // closure -- which this language does not support: functions compile
    // down to plain, non-capturing LLVM functions.
    let program = r#"
        let fact = fn(n: i64) {
            if (n < 2) { 1 } else { n * fact(n - 1) }
        };
        fact(5)
    "#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::IdentifierNotFound(ref name) if name == "fact"
    );
}

// ── module tests ────────────────────────────────────────────────────────────

#[test]
fn test_mod_declaration_without_body_is_a_noop() {
    let program = "mod foo; 1";
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_mod_block_itself_types_as_unit() {
    let program = "mod foo { fn f(x: i64) { x } }";
    assert_matches!(type_check_str_ok(program), Type::Unit);
}

#[test]
fn test_function_inside_module_is_callable_via_qualified_path() {
    let program = "mod foo { fn f(x: i64) { x } } foo::f(5)";
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_sibling_function_in_same_module_resolves_unqualified() {
    // A reference inside a module resolves against its own module first,
    // Rust-style, so a sibling can be called by its short name.
    let program = r#"
        mod outer {
            fn double(x: i64) { x * 2 }
            fn quadruple(x: i64) { double(double(x)) }
        }
        outer::quadruple(5)
    "#;
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_same_function_name_in_different_modules_does_not_collide() {
    let program = r#"
        mod a { fn f(x: i64) { x + 1 } }
        mod b { fn f(x: i64) { x + 2 } }
        (a::f(1), b::f(1))
    "#;
    assert_matches!(
        type_check_str_ok(program),
        Type::Tuple(ref ts) if matches!(ts.as_slice(), [Type::I64, Type::I64])
    );
}

#[test]
fn test_module_function_unqualified_reference_from_outside_module_is_rejected() {
    // Unlike inside the module, an unqualified reference from outside must
    // fail: `f` on its own is never in scope at the top level just because
    // some module happens to define it.
    let program = "mod foo { fn f(x: i64) { x } } f(5)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::IdentifierNotFound(ref name) if name == "f"
    );
}

#[test]
fn test_module_function_wrong_qualified_path_is_rejected() {
    let program = "mod foo { fn f(x: i64) { x } } foo::bar::f(5)";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::IdentifierNotFound(ref name) if name == "foo::bar::f"
    );
}

#[test]
fn test_mutual_recursion_between_module_level_functions() {
    // Module-level functions are hoisted the same way top-level ones are,
    // so an earlier sibling can call one defined later in the same module.
    let program = r#"
        mod parity {
            fn is_even(n: i64) {
                if (n == 0) { true } else { is_odd(n - 1) }
            }
            fn is_odd(n: i64) {
                if (n == 0) { false } else { is_even(n - 1) }
            }
        }
        parity::is_even(10)
    "#;
    assert_matches!(type_check_str_ok(program), Type::Boolean);
}

#[test]
fn test_use_of_a_nonexistent_module_is_rejected() {
    let program = "use definitely_not_a_real_module; 1";
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::UnknownModule(ref path) if path == "definitely_not_a_real_module"
    );
}

#[test]
fn test_two_uses_with_a_clashing_member_is_ambiguous() {
    // Rust-style: bringing the same short name into scope from two
    // different modules is an error at the second `use`, not silent
    // last-one-wins shadowing.
    let program = r#"
        mod a { fn f(x: i64) { x + 1 } }
        mod b { fn f(x: i64) { x + 2 } }
        use a;
        use b;
        f(1)
    "#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::DuplicateUseBinding(ref name, ref first, ref second)
            if name == "f" && first == "a::f" && second == "b::f"
    );
}

#[test]
fn test_use_colliding_with_an_existing_top_level_function_is_ambiguous() {
    let program = r#"
        fn f(x: i64) { x }
        mod a { fn f(x: i64) { x } }
        use a;
        f(1)
    "#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::DuplicateUseBinding(ref name, ref first, ref second)
            if name == "f" && first == "f" && second == "a::f"
    );
}

#[test]
fn test_use_colliding_with_a_function_defined_later_is_still_ambiguous() {
    // Top-level functions are hoisted before any `use` is processed, so the
    // collision is caught regardless of whether the colliding definition
    // comes lexically before or after the `use`.
    let program = r#"
        mod a { fn f(x: i64) { x } }
        use a;
        fn f(x: i64) { x }
        f(1)
    "#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::DuplicateUseBinding(ref name, ..) if name == "f"
    );
}

#[test]
fn test_reusing_the_same_specific_item_is_not_an_error() {
    // Unlike a genuine clash, re-`use`ing the exact same item twice is a
    // harmless no-op, same as Rust allows.
    let program = r#"
        mod a { fn f(x: i64) { x + 1 } }
        use a::f;
        use a::f;
        f(1)
    "#;
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_reusing_the_same_whole_module_is_not_an_error() {
    let program = r#"
        mod a { fn f(x: i64) { x + 1 } }
        use a;
        use a;
        f(1)
    "#;
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_use_of_a_module_brings_all_its_direct_members_into_scope() {
    // Like a C++ `using namespace`: every function declared directly in
    // `math` becomes callable unqualified after `use math;`.
    let program = r#"
        mod math {
            fn double(x: i64) { x * 2 }
            fn triple(x: i64) { x * 3 }
        }
        use math;
        (double(5), triple(5))
    "#;
    assert_matches!(
        type_check_str_ok(program),
        Type::Tuple(ref ts) if matches!(ts.as_slice(), [Type::I64, Type::I64])
    );
}

#[test]
fn test_use_of_a_specific_item_brings_only_that_item_into_scope() {
    // Like a Rust `use path::item;`: only the named function is brought
    // into scope, not its siblings.
    let program = r#"
        mod math {
            fn double(x: i64) { x * 2 }
            fn triple(x: i64) { x * 3 }
        }
        use math::double;
        double(5)
    "#;
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_use_of_a_specific_item_does_not_bring_its_siblings_into_scope() {
    let program = r#"
        mod math {
            fn double(x: i64) { x * 2 }
            fn triple(x: i64) { x * 3 }
        }
        use math::double;
        triple(5)
    "#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::IdentifierNotFound(ref name) if name == "triple"
    );
}

#[test]
fn test_use_resolves_relative_to_the_current_module() {
    // `use helpers;` inside `outer::run` finds the sibling module
    // `outer::helpers` relative to `outer`, without needing the full path.
    let program = r#"
        mod outer {
            mod helpers {
                fn double(x: i64) { x * 2 }
            }
            fn run(x: i64) {
                use helpers;
                double(x)
            }
        }
        outer::run(5)
    "#;
    assert_matches!(type_check_str_ok(program), Type::I64);
}

#[test]
fn test_use_inside_a_function_body_does_not_leak_to_the_caller() {
    // A `use` is local to the scope it's written in, same as a `let`: it
    // doesn't affect name resolution outside that function.
    let program = r#"
        mod math {
            fn double(x: i64) { x * 2 }
        }
        fn f(x: i64) {
            use math;
            double(x)
        }
        f(5);
        double(5)
    "#;
    assert_matches!(
        type_check_str_error(program),
        TypeCheckerError::IdentifierNotFound(ref name) if name == "double"
    );
}
