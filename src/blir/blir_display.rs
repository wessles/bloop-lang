use crate::blir::{
    BLIRArithOp, BLIRCmpOp, BLIRCompileUnit, BLIRConstant, BLIRFunction, BLIRLabel, BLIROperation,
    BLIRRegister, BLIRType,
};
use std::fmt::{Display, Formatter};

impl Display for BLIRConstant {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BLIRConstant::Int64(v) => write!(f, "{}", v),
            BLIRConstant::Float64(v) => write!(f, "{}", v),
            BLIRConstant::String(s) => write!(f, "{:?}", s),
        }
    }
}

impl Display for BLIRRegister {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "%{}", self.0)
    }
}

impl Display for BLIRArithOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BLIRArithOp::Add => "add",
            BLIRArithOp::Sub => "sub",
            BLIRArithOp::Mul => "mul",
            BLIRArithOp::Div => "div",
        };
        write!(f, "{}", s)
    }
}

impl Display for BLIRType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BLIRType::Ptr => write!(f, "ptr"),
            BLIRType::Int64 => write!(f, "i64"),
            BLIRType::Float64 => write!(f, "f64"),
            BLIRType::Void => write!(f, "void"),
        }
    }
}

impl Display for BLIRLabel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for BLIRCmpOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BLIRCmpOp::Lt => "lt",
            BLIRCmpOp::Le => "le",
            BLIRCmpOp::Gt => "gt",
            BLIRCmpOp::Ge => "ge",
            BLIRCmpOp::Eq => "eq",
            BLIRCmpOp::Ne => "ne",
        };
        write!(f, "{}", s)
    }
}

impl Display for BLIROperation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BLIROperation::CallPrintf(reg) => write!(f, "call printf({})", reg),
            BLIROperation::StackAllocVariable(reg) => write!(f, "{} {} = alloca", reg.1, reg),
            BLIROperation::LoadConst(c, dest) => write!(f, "{} {} = const {}", dest.1, dest, c),
            BLIROperation::StoreFromConst(c, dest) => {
                write!(f, "store {} {}, {}", dest.1, c, dest)
            }
            BLIROperation::StoreFromReg(src, dest) => write!(f, "store {}, {}", src, dest),
            BLIROperation::Load(from, dest) => write!(f, "{} {} = load {}", dest.1, dest, from),
            BLIROperation::MarkLabel(label) => write!(f, "{}:", label),
            BLIROperation::JumpToLabel(label) => write!(f, "jmp {}", label),
            BLIROperation::CondBranch(cond, if_false, if_true) => {
                write!(f, "br {}, true {}, false {}", cond, if_true, if_false)
            }
            BLIROperation::ICmp(op, dest, lhs, rhs) => {
                write!(f, "{} {} = icmp.{} {}, {}", dest.1, dest, op, lhs, rhs)
            }
            BLIROperation::IArith(op, dest, lhs, rhs) => {
                write!(f, "{} {} = {} {}, {}", dest.1, dest, op, lhs, rhs)
            }
            BLIROperation::CallFunction(name, args, dest) => {
                let args_str = args
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} {} = call {}({})", dest.1, dest, name, args_str)
            }
            BLIROperation::PowII(dest, base, power) => {
                write!(f, "{} {} = pow.ii {}, {}", dest.1, dest, base, power)
            }
            BLIROperation::PowFI(dest, base, power) => {
                write!(f, "{} {} = pow.fi {}, {}", dest.1, dest, base, power)
            }
            BLIROperation::PowFF(dest, base, power) => {
                write!(f, "{} {} = pow.ff {}, {}", dest.1, dest, base, power)
            }
        }
    }
}

impl BLIRCompileUnit {
    fn render(&self, source_lines: Option<&[&str]>) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        writeln!(out, "; program {}", self.m_name).unwrap();
        if !self.m_string_consts.is_empty() {
            let mut consts: Vec<_> = self.m_string_consts.iter().collect();
            consts.sort();
            writeln!(out, "; strings:").unwrap();
            for s in consts {
                writeln!(out, ";   {:?}", s).unwrap();
            }
        }
        for func in &self.m_functions {
            writeln!(out).unwrap();
            writeln!(out, "{}", func.render(source_lines)).unwrap();
        }
        out
    }

    // Renders the program with each op annotated with its actual source
    // line, resolved from `source` at print time -- lowering itself never
    // needs the raw source string, only (line, col) positions.
    pub(crate) fn to_pretty_string(&self, source: &str) -> String {
        let lines: Vec<&str> = source.lines().collect();
        self.render(Some(&lines))
    }
}

impl BLIRFunction {
    // Shared rendering for both plain `Display` and the source-annotated
    // pretty-printer. `source_lines`, when given, resolves each op's
    // SourceLoc to the actual source text; without it, only line:char is
    // shown. A run of consecutive ops sourced from the same line gets one
    // comment above the run (not one per op) in addition to the trailing
    // line:char every single op line gets on its own.
    fn render(&self, source_lines: Option<&[&str]>) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let params = self
            .m_params
            .iter()
            .map(|p| format!("{}: {}", p, p.1))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            out,
            "fn {}({}) -> {} {{",
            self.m_name, params, self.m_return
        )
        .unwrap();
        let mut last_line: Option<usize> = None;
        for (i, op) in self.m_body.iter().enumerate() {
            let loc = self.m_locs.get(i);
            if let Some(loc) = loc
                && last_line != Some(loc.m_line)
            {
                let comment =
                    match source_lines.and_then(|lines| lines.get(loc.m_line.saturating_sub(1))) {
                        Some(text) => {
                            format!("  ; {}:{}: {}", loc.m_line, loc.m_char, text.trim())
                        }
                        None => format!("  ; line {}:{}", loc.m_line, loc.m_char),
                    };
                writeln!(out, "{}", comment).unwrap();
                last_line = Some(loc.m_line);
            }
            let loc_suffix = loc
                .map(|loc| format!("  ; {}:{}", loc.m_line, loc.m_char))
                .unwrap_or_default();
            if matches!(op, BLIROperation::MarkLabel(_)) {
                writeln!(out, "{}{}", op, loc_suffix).unwrap();
            } else {
                writeln!(out, "  {}{}", op, loc_suffix).unwrap();
            }
        }
        if let Some(reg) = &self.m_return_reg {
            writeln!(out, "  ret %{}", reg).unwrap();
        }
        write!(out, "}}").unwrap();
        out
    }
}

impl Display for BLIRFunction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(None))
    }
}

impl Display for BLIRCompileUnit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(None))
    }
}
