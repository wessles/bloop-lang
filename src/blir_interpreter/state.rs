use crate::blir::{
    BLIRArithOp, BLIRCmpOp, BLIRConstant, BLIRFunction, BLIRLabel, BLIROperation,
};
use crate::blir_interpreter::interpreter::InterpretError;
use crate::blir_interpreter::value::InterpValue;
use std::collections::HashMap;

/// One call frame's register file (owned, so no lifetime tie to the
/// program data being interpreted), plus the shared, immutable function
/// table needed to resolve `CallFunction`. `execute` is function-agnostic:
/// it just walks whatever `BLIROperation` stream it's given against this
/// state's own registers, so a caller can run a stream that was never
/// linked as a callable function (see `BLIRInterpreter::execute_ops`)
/// against registers that outlive any single call -- since BLIR names a
/// `let`-bound register after the variable itself (not qualified by an
/// enclosing function), successive streams sharing one owned register map
/// keeps such a variable bound across them, like a `let` staying bound for
/// the rest of an ordinary function body.
///
/// `call_function` is the one place that knows about `BLIRFunction`: the
/// function-scope entry point, starting a fresh state, binding
/// `m_params`, running `m_body`, then reading back `m_return_reg`. Stack
/// slots and SSA registers are separate namespaces in the LLVM backend;
/// here both just live in the same register map keyed by name, since
/// BLIR's naming scheme never lets the two collide.
pub(crate) struct BLIRInterpreterState<'f> {
    m_registers: HashMap<String, InterpValue>,
    m_functions: &'f HashMap<&'f str, &'f BLIRFunction>,
}

impl<'f> BLIRInterpreterState<'f> {
    pub(crate) fn new(functions: &'f HashMap<&'f str, &'f BLIRFunction>) -> Self {
        Self::with_registers(HashMap::new(), functions)
    }

    pub(crate) fn with_registers(
        registers: HashMap<String, InterpValue>,
        functions: &'f HashMap<&'f str, &'f BLIRFunction>,
    ) -> Self {
        Self {
            m_registers: registers,
            m_functions: functions,
        }
    }

    pub(crate) fn into_registers(self) -> HashMap<String, InterpValue> {
        self.m_registers
    }

    pub(crate) fn bind(&mut self, name: &str, value: InterpValue) {
        self.m_registers.insert(name.to_string(), value);
    }

    // A `let` inside a conditional branch that never runs still type-checks
    // (BLIR only has function-level scoping) but never gets its register
    // bound, so a later read of it is a real, reachable runtime condition --
    // hence a graceful error here rather than a panic.
    pub(crate) fn get(&self, name: &str) -> Result<&InterpValue, InterpretError> {
        self.m_registers
            .get(name)
            .ok_or_else(|| InterpretError::UnboundRegister(name.to_string()))
    }

    // Runs one function invocation in a fresh scope: `args` bound
    // positionally to `function.m_params`, `m_body` run to completion, then
    // `m_return_reg` read back if any. Recursion is just this calling
    // itself/another on Rust's own call stack -- no separate interpreter
    // call stack is needed since BLIR is already flattened into registers.
    pub(crate) fn call_function(
        function: &'f BLIRFunction,
        args: Vec<InterpValue>,
        functions: &'f HashMap<&'f str, &'f BLIRFunction>,
        output: &mut String,
    ) -> Result<Option<InterpValue>, InterpretError> {
        let mut state = BLIRInterpreterState::new(functions);
        for (param, arg) in function.m_params.iter().zip(args) {
            state.bind(&param.0, arg);
        }
        state.execute(&function.m_body, output)?;
        function
            .m_return_reg
            .as_ref()
            .map(|reg| state.get(reg).cloned())
            .transpose()
    }

    /// Walks `ops` to completion against this state's registers, jumping
    /// around via a label -> index map built from `ops` itself.
    pub(crate) fn execute(
        &mut self,
        ops: &[BLIROperation],
        output: &mut String,
    ) -> Result<(), InterpretError> {
        let labels: HashMap<&str, usize> = ops
            .iter()
            .enumerate()
            .filter_map(|(i, op)| match op {
                BLIROperation::MarkLabel(BLIRLabel(name)) => Some((name.as_str(), i)),
                _ => None,
            })
            .collect();

        let mut pc = 0;
        while pc < ops.len() {
            match &ops[pc] {
                BLIROperation::LoadConst(constant, dest)
                | BLIROperation::StoreFromConst(constant, dest) => {
                    self.bind(&dest.0, Self::constant_value(constant));
                }
                BLIROperation::StoreFromReg(src, dest) => {
                    let value = self.get(src.0.as_str())?.clone();
                    self.bind(&dest.0, value);
                }
                BLIROperation::Load(from, dest) => {
                    let value = self.get(from.0.as_str())?.clone();
                    self.bind(&dest.0, value);
                }
                BLIROperation::StackAllocVariable(_) | BLIROperation::MarkLabel(_) => {}
                BLIROperation::JumpToLabel(BLIRLabel(name)) => {
                    pc = labels[name.as_str()];
                    continue;
                }
                BLIROperation::CondBranch(cond, BLIRLabel(if_false), BLIRLabel(if_true)) => {
                    let target = if self.get(cond.0.as_str())?.is_nonzero() {
                        if_true
                    } else {
                        if_false
                    };
                    pc = labels[target.as_str()];
                    continue;
                }
                BLIROperation::ICmp(op, dest, lhs, rhs) => {
                    let result =
                        Self::compare(op, self.get(lhs.0.as_str())?, self.get(rhs.0.as_str())?);
                    self.bind(&dest.0, InterpValue::Int64(result as i64));
                }
                BLIROperation::IArith(op, dest, lhs, rhs) => {
                    let result =
                        Self::arith(op, self.get(lhs.0.as_str())?, self.get(rhs.0.as_str())?)?;
                    self.bind(&dest.0, result);
                }
                BLIROperation::CallPrintf(reg) => {
                    Self::print(self.get(reg.0.as_str())?, output);
                }
                BLIROperation::CallFunction(fn_name, arg_regs, dest) => {
                    let callee = self
                        .m_functions
                        .get(fn_name.as_str())
                        .copied()
                        .ok_or_else(|| InterpretError::UndefinedFunction(fn_name.clone()))?;
                    let arg_values = arg_regs
                        .iter()
                        .map(|reg| self.get(reg.0.as_str()).cloned())
                        .collect::<Result<Vec<_>, _>>()?;
                    if let Some(value) =
                        Self::call_function(callee, arg_values, self.m_functions, output)?
                    {
                        self.bind(&dest.0, value);
                    }
                }
                BLIROperation::PowII(dest, base, power) => {
                    let result = Self::pow_ii(
                        self.get(base.0.as_str())?.as_int(),
                        self.get(power.0.as_str())?.as_int(),
                    )?;
                    self.bind(&dest.0, InterpValue::Int64(result));
                }
                BLIROperation::PowFI(dest, base, power) => {
                    let result = Self::pow_fi(
                        self.get(base.0.as_str())?.as_float(),
                        self.get(power.0.as_str())?.as_int(),
                    );
                    self.bind(&dest.0, InterpValue::Float64(result));
                }
                BLIROperation::PowFF(dest, base, power) => {
                    let power_int = self.get(power.0.as_str())?.as_float() as i64;
                    let result =
                        Self::pow_fi(self.get(base.0.as_str())?.as_float(), power_int);
                    self.bind(&dest.0, InterpValue::Float64(result));
                }
            }
            pc += 1;
        }

        Ok(())
    }

    fn constant_value(constant: &BLIRConstant) -> InterpValue {
        match constant {
            BLIRConstant::Int64(v) => InterpValue::Int64(*v),
            BLIRConstant::Float64(v) => InterpValue::Float64(*v),
            BLIRConstant::String(s) => InterpValue::Str(s.clone()),
        }
    }

    // The type checker unifies both operands of a comparison/arithmetic op
    // to the same type before BLIR lowering ever runs, so only the
    // same-type pairs below are reachable -- mirrors tree_evaluator.rs's
    // per-type dispatch, minus the cross int/float coercion it needs
    // (evaluator sees pre-unification values in some paths BLIR doesn't).
    fn compare(op: &BLIRCmpOp, lhs: &InterpValue, rhs: &InterpValue) -> bool {
        match (lhs, rhs) {
            (InterpValue::Int64(a), InterpValue::Int64(b)) => match op {
                BLIRCmpOp::Lt => a < b,
                BLIRCmpOp::Le => a <= b,
                BLIRCmpOp::Gt => a > b,
                BLIRCmpOp::Ge => a >= b,
                BLIRCmpOp::Eq => a == b,
                BLIRCmpOp::Ne => a != b,
            },
            (InterpValue::Float64(a), InterpValue::Float64(b)) => match op {
                BLIRCmpOp::Lt => a < b,
                BLIRCmpOp::Le => a <= b,
                BLIRCmpOp::Gt => a > b,
                BLIRCmpOp::Ge => a >= b,
                BLIRCmpOp::Eq => a == b,
                BLIRCmpOp::Ne => a != b,
            },
            _ => unreachable!("type checker guarantees ICmp operands share a type"),
        }
    }

    // Division-by-zero is a graceful `DivisionByZero` error rather than a
    // trap, matching `ast::tree_evaluator::evaluate_binary_op`'s explicit zero
    // check -- LLVM's own lowering has no such check (an actual zero
    // divisor there is a runtime crash), but that's an LLVM backend gap,
    // not a BLIR-level limitation, so the evaluator's graceful behavior is
    // what this interpreter reproduces.
    fn arith(
        op: &BLIRArithOp,
        lhs: &InterpValue,
        rhs: &InterpValue,
    ) -> Result<InterpValue, InterpretError> {
        match (lhs, rhs) {
            (InterpValue::Int64(a), InterpValue::Int64(b)) => match op {
                BLIRArithOp::Add => Ok(InterpValue::Int64(a + b)),
                BLIRArithOp::Sub => Ok(InterpValue::Int64(a - b)),
                BLIRArithOp::Mul => Ok(InterpValue::Int64(a * b)),
                BLIRArithOp::Div if *b == 0 => Err(InterpretError::DivisionByZero),
                BLIRArithOp::Div => Ok(InterpValue::Int64(a / b)),
            },
            (InterpValue::Float64(a), InterpValue::Float64(b)) => match op {
                BLIRArithOp::Add => Ok(InterpValue::Float64(a + b)),
                BLIRArithOp::Sub => Ok(InterpValue::Float64(a - b)),
                BLIRArithOp::Mul => Ok(InterpValue::Float64(a * b)),
                BLIRArithOp::Div if *b == 0.0 => Err(InterpretError::DivisionByZero),
                BLIRArithOp::Div => Ok(InterpValue::Float64(a / b)),
            },
            _ => unreachable!("type checker guarantees IArith operands share a type"),
        }
    }

    // Matches tree_evaluator.rs's Exponent arm exactly: `0 ^ negative` is a
    // division by zero (int only -- IEEE float division by zero is a
    // silent `inf`/`-inf`, which tree_evaluator.rs lets through unchecked too).
    fn pow_ii(base: i64, power: i64) -> Result<i64, InterpretError> {
        if power >= 0 {
            Ok(base.pow(power as u32))
        } else if base == 0 {
            Err(InterpretError::DivisionByZero)
        } else {
            Ok(1 / base.pow((-power) as u32))
        }
    }

    fn pow_fi(base: f64, power: i64) -> f64 {
        if power >= 0 {
            base.powi(power as i32)
        } else {
            1.0 / base.powi((-power) as i32)
        }
    }

    fn print(value: &InterpValue, output: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(output, "{}", value);
    }
}
