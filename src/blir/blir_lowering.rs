use crate::ast::expressions::{visit_expression, Expression, TypedExpression};
use crate::ast::operators::Operator;
use crate::ast::types::Type;
use crate::ast::values::Value;
use crate::ast::TypedAST;
use crate::blir::{
    BLIRArithOp, BLIRCmpOp, BLIRCompileUnit, BLIRConstant, BLIRFunction, BLIRLabel, BLIROperation,
    BLIRRegister, BLIRType, SourceLoc,
};
use crate::positional_error::PositionalError;
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
enum IRGenError {
    UnexpectedRootExpression,
    InvalidFunction,
    UnsupportedExpression(Expression<TypedExpression>),
}

impl Display for IRGenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IRGenError::UnsupportedExpression(expr) => {
                write!(
                    f,
                    "`{}` is not yet supported by the LLVM backend",
                    expr.kind_name()
                )
            }
            other => write!(f, "{:?}", other),
        }
    }
}
impl Error for IRGenError {}

fn ir_gen_error(error_type: IRGenError, expr: &TypedExpression) -> PositionalError {
    PositionalError::new(Box::new(error_type), expr.m_location.0, expr.m_location.1)
}

impl Type {
    fn to_ir(&self) -> BLIRType {
        match self {
            Type::I64 => BLIRType::Int64,
            Type::F64 => BLIRType::Float64,
            Type::String => BLIRType::Ptr,
            Type::Boolean => BLIRType::Int64,
            Type::Function(_, _) => BLIRType::Ptr,
            Type::Tuple(_) => BLIRType::Void,
            Type::Unit => BLIRType::Void,
            Type::Unknown | Type::TVar(_) => BLIRType::Void,
        }
    }
}

impl Value {
    fn to_ir(&self) -> BLIRConstant {
        match self {
            Value::Integer(c) => BLIRConstant::Int64(*c),
            Value::Double(c) => BLIRConstant::Float64(*c),
            Value::Boolean(c) => BLIRConstant::Int64(*c as i64),
            Value::String(s) => BLIRConstant::String(s.clone()),
        }
    }
}

// Generates fresh, collision-free BLIR register/label names. Each frame
// owns its own counter, so a dependent evaluation (an `if` branch, a loop
// body, ...) can `enter` a child scope and draw names from zero without
// colliding with its parent's or a sibling's. A name is the dot-joined
// path of scope labels down the stack plus a role and local counter (e.g.
// `main.if0.then4.arith.1`) -- hierarchical and readable, like clang's
// `if.then`/`while.cond` block labels, rather than one flat numbering.
struct ScopeStack {
    frames: Vec<(String, usize)>,
}

impl ScopeStack {
    fn new(root: &str) -> Self {
        ScopeStack {
            frames: vec![(root.to_string(), 0)],
        }
    }

    fn depth(&self) -> usize {
        self.frames.len()
    }

    fn path(&self) -> String {
        self.frames
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }

    // Draws the next index from the current (innermost) scope.
    fn next_index(&mut self) -> usize {
        let (_, counter) = self.frames.last_mut().expect("ScopeStack is never empty");
        let idx = *counter;
        *counter += 1;
        idx
    }

    // Enters a child scope for a dependent evaluation. `label` is
    // qualified by an index fresh in the *parent* scope, so two sibling
    // scopes reusing the same label (e.g. two `if`s in a row) still get
    // distinct paths. Pair with `leave` once the dependent evaluation is
    // fully compiled.
    fn enter(&mut self, label: &str) {
        let idx = self.next_index();
        self.frames.push((format!("{}{}", label, idx), 0));
    }

    fn leave(&mut self) {
        self.frames
            .pop()
            .expect("ScopeStack::leave with no matching enter");
    }

    fn fresh(&mut self, role: &str) -> String {
        let idx = self.next_index();
        format!("{}.{}.{}", self.path(), role, idx)
    }

    fn fresh_reg(&mut self, role: &str, ty: BLIRType) -> BLIRRegister {
        BLIRRegister(self.fresh(role), ty)
    }

    fn fresh_label(&mut self, role: &str) -> BLIRLabel {
        BLIRLabel(self.fresh(role))
    }
}

// Lowers a single function's body into a flat sequence of BLIROperations.
// Owns both the growing op list and the ScopeStack that names its
// registers/labels, and exposes lowering as methods instead of threading
// that state through free functions -- mirrors the evaluator's `State`.
struct FunctionLowering {
    m_ops: Vec<BLIROperation>,
    // Index-aligned with m_ops: the source location each op was lowered
    // from, so the pretty-printer can annotate BLIR with source lines.
    m_locs: Vec<SourceLoc>,
    m_scope: ScopeStack,
}

impl FunctionLowering {
    fn new(fn_name: &str) -> Self {
        FunctionLowering {
            m_ops: Vec::new(),
            m_locs: Vec::new(),
            m_scope: ScopeStack::new(fn_name),
        }
    }

    fn push_op(&mut self, op: BLIROperation, loc: (usize, usize)) {
        let (line, char) = loc;
        self.m_ops.push(op);
        self.m_locs.push(SourceLoc {
            m_line: line,
            m_char: char,
        });
    }

    // Compiles an expression that produces a scalar value into a register.
    fn compile_expr_to_reg(
        &mut self,
        expr: &TypedExpression,
    ) -> Result<BLIRRegister, PositionalError> {
        let loc = expr.m_location;
        match &**expr {
            Expression::Variable(name) => {
                let from = BLIRRegister(name.clone(), expr.m_info.to_ir());
                let dest = self
                    .m_scope
                    .fresh_reg(&format!("{}.loaded", name), expr.m_info.to_ir());
                self.push_op(BLIROperation::Load(from, dest.clone()), loc);
                Ok(dest)
            }
            Expression::BinaryOp(lhs, op, rhs) => match op {
                Operator::Less
                | Operator::LessEq
                | Operator::Greater
                | Operator::GreaterEq
                | Operator::Eq
                | Operator::NotEq => {
                    let lhs_reg = self.compile_expr_to_reg(lhs)?;
                    let rhs_reg = self.compile_expr_to_reg(rhs)?;
                    let result = self.m_scope.fresh_reg("cmp", BLIRType::Int64);
                    let cmp_op = match op {
                        Operator::Less => BLIRCmpOp::Lt,
                        Operator::LessEq => BLIRCmpOp::Le,
                        Operator::Greater => BLIRCmpOp::Gt,
                        Operator::GreaterEq => BLIRCmpOp::Ge,
                        Operator::Eq => BLIRCmpOp::Eq,
                        Operator::NotEq => BLIRCmpOp::Ne,
                        _ => unreachable!(),
                    };
                    self.push_op(
                        BLIROperation::ICmp(cmp_op, result.clone(), lhs_reg, rhs_reg),
                        loc,
                    );
                    Ok(result)
                }
                Operator::Add | Operator::Minus | Operator::Multiply | Operator::Divide => {
                    let lhs_reg = self.compile_expr_to_reg(lhs)?;
                    let rhs_reg = self.compile_expr_to_reg(rhs)?;
                    let result = self.m_scope.fresh_reg("arith", lhs_reg.1);
                    let arith_op = match op {
                        Operator::Add => BLIRArithOp::Add,
                        Operator::Minus => BLIRArithOp::Sub,
                        Operator::Multiply => BLIRArithOp::Mul,
                        Operator::Divide => BLIRArithOp::Div,
                        _ => unreachable!(),
                    };
                    self.push_op(
                        BLIROperation::IArith(arith_op, result.clone(), lhs_reg, rhs_reg),
                        loc,
                    );
                    Ok(result)
                }
                // Short-circuiting: `lhs && rhs` is `if lhs { rhs } else { false }`,
                // `lhs || rhs` is `if lhs { true } else { rhs }` -- rhs is only
                // ever compiled (and its side effects only ever run) along the
                // branch that needs its value, mirroring `Expression::If` below.
                Operator::And | Operator::Or => {
                    let lhs_reg = self.compile_expr_to_reg(lhs)?;
                    let role = if *op == Operator::And { "and" } else { "or" };
                    self.m_scope.enter(role);

                    let dest = self.m_scope.fresh_reg("result", BLIRType::Int64);
                    self.push_op(BLIROperation::StackAllocVariable(dest.clone()), loc);

                    let true_label = self.m_scope.fresh_label("true");
                    let false_label = self.m_scope.fresh_label("false");
                    let end_label = self.m_scope.fresh_label("end");

                    self.push_op(
                        BLIROperation::CondBranch(
                            lhs_reg,
                            false_label.clone(),
                            true_label.clone(),
                        ),
                        loc,
                    );

                    self.push_op(BLIROperation::MarkLabel(true_label), loc);
                    if *op == Operator::And {
                        let rhs_reg = self.compile_expr_to_reg_scoped(rhs, "rhs")?;
                        self.push_op(BLIROperation::StoreFromReg(rhs_reg, dest.clone()), loc);
                    } else {
                        self.push_op(
                            BLIROperation::StoreFromConst(BLIRConstant::Int64(1), dest.clone()),
                            loc,
                        );
                    }
                    self.push_op(BLIROperation::JumpToLabel(end_label.clone()), loc);

                    self.push_op(BLIROperation::MarkLabel(false_label), loc);
                    if *op == Operator::And {
                        self.push_op(
                            BLIROperation::StoreFromConst(BLIRConstant::Int64(0), dest.clone()),
                            loc,
                        );
                    } else {
                        let rhs_reg = self.compile_expr_to_reg_scoped(rhs, "rhs")?;
                        self.push_op(BLIROperation::StoreFromReg(rhs_reg, dest.clone()), loc);
                    }
                    self.push_op(BLIROperation::JumpToLabel(end_label.clone()), loc);

                    self.push_op(BLIROperation::MarkLabel(end_label), loc);

                    let result = self.m_scope.fresh_reg("result.loaded", BLIRType::Int64);
                    self.push_op(BLIROperation::Load(dest, result.clone()), loc);
                    self.m_scope.leave();
                    Ok(result)
                }
                Operator::Assign => {
                    if let Expression::Variable(var) = &***lhs {
                        let dest_reg = BLIRRegister(var.into(), lhs.m_info.to_ir());
                        let src = self.compile_expr_to_reg(rhs)?;
                        self.push_op(
                            BLIROperation::StoreFromReg(src.clone(), dest_reg.clone()),
                            loc,
                        );
                        Ok(dest_reg)
                    } else {
                        Err(ir_gen_error(IRGenError::InvalidFunction, expr))
                    }
                }
                Operator::Exponent => {
                    let base_reg = self.compile_expr_to_reg(lhs)?;
                    let power_reg = self.compile_expr_to_reg(rhs)?;
                    let dest = self.m_scope.fresh_reg("pow", base_reg.1);
                    let op = match (base_reg.1, power_reg.1) {
                        (BLIRType::Float64, BLIRType::Float64) => {
                            BLIROperation::PowFF(dest.clone(), base_reg, power_reg)
                        }
                        (BLIRType::Float64, _) => {
                            BLIROperation::PowFI(dest.clone(), base_reg, power_reg)
                        }
                        _ => BLIROperation::PowII(dest.clone(), base_reg, power_reg),
                    };
                    self.push_op(op, loc);
                    Ok(dest)
                }
                Operator::Not => unreachable!("the parser only ever produces `!` as a unary op"),
            },
            Expression::UnaryOp(op, operand) => match op {
                Operator::Minus => {
                    let operand_reg = self.compile_expr_to_reg(operand)?;
                    let zero_const = if operand_reg.1 == BLIRType::Float64 {
                        BLIRConstant::Float64(0.0)
                    } else {
                        BLIRConstant::Int64(0)
                    };
                    let zero_reg = self.m_scope.fresh_reg("const", operand_reg.1);
                    self.push_op(BLIROperation::LoadConst(zero_const, zero_reg.clone()), loc);

                    let result = self.m_scope.fresh_reg("arith", operand_reg.1);
                    self.push_op(
                        BLIROperation::IArith(
                            BLIRArithOp::Sub,
                            result.clone(),
                            zero_reg,
                            operand_reg,
                        ),
                        loc,
                    );
                    Ok(result)
                }
                // Booleans are always Int64(0|1) in BLIR, so `!x` is just
                // `1 - x` -- no ICmp/bool-typed BLIR representation needed.
                Operator::Not => {
                    let operand_reg = self.compile_expr_to_reg(operand)?;
                    let one_reg = self.m_scope.fresh_reg("const", BLIRType::Int64);
                    self.push_op(
                        BLIROperation::LoadConst(BLIRConstant::Int64(1), one_reg.clone()),
                        loc,
                    );

                    let result = self.m_scope.fresh_reg("not", BLIRType::Int64);
                    self.push_op(
                        BLIROperation::IArith(
                            BLIRArithOp::Sub,
                            result.clone(),
                            one_reg,
                            operand_reg,
                        ),
                        loc,
                    );
                    Ok(result)
                }
                _ => unreachable!("the parser only ever produces a unary `-` or `!`"),
            },
            Expression::Constant(val) => {
                let dest = self.m_scope.fresh_reg("const", val.to_ir().get_type());
                self.push_op(BLIROperation::LoadConst(val.to_ir(), dest.clone()), loc);
                Ok(dest)
            }
            Expression::FunctionCall(args) => {
                let fn_name = match &*args[0] {
                    Expression::Variable(name) => name.clone(),
                    _ => unreachable!(),
                };
                let mut arg_regs = Vec::new();
                for arg in &args[1..] {
                    arg_regs.push(self.compile_expr_to_reg(arg)?);
                }
                let dest = self.m_scope.fresh_reg("call", expr.m_info.to_ir());
                self.push_op(
                    BLIROperation::CallFunction(fn_name, arg_regs, dest.clone()),
                    loc,
                );
                Ok(dest)
            }
            Expression::If(cond, then_expr, else_expr) => {
                // A bare `if cond { .. }` with no `else` is valid syntax and
                // type-checks to Unit (see type_checker.rs's If arm), so
                // `else_expr` being None here is a normal case, not an
                // invariant violation — there's simply nothing to compile
                // for that branch.
                let cond_reg = self.compile_expr_to_reg(cond)?;

                self.m_scope.enter("if");

                // Only allocate a result slot when the if actually produces
                // a value; a statement-only if (both branches Unit, e.g.
                // just prints) has nothing to store or load.
                let dest = if expr.m_info != Type::Unit {
                    let d = self.m_scope.fresh_reg("result", expr.m_info.to_ir());
                    self.push_op(BLIROperation::StackAllocVariable(d.clone()), loc);
                    Some(d)
                } else {
                    None
                };

                let if_true_label = self.m_scope.fresh_label("true");
                let if_false_label = self.m_scope.fresh_label("false");
                let if_end_label = self.m_scope.fresh_label("end");

                // CondBranch: if reg == 0 (false) jump to label 1, else jump to label 2
                self.push_op(
                    BLIROperation::CondBranch(
                        cond_reg,
                        if_false_label.clone(),
                        if_true_label.clone(),
                    ),
                    loc,
                );

                self.push_op(BLIROperation::MarkLabel(if_true_label.clone()), loc);
                self.m_scope.enter("then");
                self.compile_if_branch(then_expr, dest.as_ref())?;
                self.m_scope.leave();
                self.push_op(BLIROperation::JumpToLabel(if_end_label.clone()), loc);

                self.push_op(BLIROperation::MarkLabel(if_false_label.clone()), loc);
                if let Some(else_expr) = else_expr {
                    self.m_scope.enter("else");
                    self.compile_if_branch(else_expr, dest.as_ref())?;
                    self.m_scope.leave();
                }
                self.push_op(BLIROperation::JumpToLabel(if_end_label.clone()), loc);

                self.push_op(BLIROperation::MarkLabel(if_end_label.clone()), loc);

                let result = match dest {
                    Some(d) => {
                        // `d` is a stack slot (only ever stored into above),
                        // not yet a usable SSA value. Load it out before
                        // returning, so every caller (return position, a
                        // BinaryOp operand, a call argument, another if's
                        // branch, ...) gets a ready-to-use value, the same
                        // contract the `Variable` arm above provides.
                        let loaded = self.m_scope.fresh_reg("result.loaded", d.1);
                        self.push_op(BLIROperation::Load(d, loaded.clone()), loc);
                        Ok(loaded)
                    }
                    // Statement-only if: nothing to return. This register is
                    // never stored anywhere, so callers that discard the
                    // result (e.g. compile_body compiling an if as a
                    // statement) never observe it.
                    None => Ok(BLIRRegister("unit".into(), BLIRType::Void)),
                };
                self.m_scope.leave();
                result
            }
            Expression::Block(body) => body
                .iter()
                .map(|expr| self.compile_expr_to_reg(expr))
                .collect::<Result<Vec<_>, _>>()?
                .last()
                .ok_or_else(|| ir_gen_error(IRGenError::InvalidFunction, expr))
                .cloned(),
            _ => Err(ir_gen_error(IRGenError::InvalidFunction, expr)),
        }
    }

    // Compiles `expr` into a register within its own child scope, so the
    // intermediate registers it takes to compute one value don't leak into
    // (or consume counter slots meant for) the surrounding statement
    // sequence.
    fn compile_expr_to_reg_scoped(
        &mut self,
        expr: &TypedExpression,
        role: &str,
    ) -> Result<BLIRRegister, PositionalError> {
        self.m_scope.enter(role);
        let result = self.compile_expr_to_reg(expr)?;
        self.m_scope.leave();
        Ok(result)
    }

    // Compiles one branch (always a Block) of an if. When `dest` is Some,
    // the branch must produce a value that gets stored into it; when None,
    // the branch is compiled as plain statements and any value is discarded.
    fn compile_if_branch(
        &mut self,
        branch: &TypedExpression,
        dest: Option<&BLIRRegister>,
    ) -> Result<(), PositionalError> {
        match dest {
            Some(d) => {
                let loc = branch.m_location;
                let val_reg = self.compile_expr_to_reg_scoped(branch, "branch")?;
                self.push_op(BLIROperation::StoreFromReg(val_reg, d.clone()), loc);
            }
            None => {
                if let Expression::Block(stmts) = &**branch {
                    self.compile_body(stmts)?;
                }
            }
        }
        Ok(())
    }

    fn create_stack_variable(
        &mut self,
        dest: BLIRRegister,
        init: Value,
        loc: (usize, usize),
    ) -> Result<(), PositionalError> {
        self.push_op(BLIROperation::StackAllocVariable(dest.clone()), loc);
        self.push_op(BLIROperation::StoreFromConst(init.to_ir(), dest), loc);
        Ok(())
    }

    // Allocates a stack slot for `id` and initializes it from `rhs`, used
    // for both a plain `let x = ..` and each element of a destructured
    // `let (a, b, ..) = ..`.
    fn compile_let_binding(
        &mut self,
        id: &str,
        ty: BLIRType,
        rhs: &TypedExpression,
    ) -> Result<(), PositionalError> {
        let dest = BLIRRegister(id.to_string(), ty);
        let loc = rhs.m_location;
        if let Expression::Constant(value) = &**rhs {
            self.create_stack_variable(dest, value.clone(), loc)
        } else {
            self.push_op(BLIROperation::StackAllocVariable(dest.clone()), loc);
            let src = self.compile_expr_to_reg_scoped(rhs, "let")?;
            self.push_op(BLIROperation::StoreFromReg(src, dest), loc);
            Ok(())
        }
    }

    fn compile_body(&mut self, body: &[TypedExpression]) -> Result<(), PositionalError> {
        for expr in body {
            let loc = expr.m_location;
            match &**expr {
                Expression::LetStatement(lhs, _, rhs) => match &***lhs {
                    Expression::Variable(id) => {
                        self.compile_let_binding(id, lhs.m_info.to_ir(), rhs)?;
                    }
                    Expression::Tuple(_) => {
                        // No aggregate/struct type exists in BLIR yet --
                        // left for when BLIR gains general struct support,
                        // rather than special-cased tuple destructuring.
                        return Err(ir_gen_error(
                            IRGenError::UnsupportedExpression((***lhs).clone()),
                            expr,
                        ));
                    }
                    _ => unreachable!("type checker guarantees let's lhs is a Variable or Tuple"),
                },
                Expression::PrintStatement(e) => {
                    let reg = self.compile_expr_to_reg_scoped(e, "print")?;
                    self.push_op(BLIROperation::CallPrintf(reg), loc);
                }
                Expression::If(..) => {
                    // compile_expr_to_reg's If arm handles both
                    // value-producing and statement-only ifs; a statement
                    // here just discards whatever (if anything) it returns.
                    self.compile_expr_to_reg_scoped(expr, "stmt")?;
                }
                Expression::While(cond, body) => {
                    self.m_scope.enter("while");

                    let cond_evaluate_label = self.m_scope.fresh_label("cond_evaluate");
                    let loop_start_label = self.m_scope.fresh_label("loop_start");
                    let loop_end_label = self.m_scope.fresh_label("loop_end");

                    self.push_op(BLIROperation::JumpToLabel(cond_evaluate_label.clone()), loc);

                    self.push_op(BLIROperation::MarkLabel(cond_evaluate_label.clone()), loc);
                    let cond_reg = self.compile_expr_to_reg_scoped(cond, "cond")?;
                    self.push_op(
                        BLIROperation::CondBranch(
                            cond_reg,
                            loop_end_label.clone(),
                            loop_start_label.clone(),
                        ),
                        loc,
                    );
                    self.push_op(BLIROperation::MarkLabel(loop_start_label.clone()), loc);
                    if let Expression::Block(stmts) = &***body {
                        self.m_scope.enter("body");
                        self.compile_body(stmts)?;
                        self.m_scope.leave();
                    }
                    self.push_op(BLIROperation::JumpToLabel(cond_evaluate_label.clone()), loc);
                    self.push_op(BLIROperation::MarkLabel(loop_end_label), loc);

                    self.m_scope.leave();
                }
                Expression::BinaryOp(lhs, Operator::Assign, rhs) => {
                    if let Expression::Variable(var) = &***lhs {
                        let dst = BLIRRegister(var.to_string(), lhs.m_info.to_ir());
                        let src = self.compile_expr_to_reg_scoped(rhs, "assign")?;
                        self.push_op(BLIROperation::StoreFromReg(src, dst), loc);
                    } else {
                        unreachable!()
                    }
                }
                Expression::Variable(name) => {
                    let from = BLIRRegister(name.clone(), expr.m_info.to_ir());
                    let dest = self
                        .m_scope
                        .fresh_reg(&format!("{}.loaded", name), expr.m_info.to_ir());
                    self.push_op(BLIROperation::Load(from, dest), loc);
                }
                Expression::FunctionCall(_) => {
                    self.compile_expr_to_reg_scoped(expr, "stmt")?;
                }
                Expression::For(init, cond, update, body) => {
                    self.m_scope.enter("for");

                    if let Some(init_expr) = init {
                        self.m_scope.enter("init");
                        self.compile_body(std::slice::from_ref(&**init_expr))?;
                        self.m_scope.leave();
                    }

                    let cond_evaluate_label = self.m_scope.fresh_label("cond_evaluate");
                    let loop_start_label = self.m_scope.fresh_label("loop_start");
                    let loop_end_label = self.m_scope.fresh_label("loop_end");

                    self.push_op(BLIROperation::JumpToLabel(cond_evaluate_label.clone()), loc);

                    self.push_op(BLIROperation::MarkLabel(cond_evaluate_label.clone()), loc);
                    let cond_reg = self.compile_expr_to_reg_scoped(cond, "cond")?;
                    self.push_op(
                        BLIROperation::CondBranch(
                            cond_reg,
                            loop_end_label.clone(),
                            loop_start_label.clone(),
                        ),
                        loc,
                    );
                    self.push_op(BLIROperation::MarkLabel(loop_start_label.clone()), loc);
                    if let Expression::Block(stmts) = &***body {
                        self.m_scope.enter("body");
                        self.compile_body(stmts)?;
                        self.m_scope.leave();
                    }
                    self.m_scope.enter("update");
                    self.compile_body(std::slice::from_ref(&**update))?;
                    self.m_scope.leave();
                    self.push_op(BLIROperation::JumpToLabel(cond_evaluate_label.clone()), loc);
                    self.push_op(BLIROperation::MarkLabel(loop_end_label), loc);

                    self.m_scope.leave();
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl BLIRCompileUnit {
    fn compile_function_definition(
        &self,
        name: String,
        params: &Vec<(String, Type)>,
        body: &Vec<TypedExpression>,
    ) -> Result<BLIRFunction, PositionalError> {
        let mut func_def = BLIRFunction {
            m_name: name,
            m_params: Vec::new(),
            m_return: BLIRType::Void,
            m_return_reg: None,
            m_body: Vec::new(),
            m_locs: Vec::new(),
        };

        for (name, ty) in params {
            func_def
                .m_params
                .push(BLIRRegister(name.clone(), ty.to_ir()));
        }

        if let Some(last) = body.last() {
            func_def.m_return = last.m_info.to_ir();
        }

        let (body_stmts, return_expr) = if func_def.m_return != BLIRType::Void {
            match body.split_last() {
                Some((last, rest)) => (rest, Some(last)),
                None => (body.as_slice(), None),
            }
        } else {
            (body.as_slice(), None)
        };

        let mut lowering = FunctionLowering::new(&func_def.m_name);

        lowering.compile_body(body_stmts)?;

        if let Some(last) = return_expr {
            match lowering.compile_expr_to_reg_scoped(last, "return") {
                Ok(reg) => func_def.m_return_reg = Some(reg.0),
                // Expression type not supported
                Err(e) => return Err(e),
            }
        }

        assert_eq!(
            lowering.m_scope.depth(),
            1,
            "unbalanced ScopeStack enter/leave while compiling `{}`",
            func_def.m_name
        );

        func_def.m_body = lowering.m_ops;
        func_def.m_locs = lowering.m_locs;

        Ok(func_def)
    }

    fn lower_from_ast(name: String, ast: &TypedAST) -> Result<BLIRCompileUnit, PositionalError> {
        let mut program = BLIRCompileUnit {
            m_name: name,
            m_functions: Vec::<BLIRFunction>::new(),
            m_string_consts: HashSet::<String>::new(),
        };

        fn check_top_level_shape(expressions: &[TypedExpression]) -> Result<(), PositionalError> {
            for expr in expressions {
                match &**expr {
                    Expression::FunctionDefinition(_, _, _)
                    | Expression::UseStatement(_)
                    | Expression::Module(_, None) => continue,
                    Expression::Module(_, Some(body)) => check_top_level_shape(body)?,
                    _ => return Err(ir_gen_error(IRGenError::UnexpectedRootExpression, expr)),
                }
            }
            Ok(())
        }
        check_top_level_shape(&ast.m_expressions)?;

        for expr in &ast.m_expressions {
            visit_expression(expr, &mut |expr| match &expr.m_expression {
                Expression::FunctionDefinition(o_name, params, body) => {
                    let name = match o_name {
                        Some(name) => name.clone(),
                        None => format!("anonymous_function_{}", program.m_functions.len()),
                    };
                    program
                        .m_functions
                        .push(program.compile_function_definition(name, params, body)?);
                    Ok(())
                }
                Expression::Constant(Value::String(s)) => {
                    program.m_string_consts.insert(s.clone());
                    Ok(())
                }
                _ => Ok(()),
            })?;
        }

        Ok(program)
    }

    pub(crate) fn new(name: String, ast: &TypedAST) -> Result<BLIRCompileUnit, PositionalError> {
        Self::lower_from_ast(name, ast)
    }
}
