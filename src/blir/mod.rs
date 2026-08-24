mod blir_display;
mod blir_lowering;
mod blir_lowering_tests;
pub(crate) mod bloop_library;
pub(crate) mod test_support;
pub(crate) mod bloop_stdlib;

use std::collections::HashSet;

#[derive(Debug, PartialEq)]
pub(crate) enum BLIRConstant {
    Int64(i64),
    Float64(f64),
    String(String),
}

impl BLIRConstant {
    pub(crate) fn get_type(&self) -> BLIRType {
        match self {
            BLIRConstant::Int64(_) => BLIRType::Int64,
            BLIRConstant::Float64(_) => BLIRType::Float64,
            BLIRConstant::String(_) => BLIRType::Ptr,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BLIRRegister(pub(crate) String, pub(crate) BLIRType);

#[derive(Debug, Clone, PartialEq, Copy)]
pub(crate) enum BLIRArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub(crate) enum BLIRType {
    Ptr,
    Int64,
    Float64,
    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BLIRLabel(pub(crate) String);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BLIRCmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, PartialEq)]
pub(crate) enum BLIROperation {
    CallPrintf(BLIRRegister),
    StackAllocVariable(BLIRRegister),
    LoadConst(BLIRConstant, BLIRRegister),
    StoreFromConst(BLIRConstant, BLIRRegister),
    StoreFromReg(BLIRRegister, BLIRRegister),
    Load(BLIRRegister, BLIRRegister),
    MarkLabel(BLIRLabel),
    JumpToLabel(BLIRLabel),
    CondBranch(BLIRRegister, BLIRLabel, BLIRLabel), // if 0, jump to label 1, else jump to label 2
    ICmp(BLIRCmpOp, BLIRRegister, BLIRRegister, BLIRRegister), // dest = lhs <op> rhs
    IArith(BLIRArithOp, BLIRRegister, BLIRRegister, BLIRRegister), // dest = lhs <op> rhs
    CallFunction(String, Vec<BLIRRegister>, BLIRRegister), // fn_name, args, dest
    PowII(BLIRRegister, BLIRRegister, BLIRRegister), // dest = base ^ power, int base, int power
    PowFI(BLIRRegister, BLIRRegister, BLIRRegister), // dest = base ^ power, float base, int power
    PowFF(BLIRRegister, BLIRRegister, BLIRRegister), // dest = base ^ power, float base, float power
}

// Where a BLIROperation was lowered from, so the pretty-printer can
// annotate BLIR with the source line (and, given the source text
// separately, its actual text) each op came from.
#[derive(Debug, Clone)]
pub(crate) struct SourceLoc {
    pub(crate) m_line: usize,
    pub(crate) m_char: usize,
}

#[derive(Debug)]
pub(crate) struct BLIRFunction {
    pub(crate) m_name: String,
    pub(crate) m_params: Vec<BLIRRegister>,
    pub(crate) m_return: BLIRType,
    pub(crate) m_return_reg: Option<String>,
    pub(crate) m_body: Vec<BLIROperation>,
    pub(crate) m_locs: Vec<SourceLoc>,
}

#[derive(Debug)]
pub(crate) struct BLIRCompileUnit {
    pub(crate) m_name: String,
    pub(crate) m_string_consts: HashSet<String>,
    pub(crate) m_functions: Vec<BLIRFunction>,
}
