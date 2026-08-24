// Runtime value held in a register. `Str` stands in for BLIR's `Ptr` type --
// the only thing a `Ptr`-typed register ever actually holds in this
// language is a string constant (BLIR has no other pointer-producing op),
// so there's no need for a real pointer/heap model to interpret it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InterpValue {
    Int64(i64),
    Float64(f64),
    Str(String),
}

// Mirrors `interpreter::BLIRInterpreter::print`'s per-variant formatting
// (a string prints raw, a float at 6 decimal places), for callers -- like
// the REPL -- that show a value back to the user rather than printing it.
impl std::fmt::Display for InterpValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpValue::Int64(v) => write!(f, "{}", v),
            InterpValue::Float64(v) => write!(f, "{:.6}", v),
            InterpValue::Str(s) => write!(f, "{}", s),
        }
    }
}

impl InterpValue {
    pub(crate) fn is_nonzero(&self) -> bool {
        match self {
            InterpValue::Int64(v) => *v != 0,
            InterpValue::Float64(v) => *v != 0.0,
            InterpValue::Str(_) => unreachable!("a branch condition is always i64-typed"),
        }
    }

    pub(crate) fn as_int(&self) -> i64 {
        match self {
            InterpValue::Int64(v) => *v,
            _ => unreachable!("expected an i64-typed register"),
        }
    }

    pub(crate) fn as_float(&self) -> f64 {
        match self {
            InterpValue::Float64(v) => *v,
            _ => unreachable!("expected an f64-typed register"),
        }
    }
}
