use crate::ast::expressions::{
    visit_expression, visit_expression_mut, Expression, TypedExpression,
};
use crate::ast::operators::Operator;
use crate::ast::types::Type;
use crate::ast::values::Value;
use crate::ast::link_symbols::LinkSymbols;
use crate::ast::{ParsedAST, TypedAST};
use crate::positional_error::PositionalError;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
// region errors

#[derive(Debug)]
pub(super) enum TypeCheckerError {
    IdentifierNotFound(String),
    UnificationFail(Type, Type),
    CircularTypes(usize, Type),
    ArityMismatch(usize, usize),
    TupleLengthMismatch(usize, usize),
    AmbiguousType(Type),
    InvalidAssignmentTarget,
    DuplicateTupleBinding(String),
    UncomparableType(Type),
    InvalidExponentBase(Type),
    InvalidUnaryOperand(Operator, Type),
    UnknownModule(String),
    /// A `use` tried to bring `name` into scope already bound (by an
    /// earlier `use` or an existing definition) to a *different* target --
    /// the two qualified names that collided are included for the message.
    DuplicateUseBinding(String, String, String),
}

impl Display for TypeCheckerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeCheckerError::IdentifierNotFound(id) => write!(f, "Identifier not found: {}", id),
            TypeCheckerError::UnificationFail(a, b) => {
                write!(f, "Type mismatch: expected {}, got {}", a, b)
            }
            TypeCheckerError::CircularTypes(id, t) => {
                write!(f, "Occurs check failed: ?t{} occurs in {}", id, t)
            }
            TypeCheckerError::ArityMismatch(expected, got) => {
                write!(
                    f,
                    "Arity mismatch: expected {} arguments, got {}",
                    expected, got
                )
            }
            TypeCheckerError::TupleLengthMismatch(expected, got) => {
                write!(
                    f,
                    "Tuple length mismatch: expected {}, got {}",
                    expected, got
                )
            }
            TypeCheckerError::AmbiguousType(t) => {
                write!(
                    f,
                    "Cannot determine a concrete type for `{}` — this program has no way to resolve it (e.g. an unused function whose parameter type is never constrained by a call)",
                    t
                )
            }
            TypeCheckerError::InvalidAssignmentTarget => {
                write!(f, "Left-hand side of `=` must be a variable")
            }
            TypeCheckerError::DuplicateTupleBinding(name) => {
                write!(
                    f,
                    "`{}` is bound more than once in this tuple destructure",
                    name
                )
            }
            TypeCheckerError::UncomparableType(t) => {
                write!(f, "`{}` values cannot be compared", t)
            }
            TypeCheckerError::InvalidExponentBase(t) => {
                write!(
                    f,
                    "The left-hand side of `^` must be i64 or f64, got `{}`",
                    t
                )
            }
            TypeCheckerError::InvalidUnaryOperand(op, t) => {
                let expected = if *op == Operator::Not { "bool" } else { "i64 or f64" };
                write!(
                    f,
                    "The operand of unary `{}` must be {}, got `{}`",
                    op, expected, t
                )
            }
            TypeCheckerError::UnknownModule(path) => {
                write!(f, "No module or item named `{}` found to `use`", path)
            }
            TypeCheckerError::DuplicateUseBinding(name, first, second) => {
                write!(
                    f,
                    "`{}` is ambiguous: brought into scope by both `{}` and `{}`",
                    name, first, second
                )
            }
        }
    }
}

impl Error for TypeCheckerError {}

fn tc_err(err: TypeCheckerError, loc: (usize, usize)) -> PositionalError {
    PositionalError::new(Box::new(err), loc.0, loc.1)
}

// endregion errors

// region state

struct TypeCheckingState {
    m_environment: HashMap<String, Type>,
    m_substitutions: SubstitutionMap,
    m_next_var_type: usize,
    m_next_anon_id: usize,
    /// Function types pre-registered by `hoist_top_level_function_signatures`,
    /// keyed by fully-qualified name. Consumed the first time that name's
    /// `FunctionDefinition` is actually inferred, so a later redefinition
    /// under the same name falls back to its own fresh type variables.
    m_hoisted_signatures: HashMap<String, Type>,
    /// The chain of enclosing module names at the current point of
    /// inference, e.g. `["outer", "inner"]` inside `mod outer { mod inner
    /// { .. } }`. Used to qualify module-level names and to resolve
    /// references innermost-module-outward, the same way Rust does.
    m_module_path: Vec<String>,
    /// Short-name -> qualified-name redirects from `use` statements (a
    /// whole module's members, or one item), like a C++ `using namespace`.
    /// Looked up dynamically per reference rather than snapshotted, so a
    /// `use` naming something not yet inferred still resolves once it is.
    m_aliases: HashMap<String, String>,
}

impl TypeCheckingState {
    fn next_variable_type(&mut self) -> Type {
        let id = self.m_next_var_type;
        self.m_next_var_type += 1;
        Type::TVar(id)
    }

    fn next_anon_func_id(&mut self) -> String {
        self.m_next_anon_id += 1;
        format!("__anon_func_{}", self.m_next_anon_id)
    }

    /// Prefixes `name` with the current module path, e.g. `qualify("f")`
    /// while inside `mod outer` returns `"outer::f"`.
    fn qualify(&self, name: &str) -> String {
        if self.m_module_path.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.m_module_path.join("::"), name)
        }
    }

    /// Resolves a reference against the environment by trying it prefixed
    /// with progressively fewer enclosing module segments, innermost first,
    /// down to the reference exactly as written. Returns the qualified name
    /// that matched, along with its type. A name brought into scope by a
    /// `use` is tried first, redirecting to its qualified target.
    fn resolve(&self, id: &str) -> Option<(String, Type)> {
        if let Some(target) = self.m_aliases.get(id)
            && let Some(t) = self.m_environment.get(target)
        {
            return Some((target.clone(), t.clone()));
        }
        for i in (0..=self.m_module_path.len()).rev() {
            let candidate = if i == 0 {
                id.to_string()
            } else {
                format!("{}::{}", self.m_module_path[..i].join("::"), id)
            };
            if let Some(t) = self.m_environment.get(&candidate) {
                return Some((candidate, t.clone()));
            }
        }
        None
    }

    /// Resolves what a `use path;` should bring into scope, tried with the
    /// same innermost-outward search as an ordinary reference. If `path`
    /// names an item directly, that item alone is brought in under its
    /// last segment; if it names a module, every *direct* child is brought
    /// in (like a C++ `using namespace`, not recursive). An empty result
    /// means `path` matches neither, which the caller turns into
    /// `UnknownModule`.
    fn resolve_use_targets(&self, path: &str) -> Vec<(String, String)> {
        for i in (0..=self.m_module_path.len()).rev() {
            let candidate = if i == 0 {
                path.to_string()
            } else {
                format!("{}::{}", self.m_module_path[..i].join("::"), path)
            };

            if self.m_environment.contains_key(&candidate) {
                let short_name = candidate.rsplit("::").next().unwrap().to_string();
                return vec![(short_name, candidate)];
            }

            let prefix = format!("{}::", candidate);
            let mut children: Vec<(String, String)> = self
                .m_environment
                .keys()
                .filter_map(|k| {
                    k.strip_prefix(prefix.as_str())
                        .filter(|rest| !rest.contains("::"))
                        .map(|rest| (rest.to_string(), k.clone()))
                })
                .collect();
            // `m_environment` is a HashMap, so iteration order (and thus
            // which colliding name gets reported first, if several of this
            // module's members collide with something already in scope) is
            // otherwise nondeterministic across runs.
            children.sort();
            if !children.is_empty() {
                return children;
            }
        }
        Vec::new()
    }
}

// endregion state

// region algorithm

type SubstitutionMap = HashMap<usize, Type>;

/// Applies a set of type substitutions to a given type.
///
/// - This function performs deep substitution for nested types.
/// - If a substitution leads to a type variable that also has a substitution, the function
///   recursively resolves it.
/// - Can result in a TVar which must be resolved later in a valid program
fn apply_substitutions(subst: &SubstitutionMap, ty: &Type) -> Type {
    match ty {
        Type::TVar(id) => match subst.get(id) {
            Some(t) => apply_substitutions(subst, t),
            None => ty.clone(),
        },
        Type::Function(args, ret) => Type::Function(
            args.iter().map(|a| apply_substitutions(subst, a)).collect(),
            Box::new(apply_substitutions(subst, ret)),
        ),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| apply_substitutions(subst, t)).collect()),
        _ => ty.clone(),
    }
}

/// Checks that `t1` and `t2` bind to the same type.
fn unify(
    t1: &Type,
    t2: &Type,
    subst: &mut SubstitutionMap,
    loc: (usize, usize),
) -> Result<(), PositionalError> {
    /// Applies substitutions to `ty` and checks if type `id` is used to define `ty`.
    fn check_circular_types(subst: &SubstitutionMap, ty: &Type, id: usize) -> bool {
        match apply_substitutions(subst, ty) {
            Type::TVar(vid) => vid == id,
            Type::Function(args, ret) => {
                args.iter().any(|a| check_circular_types(subst, a, id))
                    || check_circular_types(subst, &ret, id)
            }
            Type::Tuple(ts) => ts.iter().any(|t| check_circular_types(subst, t, id)),
            _ => false,
        }
    }

    let t1 = apply_substitutions(subst, t1);
    let t2 = apply_substitutions(subst, t2);

    if t1 == t2 {
        return Ok(());
    }

    match (&t1, &t2) {
        (Type::TVar(id), _) => {
            if check_circular_types(subst, &t2, *id) {
                return Err(tc_err(TypeCheckerError::CircularTypes(*id, t2), loc));
            }
            subst.insert(*id, t2);
            Ok(())
        }
        (_, Type::TVar(id)) => {
            if check_circular_types(subst, &t1, *id) {
                return Err(tc_err(TypeCheckerError::CircularTypes(*id, t1), loc));
            }
            subst.insert(*id, t1);
            Ok(())
        }
        (Type::Function(a1, r1), Type::Function(a2, r2)) => {
            if a1.len() != a2.len() {
                return Err(tc_err(
                    TypeCheckerError::ArityMismatch(a1.len(), a2.len()),
                    loc,
                ));
            }
            for (a, b) in a1.iter().zip(a2.iter()) {
                unify(a, b, subst, loc)?;
            }
            unify(r1, r2, subst, loc)
        }
        (Type::Tuple(ts1), Type::Tuple(ts2)) => {
            if ts1.len() != ts2.len() {
                return Err(tc_err(
                    TypeCheckerError::TupleLengthMismatch(ts1.len(), ts2.len()),
                    loc,
                ));
            }
            for (a, b) in ts1.iter().zip(ts2.iter()) {
                unify(a, b, subst, loc)?;
            }
            Ok(())
        }
        _ => Err(tc_err(
            TypeCheckerError::UnificationFail(t1.clone(), t2.clone()),
            loc,
        )),
    }
}

// endregion algorithm

fn infer(
    state: &mut TypeCheckingState,
    expr: &mut TypedExpression,
) -> Result<Type, PositionalError> {
    let loc = expr.m_location;
    let ty = match &mut expr.m_expression {
        Expression::Constant(c) => Ok(match c {
            Value::Integer(_) => Type::I64,
            Value::Double(_) => Type::F64,
            Value::String(_) => Type::String,
            Value::Boolean(_) => Type::Boolean,
        }),

        Expression::Unit => Ok(Type::Unit),
        Expression::Module(name, body) => {
            if let Some(body) = body {
                state.m_module_path.push(name.clone());
                for stmt in body.iter_mut() {
                    infer(state, stmt)?;
                }
                state.m_module_path.pop();
            }
            Ok(Type::Unit)
        }
        Expression::UseStatement(path) => {
            let targets = state.resolve_use_targets(path);
            if targets.is_empty() {
                return Err(tc_err(TypeCheckerError::UnknownModule(path.clone()), loc));
            }
            for (short_name, target) in targets {
                // Rust-style: a `use` that collides with something already
                // in scope under the same name is an error, unless it's
                // the exact same target (re-`use`ing the same item is a
                // harmless no-op, same as Rust allows).
                if let Some((existing, _)) = state.resolve(&short_name) {
                    if existing != target {
                        return Err(tc_err(
                            TypeCheckerError::DuplicateUseBinding(short_name, existing, target),
                            loc,
                        ));
                    }
                    continue;
                }
                state.m_aliases.insert(short_name, target);
            }
            Ok(Type::Unit)
        }
        Expression::While(cond, body) => {
            let ty_cond = infer(state, cond)?;
            infer(state, body)?;
            unify(&ty_cond, &Type::Boolean, &mut state.m_substitutions, loc)?;
            Ok(Type::Unit)
        }

        Expression::For(init, cond, update, body) => {
            if let Some(init) = init {
                infer(state, init)?;
            }
            let ty_cond = infer(state, cond)?;
            unify(&ty_cond, &Type::Boolean, &mut state.m_substitutions, loc)?;
            infer(state, body)?;
            infer(state, update)?;
            Ok(Type::Unit)
        }

        Expression::Variable(id) => match state.resolve(id) {
            Some((resolved, t)) => {
                *id = resolved;
                Ok(t)
            }
            None => Err(tc_err(
                TypeCheckerError::IdentifierNotFound(id.clone()),
                loc,
            )),
        },

        Expression::Tuple(elts) => {
            let mut types = Vec::new();
            for elt in elts.iter_mut() {
                types.push(infer(state, elt)?);
            }
            Ok(Type::Tuple(types))
        }

        Expression::Block(stmts) => {
            let mut ty = Type::Unit;
            for stmt in stmts.iter_mut() {
                ty = infer(state, stmt)?;
            }
            Ok(ty)
        }

        Expression::PrintStatement(val) => {
            infer(state, val)?;
            Ok(Type::Unit)
        }

        Expression::BinaryOp(left, op, right) => {
            let tl = infer(state, left)?;
            let tr = infer(state, right)?;
            match op {
                Operator::Add | Operator::Minus | Operator::Multiply | Operator::Divide => {
                    unify(&tl, &tr, &mut state.m_substitutions, loc)?;
                    Ok(apply_substitutions(&state.m_substitutions, &tl))
                }
                Operator::Exponent => {
                    let resolved_base = apply_substitutions(&state.m_substitutions, &tl);
                    match resolved_base {
                        Type::I64 | Type::F64 => {
                            unify(&Type::I64, &tr, &mut state.m_substitutions, loc)?;
                            Ok(resolved_base)
                        }
                        other => Err(tc_err(TypeCheckerError::InvalidExponentBase(other), loc)),
                    }
                }
                Operator::Eq | Operator::NotEq => {
                    unify(&tl, &tr, &mut state.m_substitutions, loc)?;
                    let resolved = apply_substitutions(&state.m_substitutions, &tl);
                    if resolved == Type::String {
                        return Err(tc_err(TypeCheckerError::UncomparableType(resolved), loc));
                    }
                    Ok(Type::Boolean)
                }
                Operator::Less | Operator::LessEq | Operator::Greater | Operator::GreaterEq => {
                    unify(&tl, &tr, &mut state.m_substitutions, loc)?;
                    Ok(Type::Boolean)
                }
                Operator::And | Operator::Or => {
                    unify(&tl, &Type::Boolean, &mut state.m_substitutions, loc)?;
                    unify(&tr, &Type::Boolean, &mut state.m_substitutions, loc)?;
                    Ok(Type::Boolean)
                }
                Operator::Assign => {
                    if !matches!(&left.m_expression, Expression::Variable(_)) {
                        return Err(tc_err(TypeCheckerError::InvalidAssignmentTarget, loc));
                    }
                    unify(&tl, &tr, &mut state.m_substitutions, loc)?;
                    Ok(tl.clone())
                }
                Operator::Not => unreachable!("the parser only ever produces `!` as a unary op"),
            }
        }

        Expression::UnaryOp(op, operand) => {
            let t_operand = infer(state, operand)?;
            match op {
                Operator::Minus => {
                    let resolved = apply_substitutions(&state.m_substitutions, &t_operand);
                    match resolved {
                        Type::I64 | Type::F64 => Ok(resolved),
                        other => Err(tc_err(
                            TypeCheckerError::InvalidUnaryOperand(Operator::Minus, other),
                            loc,
                        )),
                    }
                }
                Operator::Not => {
                    let resolved = apply_substitutions(&state.m_substitutions, &t_operand);
                    match resolved {
                        Type::Boolean => Ok(Type::Boolean),
                        other => Err(tc_err(
                            TypeCheckerError::InvalidUnaryOperand(Operator::Not, other),
                            loc,
                        )),
                    }
                }
                _ => unreachable!("the parser only ever produces a unary `-` or `!`"),
            }
        }

        Expression::If(cond, then_expr, else_expr) => {
            infer(state, cond)?;
            let t_then = infer(state, then_expr)?;
            match else_expr {
                Some(else_branch) => {
                    let t_else = infer(state, else_branch)?;
                    unify(&t_then, &t_else, &mut state.m_substitutions, loc)?;
                    Ok(apply_substitutions(&state.m_substitutions, &t_then))
                }
                None => Ok(Type::Unit),
            }
        }

        Expression::FunctionDefinition(o_name, params, body) => {
            // Named module-level functions are registered under their fully
            // qualified path (`outer::inner::name`), so calls from anywhere
            // else can reach them via `resolve` the same way any other
            // reference does.
            let qualified_name = o_name.as_ref().map(|n| state.qualify(n));

            // If pre-registered by the hoisting pass, reuse its param/return
            // type variables instead of minting fresh ones, so earlier
            // siblings that already called this one by name get unified
            // against the variables this definition resolves.
            let hoisted = qualified_name
                .as_ref()
                .and_then(|q| state.m_hoisted_signatures.remove(q));

            let (param_types, ret_var): (Vec<Type>, Type) = match hoisted {
                Some(Type::Function(hoisted_params, hoisted_ret)) => (hoisted_params, *hoisted_ret),
                _ => {
                    // Assign type variables to each unknown param type
                    let param_types: Vec<Type> = params
                        .iter()
                        .map(|(_, t)| {
                            if *t == Type::Unknown {
                                state.next_variable_type()
                            } else {
                                t.clone()
                            }
                        })
                        .collect();
                    let ret_var = state.next_variable_type();
                    (param_types, ret_var)
                }
            };

            // Replace each Unknown placeholder with the fresh TVar
            for ((_, t), fresh_t) in params.iter_mut().zip(param_types.iter()) {
                *t = fresh_t.clone();
            }

            // Build the function type with a fresh return var so recursive
            // calls can look it up before we know the return type.
            let fn_type = Type::Function(param_types.clone(), Box::new(ret_var.clone()));

            // Register in env before checking the body (enables recursion).
            let name = match qualified_name {
                Some(n) => n,
                None => {
                    let anon = state.next_anon_func_id();
                    state.qualify(&anon)
                }
            };
            state.m_environment.insert(name.clone(), fn_type.clone());

            let saved_env = state.m_environment.clone();
            let saved_aliases = state.m_aliases.clone();
            let resolved = {
                // Build inner env with params.
                for ((param_name, _annotate), t_param) in params.iter().zip(param_types.iter()) {
                    state
                        .m_environment
                        .insert(param_name.clone(), t_param.clone());
                }

                let mut t_body = Type::Unit;
                for stmt in body.iter_mut() {
                    t_body = infer(state, stmt)?;
                }

                // Write back the fully-qualified name so later passes
                // (evaluator, BLIR lowering) see it directly.
                *o_name = Some(name.clone());

                // Unify the placeholder return var with the actual body type.
                unify(&ret_var, &t_body, &mut state.m_substitutions, loc)?;

                apply_substitutions(&state.m_substitutions, &fn_type)
            };

            // Update the outer env entry with the resolved type. Any `use`
            // inside the body is local to it too, same as its param/local
            // bindings.
            state.m_environment = saved_env;
            state.m_aliases = saved_aliases;
            state.m_environment.insert(name, resolved.clone());

            Ok(resolved)
        }

        Expression::FunctionCall(args) => {
            let (callee, args) = match args.as_mut_slice() {
                [callee, args @ ..] => (callee, args),
                _ => panic!(
                    "Invalid function call (ast_parser can never output such a malformed expression)"
                ),
            };

            let callee_ty = infer(state, callee)?;

            let mut arg_types = Vec::new();
            for arg in args {
                arg_types.push(infer(state, arg)?);
            }

            let ret_var = state.next_variable_type();
            let expected_fn_ty = Type::Function(arg_types, Box::new(ret_var.clone()));
            unify(&callee_ty, &expected_fn_ty, &mut state.m_substitutions, loc)?;

            Ok(apply_substitutions(&state.m_substitutions, &ret_var))
        }

        Expression::LetStatement(lhs, annotated_type, rhs) => {
            let rhs_ty = infer(state, rhs)?;

            if *annotated_type != Type::Unknown {
                unify(
                    annotated_type,
                    &rhs_ty,
                    &mut state.m_substitutions,
                    rhs.m_location,
                )?;
            }

            let bound_ty = apply_substitutions(&state.m_substitutions, &rhs_ty);

            match &mut lhs.m_expression {
                Expression::Variable(id) => {
                    state.m_environment.insert(id.clone(), bound_ty.clone());
                    lhs.m_info = bound_ty;
                }
                Expression::Tuple(vars) => {
                    let Type::Tuple(ref elem_types) = bound_ty else {
                        return Err(tc_err(
                            TypeCheckerError::UnificationFail(Type::Tuple(vec![]), bound_ty),
                            loc,
                        ));
                    };
                    if vars.len() != elem_types.len() {
                        return Err(tc_err(
                            TypeCheckerError::TupleLengthMismatch(vars.len(), elem_types.len()),
                            loc,
                        ));
                    }
                    let mut seen_names = HashSet::new();
                    for (var, elem_ty) in vars.iter_mut().zip(elem_types.iter()) {
                        let Expression::Variable(var_id) = &mut var.m_expression else {
                            return Err(tc_err(TypeCheckerError::InvalidAssignmentTarget, loc));
                        };
                        if !seen_names.insert(var_id.clone()) {
                            return Err(tc_err(
                                TypeCheckerError::DuplicateTupleBinding(var_id.clone()),
                                loc,
                            ));
                        }
                        state.m_environment.insert(var_id.clone(), elem_ty.clone());
                        var.m_info = elem_ty.clone();
                    }
                    lhs.m_info = bound_ty;
                }
                _ => unreachable!(),
            }

            Ok(Type::Unit)
        }
    }?;

    expr.m_info = apply_substitutions(&state.m_substitutions, &ty);
    Ok(expr.m_info.clone())
}

// region entry point

fn apply_substitutions_to_expr(expr: &mut TypedExpression, subst: &SubstitutionMap) {
    visit_expression_mut(expr, &mut |node| {
        node.m_info = apply_substitutions(subst, &node.m_info);
        if let Expression::FunctionDefinition(_, params, _) = &mut node.m_expression {
            for (_, t) in params.iter_mut() {
                *t = apply_substitutions(subst, t);
            }
        }
    });
}

fn assert_no_unknown_types(expr: &TypedExpression) -> Result<(), PositionalError> {
    fn is_unresolved(ty: &Type) -> bool {
        match ty {
            Type::Unknown | Type::TVar(_) => true,
            Type::Function(args, ret) => args.iter().any(is_unresolved) || is_unresolved(ret),
            Type::Tuple(ts) => ts.iter().any(is_unresolved),
            _ => false,
        }
    }

    visit_expression(expr, &mut |node| {
        if is_unresolved(&node.m_info) {
            return Err(tc_err(
                TypeCheckerError::AmbiguousType(node.m_info.clone()),
                node.m_location,
            ));
        }
        if let Expression::FunctionDefinition(_, params, _) = &node.m_expression {
            for (_, t) in params {
                if is_unresolved(t) {
                    return Err(tc_err(
                        TypeCheckerError::AmbiguousType(t.clone()),
                        node.m_location,
                    ));
                }
            }
        }
        Ok(())
    })
}

/// Pre-registers a function type (fresh type variables standing in for the
/// return type and any unannotated parameter) for every named top-level
/// function, before any of them are type-checked -- so an earlier
/// function's body can call a sibling defined later (mutual recursion),
/// rather than only resolving calls to itself or an already-processed
/// sibling as `infer` would add them one at a time.
fn hoist_top_level_function_signatures(state: &mut TypeCheckingState, exprs: &[TypedExpression]) {
    for expr in exprs {
        match &expr.m_expression {
            Expression::FunctionDefinition(Some(name), params, _) => {
                let param_types: Vec<Type> = params
                    .iter()
                    .map(|(_, t)| {
                        if *t == Type::Unknown {
                            state.next_variable_type()
                        } else {
                            t.clone()
                        }
                    })
                    .collect();
                let ret_var = state.next_variable_type();
                let fn_type = Type::Function(param_types, Box::new(ret_var));
                let qualified = state.qualify(name);
                state
                    .m_environment
                    .insert(qualified.clone(), fn_type.clone());
                state.m_hoisted_signatures.insert(qualified, fn_type);
            }
            Expression::Module(name, Some(body)) => {
                state.m_module_path.push(name.clone());
                hoist_top_level_function_signatures(state, body);
                state.m_module_path.pop();
            }
            _ => {}
        }
    }
}

pub(crate) fn type_check(
    parsed_ast: ParsedAST,
    external_symbol_map: LinkSymbols,
) -> Result<TypedAST, PositionalError> {
    let mut state = TypeCheckingState {
        m_environment: external_symbol_map.m_map,
        m_substitutions: HashMap::new(),
        m_next_var_type: 0,
        m_next_anon_id: 0,
        m_hoisted_signatures: HashMap::new(),
        m_module_path: Vec::new(),
        m_aliases: HashMap::new(),
    };

    let mut typed_expressions = TypedAST::from(parsed_ast);

    hoist_top_level_function_signatures(&mut state, &typed_expressions.m_expressions);

    for expr in &mut typed_expressions.m_expressions {
        infer(&mut state, expr)?;
    }

    for expr in &mut typed_expressions.m_expressions {
        apply_substitutions_to_expr(expr, &state.m_substitutions);
    }

    for expr in &typed_expressions.m_expressions {
        assert_no_unknown_types(expr)?;
    }

    Ok(typed_expressions)
}

// endregion entry point
